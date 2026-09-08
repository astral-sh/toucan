use toucan_semantic::checked::{ImmediateStage, X86Intrinsic};
use toucan_semantic::{
    Analysis, AnalysisOptions, Error, Type, TypeKind, analyze_with_profile, evaluate_integer,
};
use toucan_target::{Compiler, CompilerProfile, Target};

fn parity(source: &str, profile: CompilerProfile) -> Result<Analysis, Error> {
    let plain = analyze_with_profile(source, profile, &AnalysisOptions::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&plain, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => {
            assert_eq!(a.offset, b.offset);
            assert_eq!(a.message, b.message);
        }
        _ => panic!("{profile:?}: plain={plain:?}; retained={retained:?}"),
    }
    retained
}

const LAYOUTS: &str = r#"
    enum E { E0, E1 };
    struct EnumMember { char lead; enum E value; char tail; };
    typedef unsigned char Aligned __attribute__((aligned(8)));
    struct Bits { Aligned a:1; Aligned b:1; char tail; };
    struct Nested { char lead; struct Bits members[2]; char tail; };
    struct Three { char a[3]; };
    typedef _Atomic(struct Three) AtomicThree;
    typedef _Atomic(struct Bits) AtomicBits;
"#;

#[test]
fn linux_inferred_atomic_types_follow_the_selected_compiler() {
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        for compiler in [Compiler::Gnu, Compiler::Clang] {
            let profile = CompilerProfile::new(target, compiler).unwrap();
            let source = "int outer; void f(void){__auto_type outer=outer;}";
            assert_eq!(parity(source, profile).is_ok(), compiler == Compiler::Gnu);
            let source = "void f(void){_Atomic int a=0;__auto_type b=a;_Static_assert(_Generic(&b,_Atomic(int)*:1,default:0),\"atomic inference\");}";
            assert_eq!(parity(source, profile).is_ok(), compiler == Compiler::Clang);
            parity(
                "void f(void){_Atomic(int*) p=(_Atomic(int*))0;__auto_type q=(_Atomic(int*))0;}",
                profile,
            )
            .unwrap();
        }
    }
}

#[test]
fn linux_layout_and_evaluation_keep_the_selected_compiler_recursively() {
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        for compiler in [Compiler::Gnu, Compiler::Clang] {
            let profile = CompilerProfile::new(target, compiler).unwrap();
            let analysis = parity(LAYOUTS, profile).unwrap();
            let unit = analysis.unit();
            let clang = compiler == Compiler::Clang;
            for (expression, expected) in [
                ("sizeof(struct EnumMember)", 12),
                ("_Alignof(struct EnumMember)", 4),
                ("__builtin_offsetof(struct EnumMember,tail)", 8),
                ("sizeof(struct Nested)", if clang { 32 } else { 48 }),
                (
                    "__builtin_offsetof(struct Nested,tail)",
                    if clang { 24 } else { 40 },
                ),
                ("sizeof(struct Bits)", if clang { 8 } else { 16 }),
                (
                    "__builtin_offsetof(struct Bits,tail)",
                    if clang { 1 } else { 9 },
                ),
                ("sizeof(AtomicThree)", if clang { 4 } else { 3 }),
                ("_Alignof(AtomicThree)", if clang { 4 } else { 1 }),
                ("sizeof(AtomicBits)", if clang { 8 } else { 16 }),
            ] {
                assert_eq!(
                    evaluate_integer(unit, expression).unwrap().value,
                    expected,
                    "{profile:?}: {expression}"
                );
            }
            let bits = unit
                .records
                .iter()
                .position(|r| r.name.as_deref() == Some("Bits"))
                .unwrap();
            let layout = unit.layout(&Type::new(TypeKind::Record(bits))).unwrap();
            assert_eq!(
                layout.fields[1].as_ref().unwrap().offset_bits,
                if clang { 1 } else { 64 }
            );
            assert_eq!(unit.profile().unwrap(), profile);
        }
    }
}

#[test]
fn compiler_rules_do_not_change_linux_scalar_types_or_float_formats() {
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        for compiler in [Compiler::Gnu, Compiler::Clang] {
            let profile = CompilerProfile::new(target, compiler).unwrap();
            let source = "enum E { A = 0, B = 1ULL << 40 }; _Static_assert(_Generic(__builtin_bswap64(0), unsigned long:1,default:0),\"Linux bswap type\"); _Static_assert(sizeof(long double)==16,\"Linux long double\");";
            let analysis = parity(source, profile).unwrap();
            let unit = analysis.unit();
            let a = evaluate_integer(unit, "A").unwrap();
            assert_eq!(
                (a.bits, a.signed, a.rank),
                if compiler == Compiler::Gnu {
                    (64, false, 4)
                } else {
                    (32, true, 3)
                }
            );
            assert_eq!(evaluate_integer(unit, "sizeof(L'A')").unwrap().value, 4);
            let format = if matches!(
                target,
                Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
            ) {
                "0x1p-100L + 1.0L != 1.0L"
            } else {
                "0x1p-100L + 1.0L == 1.0L"
            };
            assert!(
                matches!(toucan_semantic::evaluate_arithmetic(unit,format).unwrap(), toucan_semantic::ArithmeticConstant::Integer(value) if value.value == 1)
            );
        }
    }
}

#[test]
fn clang_linux_uses_clang_constraints_and_retained_builtin_signatures() {
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        for compiler in [Compiler::Gnu, Compiler::Clang] {
            let profile = CompilerProfile::new(target, compiler).unwrap();
            for (source, clang_valid) in [
                ("__thread static int x;", true),
                ("void f(register void);", true),
                ("enum { C=L'ab' };", false),
                ("struct Opaque; _Atomic(struct Opaque) *p;", false),
            ] {
                assert_eq!(
                    parity(source, profile).is_ok(),
                    if compiler == Compiler::Clang {
                        clang_valid
                    } else {
                        !clang_valid
                    },
                    "{profile:?}: {source}"
                );
            }
            if matches!(
                target,
                Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
            ) {
                assert_eq!(
                    parity("typedef __Float32x4_t Native;", profile).is_ok(),
                    compiler == Compiler::Gnu
                );
            }
        }
    }
    let target = Target::X86_64UnknownLinuxGnu;
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(target, compiler).unwrap();
        let stage = X86Intrinsic::VecExtV2si.immediate_constraints_with_profile(profile)[0].stage();
        assert_eq!(
            stage,
            if compiler == Compiler::Gnu {
                ImmediateStage::AfterInlining
            } else {
                ImmediateStage::Frontend
            }
        );
        let source = "typedef int V __attribute__((vector_size(8))); int f(V v,int n){ return __builtin_ia32_vec_ext_v2si(v,n); }";
        assert_eq!(parity(source, profile).is_ok(), compiler == Compiler::Gnu);
        let source = "long double f(_Atomic(long double)*p){return __c11_atomic_fetch_add(p,1,0);}";
        assert!(parity(source, profile).is_err());
        assert!(
            parity(
                "int __attribute__((ms_abi)) f(void); int __attribute__((cdecl)) f(void);",
                profile
            )
            .is_err()
        );
    }
}

#[test]
fn clang_forward_tag_alignment_includes_the_windows_profile() {
    for profile in CompilerProfile::ALL {
        let analysis = parity(
            "struct __attribute__((aligned(16))) Forward; struct Forward { char c; }; struct __attribute__((packed)) PackedForward; struct PackedForward { char c; int n; };",
            profile,
        )
        .unwrap();
        assert_eq!(
            evaluate_integer(analysis.unit(), "sizeof(struct PackedForward)")
                .unwrap()
                .value,
            if profile.compiler() == Compiler::Clang {
                5
            } else {
                8
            }
        );
        assert_eq!(
            evaluate_integer(analysis.unit(), "_Alignof(struct Forward)")
                .unwrap()
                .value,
            if profile.compiler() == Compiler::Clang {
                16
            } else {
                1
            },
            "{profile:?}"
        );
    }
}

#[test]
fn manually_changed_translation_units_reject_unsupported_profiles() {
    let mut unit = parity("int x;", CompilerProfile::ALL[0])
        .unwrap()
        .into_unit();
    unit.target = Target::X86_64AppleDarwin;
    assert!(unit.profile().is_err());
    assert!(unit.layout(&unit.declarations[0].ty).is_err());
    assert!(evaluate_integer(&unit, "1").is_err());
}

#[test]
#[ignore = "requires native GNU GCC, Clang, and Clang cross-target backends"]
fn same_target_layouts_and_named_default_correction_match_compilers() {
    let directory = tempfile::tempdir().unwrap();
    for profile in CompilerProfile::ALL {
        let mut source = String::from(
            "struct __attribute__((aligned(16))) Forward; struct Forward { char c; }; struct __attribute__((packed)) PackedForward; struct PackedForward { char c; int n; };\n",
        );
        if matches!(
            profile.target(),
            Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        ) {
            source.push_str(LAYOUTS);
        }
        let analysis = parity(&source, profile).unwrap();
        let expressions = if matches!(
            profile.target(),
            Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        ) {
            vec![
                "_Alignof(struct Forward)",
                "sizeof(struct PackedForward)",
                "sizeof(struct EnumMember)",
                "_Alignof(struct EnumMember)",
                "__builtin_offsetof(struct EnumMember,tail)",
                "sizeof(struct Nested)",
                "sizeof(struct Bits)",
                "__builtin_offsetof(struct Bits,tail)",
                "sizeof(AtomicThree)",
                "sizeof(AtomicBits)",
            ]
        } else {
            vec!["_Alignof(struct Forward)", "sizeof(struct PackedForward)"]
        };
        for expression in expressions {
            source.push_str(&format!(
                "_Static_assert(({expression})=={},\"{expression}\");\n",
                evaluate_integer(analysis.unit(), expression).unwrap().value
            ));
        }
        let input = directory.path().join("profile.c");
        std::fs::write(&input, source).unwrap();
        let mut command = if profile.compiler() == Compiler::Clang {
            let mut command = std::process::Command::new("clang");
            command.args(["-target", profile.target().triple()]);
            command
        } else if (matches!(
            profile.target(),
            Target::X86_64UnknownLinuxGnu | Target::X86_64UnknownLinuxMusl
        ) && cfg!(all(target_os = "linux", target_arch = "x86_64")))
            || (matches!(
                profile.target(),
                Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
            ) && cfg!(all(target_os = "linux", target_arch = "aarch64")))
        {
            std::process::Command::new(std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()))
        } else {
            continue;
        };
        let output = command
            .args(["-std=gnu11", "-fsyntax-only"])
            .arg(input)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{profile:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

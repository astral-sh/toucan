use toucan_semantic::{
    Analysis, AnalysisOptions, ArithmeticConstant, Error, analyze_with_profile, evaluate_vector,
};
use toucan_target::{Compiler, CompilerProfile, LanguageMode};

const TYPES: &str = r#"
typedef int I4 __attribute__((vector_size(16)));
typedef unsigned U4 __attribute__((vector_size(16)));
typedef float F4 __attribute__((vector_size(16)));
typedef double D2 __attribute__((vector_size(16)));
typedef signed char C4 __attribute__((vector_size(4)));
typedef unsigned char B4 __attribute__((vector_size(4)));
typedef short S4 __attribute__((vector_size(8)));
typedef I4 A4 __attribute__((aligned(32)));
extern I4 runtime;
extern int scalar;
I4 input(void);
"#;

struct Case {
    name: &'static str,
    ty: &'static str,
    expression: &'static str,
    bits: &'static [u128],
    gnu_static: bool,
    clang_static: bool,
}

const CASES: &[Case] = &[
    Case {
        name: "zero_fill",
        ty: "I4",
        expression: "(I4){1,2}",
        bits: &[1, 2, 0, 0],
        gnu_static: true,
        clang_static: true,
    },
    Case {
        name: "unary_plus",
        ty: "C4",
        expression: "+(C4){1,-2,3,-4}",
        bits: &[1, 254, 3, 252],
        gnu_static: true,
        clang_static: true,
    },
    Case {
        name: "unary_minus",
        ty: "I4",
        expression: "-(I4){1,-2,3,-4}",
        bits: &[4294967295, 2, 4294967293, 4],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "complement",
        ty: "B4",
        expression: "~(B4){1,2,3,4}",
        bits: &[254, 253, 252, 251],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "sum",
        ty: "I4",
        expression: "(I4){1,2,3,4}+(I4){5,6,7,8}",
        bits: &[6, 8, 10, 12],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "unsigned_wrap",
        ty: "B4",
        expression: "(B4){255,0,128,2}+(B4){1,255,128,254}",
        bits: &[0, 255, 0, 0],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "scalar_splat",
        ty: "C4",
        expression: "5-(C4){1,2,3,4}",
        bits: &[4, 3, 2, 1],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "multiply",
        ty: "S4",
        expression: "(S4){1,-2,3,-4}*(S4){5,6,7,8}",
        bits: &[5, 65524, 21, 65504],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "divide",
        ty: "I4",
        expression: "(I4){-17,17,6,-7}/(I4){4,-4,3,-2}",
        bits: &[4294967292, 4294967292, 2, 3],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "remainder",
        ty: "I4",
        expression: "(I4){-17,17,6,-7}%(I4){4,-4,3,-2}",
        bits: &[4294967295, 1, 0, 4294967295],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "bitwise",
        ty: "B4",
        expression: "((B4){1,3,5,7}&(B4){2,3,4,5})^(B4){0,1,2,3}",
        bits: &[0, 2, 6, 6],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "shift_lanes",
        ty: "B4",
        expression: "(B4){1,2,3,4}<<(B4){1,2,3,4}",
        bits: &[2, 8, 24, 64],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "signed_shift",
        ty: "C4",
        expression: "(C4){-1,-8,16,64}>>2L",
        bits: &[255, 254, 4, 16],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "comparison",
        ty: "I4",
        expression: "(U4){4294967295,1,2,3}>(U4){0,1,3,2}",
        bits: &[4294967295, 0, 0, 4294967295],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "narrow_mask",
        ty: "__typeof__((B4){0} != (B4){0})",
        expression: "(B4){255,1,2,3}!=(B4){0,1,3,2}",
        bits: &[255, 0, 255, 255],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "float_rounding",
        ty: "F4",
        expression: "(F4){16777216.0f,1.5f,-0.0f,0x1p-149f}+(F4){1.0f,2.25f,-0.0f,0x1p-149f}",
        bits: &[0x4b800000, 0x40700000, 0x80000000, 2],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "double_negate",
        ty: "D2",
        expression: "-(D2){0.0,-0.0}",
        bits: &[0x8000000000000000, 0],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "float_compare",
        ty: "I4",
        expression: "(F4){1.0f,2.0f,3.0f,4.0f}<=(F4){0.0f,2.0f,4.0f,1.0f}",
        bits: &[0, 4294967295, 4294967295, 0],
        gnu_static: false,
        clang_static: true,
    },
    Case {
        name: "conditional",
        ty: "I4",
        expression: "1?(I4){1,2,3,4}:runtime",
        bits: &[1, 2, 3, 4],
        gnu_static: true,
        clang_static: true,
    },
    Case {
        name: "choose",
        ty: "I4",
        expression: "__builtin_choose_expr(0,input(),(I4){1,2,3,4})",
        bits: &[1, 2, 3, 4],
        gnu_static: true,
        clang_static: true,
    },
    Case {
        name: "generic",
        ty: "I4",
        expression: "_Generic(1,int:(I4){1,2,3,4},default:input())",
        bits: &[1, 2, 3, 4],
        gnu_static: true,
        clang_static: true,
    },
    Case {
        name: "identity_conversion",
        ty: "I4",
        expression: "__builtin_convertvector((I4){1,2,3,4},I4)",
        bits: &[1, 2, 3, 4],
        gnu_static: true,
        clang_static: false,
    },
    Case {
        name: "numeric_conversion",
        ty: "F4",
        expression: "__builtin_convertvector((I4){1,-2,16777217,0},F4)",
        bits: &[0x3f800000, 0xc0000000, 0x4b800000, 0],
        gnu_static: false,
        clang_static: false,
    },
    Case {
        name: "truncate_conversion",
        ty: "I4",
        expression: "__builtin_convertvector((F4){1.5f,-2.75f,3.0f,0.5f},I4)",
        bits: &[1, 4294967294, 3, 0],
        gnu_static: false,
        clang_static: false,
    },
    Case {
        name: "narrow_conversion",
        ty: "B4",
        expression: "__builtin_convertvector((I4){256,-1,128,257},B4)",
        bits: &[0, 255, 128, 1],
        gnu_static: false,
        clang_static: false,
    },
];

fn check(source: &str, profile: CompilerProfile) -> Result<Analysis, Error> {
    let ordinary = analyze_with_profile(source, profile, &AnalysisOptions::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..AnalysisOptions::default()
        },
    );
    match (&ordinary, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => assert_eq!((a.offset, &a.message), (b.offset, &b.message)),
        _ => panic!("ordinary/retained mismatch: {ordinary:?}, {retained:?}"),
    }
    retained
}

fn bits(value: ArithmeticConstant) -> u128 {
    match value {
        ArithmeticConstant::Integer(value) => value.value,
        ArithmeticConstant::Floating(value) => value.to_bits(),
        ArithmeticConstant::Complex(_) => panic!("complex vector lane"),
    }
}

#[test]
fn vector_values_and_static_admissibility_are_separate() {
    for profile in CompilerProfile::ALL {
        for mode in [LanguageMode::C11, LanguageMode::Gnu11] {
            let profile = profile.with_language_mode(mode);
            let unit = check(TYPES, profile).unwrap();
            for case in CASES {
                let value = evaluate_vector(unit.unit(), case.expression)
                    .unwrap_or_else(|e| panic!("{} {profile:?}: {e}", case.name));
                assert_eq!(
                    value.lanes().iter().copied().map(bits).collect::<Vec<_>>(),
                    case.bits,
                    "{} {profile:?}",
                    case.name
                );
                let source = format!("{TYPES}\n{} result={};", case.ty, case.expression);
                let result = check(&source, profile);
                let expected = if profile.compiler() == Compiler::Gnu {
                    case.gnu_static
                } else {
                    case.clang_static
                };
                assert_eq!(
                    result.is_ok(),
                    expected,
                    "{} {profile:?}: {result:?}",
                    case.name
                );
            }
        }
    }
}

#[test]
fn invalid_lanes_effects_and_unimplemented_forms_remain_diagnostics() {
    for profile in CompilerProfile::ALL {
        let unit = check(TYPES, profile).unwrap();
        for expression in [
            "runtime",
            "input()",
            "(I4){scalar,0,0,0}",
            "(I4){scalar++,0,0,0}",
            "(I4){1,2,3,4}/(I4){0,1,1,1}",
            "(I4){1,2,3,4}<<(I4){32,0,0,0}",
            "(B4){1,2,3,4}<<256",
            "(B4){1,2,3,4}<<-1",
            "(B4){1,2,3,4}<<8",
            "(I4){2147483647,0,0,0}+(I4){1,0,0,0}",
            "-(C4){-128,0,0,0}",
            "(C4){-128,0,0,0}%(C4){-1,1,1,1}",
            "(C4){127,0,0,0}+(C4){1,0,0,0}",
            "(I4){-2147483648,0,0,0}/(I4){-1,1,1,1}",
            "__builtin_convertvector((F4){0x1p31f,0,0,0},I4)",
            "__builtin_convertvector((I4){1,2,3,4},D2)",
            "(F4)(I4){1065353216,0,0,0}",
            "__builtin_shufflevector((I4){1,2,3,4},(I4){0},-1,1,2,3)",
            "(scalar++,(I4){1,2,3,4})",
            "(I4){[1]=2}",
            "(I4){{1},2,3,4}",
            "(I4){1,2,3,4,5}",
        ] {
            assert!(
                evaluate_vector(unit.unit(), expression).is_err(),
                "{profile:?}: {expression}"
            );
        }
        let error = check(
            &format!("{TYPES}\nI4 result=1?(I4){{1,2,3,4}}:unknown;"),
            profile,
        )
        .unwrap_err();
        assert!(error.message.contains("unknown"));
        let value = evaluate_vector(unit.unit(), "(A4){1,2,3,4}").unwrap();
        assert_eq!(value.ty().alignment_bytes(), 32);
        let value = evaluate_vector(
            unit.unit(),
            "__builtin_convertvector((I4){1,2,3,4},const A4)",
        )
        .unwrap();
        assert_eq!(value.ty().alignment_bytes(), 32);
        assert!(value.ty().qualifiers().is_const);
    }
}

#[test]
fn extended_lanes_round_in_their_target_format() {
    use toucan_semantic::FloatingFormat;
    use toucan_target::Target;
    for profile in CompilerProfile::ALL.into_iter().filter(|profile| {
        // This fixture declares __int128 vectors, which Clang rejects on
        // both 32-bit profiles. ARMv7's supported vectors are checked below.
        !matches!(
            profile.target(),
            Target::I686UnknownLinuxGnu | Target::Armv7UnknownLinuxGnueabihf
        )
    }) {
        let prefix = "typedef _Float16 H4 __attribute__((vector_size(8))); typedef __bf16 B4 __attribute__((vector_size(8))); typedef float F4 __attribute__((vector_size(16))); typedef long double L __attribute__((vector_size(16))); typedef __int128 I __attribute__((vector_size(16))); typedef unsigned __int128 U __attribute__((vector_size(16)));";
        let analysis = check(prefix, profile).unwrap();
        let unit = analysis.unit();
        let half = evaluate_vector(
            unit,
            "__builtin_convertvector((F4){1.00048828125f,0x1p-24f,-0.0f,65504.0f},H4)",
        )
        .unwrap();
        assert_eq!(
            half.lanes().iter().copied().map(bits).collect::<Vec<_>>(),
            [0x3c00, 1, 0x8000, 0x7bff]
        );
        let bf = evaluate_vector(
            unit,
            "__builtin_convertvector((F4){1.00390625f,1.01171875f,-0.0f,0x1p-133f},B4)",
        )
        .unwrap();
        assert_eq!(
            bf.lanes().iter().copied().map(bits).collect::<Vec<_>>(),
            [0x3f80, 0x3f82, 0x8000, 1]
        );
        for (ty, expression) in [
            ("H4", "(H4){1,2,3,4}+(H4){0.5,0.25,0.125,0.0625}"),
            ("B4", "(B4){1,2,3,4}+(B4){0.5,0.25,0.125,0.0625}"),
        ] {
            let result = evaluate_vector(unit, expression);
            assert_eq!(
                result.is_ok(),
                profile.compiler() == Compiler::Clang,
                "{profile:?} {ty}: {result:?}"
            );
            let initialized = check(&format!("{prefix} {ty} v={expression};"), profile);
            assert_eq!(
                initialized.is_ok(),
                profile.compiler() == Compiler::Clang,
                "{profile:?} {ty}: {initialized:?}"
            );
        }
        let long_double = evaluate_vector(unit, "(L){1.0L}+(L){0.5L}").unwrap();
        let ArithmeticConstant::Floating(lane) = long_double.lanes()[0] else {
            panic!()
        };
        let (format, expected) = match profile.target() {
            Target::X86_64UnknownLinuxGnu
            | Target::X86_64UnknownLinuxMusl
            | Target::I686UnknownLinuxGnu
            | Target::X86_64AppleDarwin => (FloatingFormat::X87, 0x3fff_c000000000000000),
            Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl => (
                FloatingFormat::Binary128,
                (0x3fffu128 << 112) | (1u128 << 111),
            ),
            Target::Aarch64AppleDarwin
            | Target::X86_64PcWindowsMsvc
            | Target::Aarch64PcWindowsMsvc
            | Target::Armv7UnknownLinuxGnueabihf => (FloatingFormat::Binary64, 0x3ff8000000000000),
        };
        assert_eq!(lane.format(), format);
        assert_eq!(lane.to_bits(), expected);
        assert_eq!(long_double.ty().size_bytes(), 16);
        assert_eq!(
            bits(
                evaluate_vector(unit, "(U){(unsigned __int128)-1}+(U){1}")
                    .unwrap()
                    .lanes()[0]
            ),
            0
        );
        assert_eq!(
            bits(evaluate_vector(unit, "(I){1}+(I){2}").unwrap().lanes()[0]),
            3
        );
        assert!(evaluate_vector(unit, "(I){-((__int128)1<<126)*2}/(I){-1}").is_err());
        if profile.target().is_x86_64() && profile.target().is_linux() {
            let quad = check(
                "typedef __float128 Q __attribute__((vector_size(16)));",
                profile,
            )
            .unwrap();
            let value = evaluate_vector(quad.unit(), "(Q){1.0Q}+(Q){0.5Q}").unwrap();
            assert_eq!(bits(value.lanes()[0]), (0x3fffu128 << 112) | (1u128 << 111));
        }
    }
}

#[test]
fn armv7_narrow_and_binary64_long_double_vectors_keep_their_lanes() {
    use toucan_semantic::FloatingFormat;
    use toucan_target::Target;

    let profile = CompilerProfile::default_for(Target::Armv7UnknownLinuxGnueabihf);
    let source = "typedef _Float16 H4 __attribute__((vector_size(8))); \
        typedef __bf16 B4 __attribute__((vector_size(8))); \
        typedef float F4 __attribute__((vector_size(16))); \
        typedef long double L __attribute__((vector_size(16))); \
        _Static_assert(sizeof(L)==16 && _Alignof(L)==8, \"ARM vector alignment\");";
    let analysis = check(source, profile).unwrap();
    let unit = analysis.unit();
    assert_eq!(
        evaluate_vector(
            unit,
            "__builtin_convertvector((F4){1.00048828125f,0x1p-24f,-0.0f,65504.0f},H4)"
        )
        .unwrap()
        .lanes()
        .iter()
        .copied()
        .map(bits)
        .collect::<Vec<_>>(),
        [0x3c00, 1, 0x8000, 0x7bff],
    );
    let long_double = evaluate_vector(unit, "(L){1.0L}+(L){0.5L}").unwrap();
    assert_eq!(long_double.lanes().len(), 2);
    let ArithmeticConstant::Floating(first) = long_double.lanes()[0] else {
        panic!("first long-double lane must be floating");
    };
    assert_eq!(
        (first.format(), first.to_bits()),
        (FloatingFormat::Binary64, 0x3ff8000000000000)
    );
    assert!(
        check(
            "typedef __int128 V __attribute__((vector_size(16)));",
            profile
        )
        .is_err()
    );
}

#[test]
fn vector_queries_bound_work_and_reject_mutated_oversized_types() {
    use toucan_semantic::TypeKind;
    let profile = CompilerProfile::ALL
        .into_iter()
        .find(|p| p.compiler() == Compiler::Clang)
        .unwrap();
    let mut unit = check(
        "typedef unsigned char B __attribute__((vector_size(16)));",
        profile,
    )
    .unwrap()
    .into_unit();
    let mut expression = String::from("(B){0}");
    for _ in 0..12 {
        expression = format!("({expression}+{expression})");
    }
    let error = evaluate_vector(&unit, &expression).unwrap_err();
    assert!(error.message.contains("65536-step limit"), "{error}");
    let TypeKind::Vector { lanes, .. } = &mut unit.typedefs.get_mut("B").unwrap().kind else {
        panic!()
    };
    *lanes = u64::MAX;
    let error = evaluate_vector(&unit, "(B){0}").unwrap_err();
    assert!(!error.message.is_empty());
}

#[test]
fn constant_shape_outlives_the_temporary_type_environment() {
    use toucan_semantic::{IntegerKind, VectorElement, VectorKind};
    for profile in CompilerProfile::ALL {
        let value = {
            let unit = check(
                "typedef int A __attribute__((vector_size(16),aligned(32)));",
                profile,
            )
            .unwrap();
            evaluate_vector(unit.unit(), "(__typeof__(A)){1,2,3,4}").unwrap()
        };
        assert_eq!(value.ty().alignment_bytes(), 32);
        assert_eq!(value.ty().size_bytes(), 16);
        assert_eq!(value.ty().lane_count(), 4);
        assert_eq!(
            value.ty().element(),
            VectorElement::Integer(IntegerKind::Int)
        );
        assert_eq!(value.ty().kind(), VectorKind::Gnu);
        assert!(!value.ty().is_atomic());
        assert_eq!(
            value.lanes().iter().copied().map(bits).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
    }
}

#[test]
fn vector_folding_does_not_change_scalar_constant_queries() {
    for profile in CompilerProfile::ALL {
        check(&format!("{TYPES}\n_Static_assert(!__builtin_constant_p((I4){{1,2,3,4}}+(I4){{5,6,7,8}}), \"conservative query\");"), profile).unwrap();
        assert!(check(&format!("{TYPES}\n_Static_assert(((I4){{1,2,3,4}}+(I4){{5,6,7,8}})[0]==6, \"not an ICE\");"), profile).is_err());
    }
}

#[test]
#[ignore = "requires native GCC/Clang; compares C storage bits at O0/O2"]
fn native_static_vector_values_match_folded_lanes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("vectors.c");
    for cc in ["gcc", "clang"] {
        if cc == "gcc" && !cfg!(target_os = "linux") {
            continue;
        }
        let mut source = format!("#include <stdio.h>\n{TYPES}\n");
        let mut prints = String::from("int main(void){\n");
        let mut expected = String::new();
        for (i, case) in CASES.iter().enumerate() {
            if !(if cc == "gcc" {
                case.gnu_static
            } else {
                case.clang_static
            }) {
                continue;
            }
            source.push_str(&format!("{} value_{i}={};\n", case.ty, case.expression));
            for (lane, bits) in case.bits.iter().enumerate() {
                prints.push_str(&format!("{{unsigned long long bits=0; __typeof__(value_{i}[{lane}]) lane=value_{i}[{lane}]; __builtin_memcpy(&bits,&lane,sizeof(lane)); printf(\"%llu\\n\",bits);}}\n"));
                expected.push_str(&format!("{bits}\n"));
            }
        }
        source.push_str(&prints);
        source.push_str("return 0;}\n");
        std::fs::write(&file, source).unwrap();
        for optimization in ["-O0", "-O2"] {
            let binary = dir.path().join("vectors");
            let output = std::process::Command::new(if cc == "gcc" {
                std::env::var("TOUCAN_GCC").unwrap_or_else(|_| cc.into())
            } else {
                cc.into()
            })
            .args([
                "-std=gnu11",
                optimization,
                "-fsanitize=undefined",
                "-fno-sanitize-recover=all",
            ])
            .arg(&file)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
            assert!(
                toucan_test_support::compiler_acceptance(&output).unwrap(),
                "{cc} {optimization}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = std::process::Command::new(binary).output().unwrap();
            assert!(
                output.status.success(),
                "{cc} {optimization}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                expected,
                "{cc} {optimization}"
            );
        }
    }
}

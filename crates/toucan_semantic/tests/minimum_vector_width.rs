use toucan_semantic::{Analysis, AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};

fn check(source: &str, profile: CompilerProfile) -> Result<Analysis, toucan_semantic::Error> {
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
        (Err(a), Err(b)) => assert_eq!((a.offset, &a.message), (b.offset, &b.message)),
        _ => panic!("{profile:?} {source}: {plain:?} {retained:?}"),
    }
    retained
}
fn width(analysis: &Analysis, name: &str) -> Option<u32> {
    let index = analysis
        .unit()
        .declarations
        .iter()
        .position(|d| d.name == name)
        .unwrap();
    analysis
        .unit()
        .function_options
        .get(&index)
        .and_then(|o| o.minimum_vector_width())
}

const VALUES: &[(&str, Option<u32>)] = &[
    ("0", Some(0)),
    ("3", Some(3)),
    ("128", Some(128)),
    ("4294967295u", Some(u32::MAX)),
    ("-1", Some(u32::MAX)),
    ("(signed char)-1", Some(255)),
    ("(short)-1", Some(65535)),
    ("(-2147483647-1)", Some(1 << 31)),
    ("(__int128)128", Some(128)),
    ("sizeof(int)*32", Some(128)),
    ("(int)128.0", Some(128)),
    ("4294967296ull", None),
    ("(__int128)-1", None),
    ("-1LL", None),
    ("128.0", None),
    ("(int)(64.0+64.0)", None),
    ("(0,128)", None),
    ("(int)-128.0", None),
];
const CASES: &[(&str, bool)] = &[
    (
        "int g(void); int n; __attribute__((min_vector_width(0&&g()))) int f(void);",
        false,
    ),
    (
        "int g(void); int n; __attribute__((min_vector_width(0&&n))) int f(void);",
        false,
    ),
    (
        "int g(void); int n; __attribute__((min_vector_width(1?0:g()))) int f(void);",
        false,
    ),
    (
        "int g(void); int n; __attribute__((min_vector_width(0?g():0))) int f(void);",
        false,
    ),
    ("__attribute__((min_vector_width())) int f(void);", false),
    ("__attribute__((min_vector_width(1,2))) int f(void);", false),
    ("__attribute__((min_vector_width(128))) int value;", false),
    (
        "__attribute__((min_vector_width(128))) typedef int F(void);",
        false,
    ),
    (
        "int (*f)(void) __attribute__((min_vector_width(128)));",
        false,
    ),
    (
        "int f(int p __attribute__((min_vector_width(128))));",
        false,
    ),
    (
        "struct S{int x __attribute__((min_vector_width(128)));};",
        false,
    ),
    (
        "const int N=128; __attribute__((min_vector_width(N))) int f(void);",
        false,
    ),
    (
        "int N; __attribute__((min_vector_width(N))) int f(void);",
        false,
    ),
    (
        "int f(void){return 0;} __attribute__((min_vector_width())) int f(void);",
        false,
    ),
    (
        "int f(void){return 0;} __attribute__((min_vector_width(128))) int f(void);",
        true,
    ),
    (
        "__attribute__((min_vector_width(1),min_vector_width(128.0))) int f(void);",
        false,
    ),
    (
        "int x=sizeof(int __attribute__((min_vector_width())));",
        true,
    ),
    (
        "int x=sizeof(int __attribute__((min_vector_width(128.0))));",
        true,
    ),
    (
        "int x=sizeof(int __attribute__((min_vector_width(oops))));",
        false,
    ),
    (
        "__attribute__((min_vector_width(sizeof(struct T{int x;})))) int f(void){return sizeof(struct T);}",
        true,
    ),
];

#[test]
fn argument_bits_and_subjects_follow_clang_without_affecting_gnu() {
    for profile in CompilerProfile::ALL {
        for &(value, expected) in VALUES {
            let source =
                format!("__attribute__((min_vector_width({value}))) int f(void){{return 0;}}");
            let a = check(&source, profile);
            assert_eq!(
                a.is_ok(),
                (profile.compiler() == Compiler::Gnu || expected.is_some())
                    && !(profile.target() == Target::I686UnknownLinuxGnu
                        && value.contains("__int128")),
                "{profile:?} {source}: {a:?}"
            );
            if let Ok(a) = a {
                assert_eq!(
                    width(&a, "f"),
                    if profile.compiler() == Compiler::Clang {
                        expected
                    } else {
                        None
                    }
                );
            }
        }
        for &(source, accepted) in CASES {
            assert_eq!(
                check(source, profile).is_ok(),
                profile.compiler() == Compiler::Gnu || accepted,
                "{profile:?} {source}"
            );
        }
        let a = check(
            "__attribute__((min_vector_width(-1L))) int f(void);",
            profile,
        );
        assert_eq!(
            a.is_ok(),
            profile.compiler() == Compiler::Gnu
                || profile.target().is_windows()
                || profile.target() == Target::I686UnknownLinuxGnu
        );
    }
}

#[test]
fn declaration_hints_preserve_source_order_and_lexical_inheritance() {
    for profile in CompilerProfile::ALL {
        let source = "__attribute__((min_vector_width(64),min_vector_width(128))) int f(void); __attribute__((min_vector_width(256))) int f(void){return 0;} __attribute__((min_vector_width(512))) int f(void);";
        let a = check(source, profile).unwrap();
        if profile.compiler() == Compiler::Gnu {
            assert!(a.unit().function_options.is_empty());
            continue;
        }
        assert_eq!(width(&a, "f"), Some(256));
        let sites = a
            .checked()
            .unwrap()
            .function_option_sites()
            .collect::<Vec<_>>();
        assert_eq!(sites.len(), 3);
        assert_eq!(
            sites
                .iter()
                .map(|s| s.effective().minimum_vector_width())
                .collect::<Vec<_>>(),
            [Some(64), Some(256), Some(256)]
        );
        assert_eq!(
            sites[0]
                .minimum_vector_width()
                .iter()
                .map(|a| a.value())
                .collect::<Vec<_>>(),
            [64, 128]
        );
        for site in &sites {
            for attr in site.minimum_vector_width() {
                assert!(source[attr.source().range()].starts_with("min_vector_width("));
            }
        }
        for (prefix, expected) in [
            (
                "void g(void){__attribute__((min_vector_width(64))) int f(void);}",
                Some(64),
            ),
            (
                "int f(void); void g(void){__attribute__((min_vector_width(64))) int f(void);}",
                None,
            ),
            (
                "void g(void){int f(void);} void h(void){__attribute__((min_vector_width(64))) int f(void);}",
                None,
            ),
            (
                "void g(void){__attribute__((min_vector_width(64))) int f(void);} void h(void){__attribute__((min_vector_width(128))) int f(void);}",
                Some(64),
            ),
        ] {
            let a = check(&format!("{prefix} int f(void){{return 0;}}"), profile).unwrap();
            assert_eq!(width(&a, "f"), expected);
        }
        assert_eq!(
            width(
                &check(
                    "__attribute__((min_vector_width(128))) int f(void); int f(void){return 0;}",
                    profile
                )
                .unwrap(),
                "f"
            ),
            Some(128)
        );
    }
}

#[test]
fn width_is_a_hint_and_caller_built_units_keep_profile_validation() {
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let a=check("__attribute__((min_vector_width(128),noinline)) int f(int x){return x;} __attribute__((min_vector_width(64))) int g(int x){return f(x);}",profile).unwrap();
        assert_eq!(width(&a, "g"), Some(64));
        let mut unit = a.into_unit();
        assert_eq!(
            toucan_semantic::evaluate_integer(&unit, "sizeof(int)")
                .unwrap()
                .value,
            4
        );
        unit.compiler = Compiler::Gnu;
        unit.target = Target::X86_64UnknownLinuxGnu;
        assert!(
            unit.validate_function_options()
                .unwrap_err()
                .message
                .contains("Clang")
        );
    }
    let clang = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    let e=check("__attribute__((target(\"no-mmx\"),min_vector_width(128))) void f(void){__builtin_ia32_emms();}",clang).unwrap_err();
    assert!(e.message.contains("MMX"), "{e}");
}

#[test]
#[ignore = "requires genuine native GNU GCC and Clang's five target backends"]
fn native_arguments_subjects_and_hint_lowering() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("width.c");
    let output = directory.path().join("width.ll");
    for profile in CompilerProfile::ALL {
        let clang = profile.compiler() == Compiler::Clang;
        let mut cc = if clang {
            let mut c = std::process::Command::new("clang");
            c.args(["-target", profile.target().triple(), "-emit-llvm"]);
            c
        } else if (cfg!(all(target_os = "linux", target_arch = "x86_64"))
            && matches!(
                profile.target(),
                Target::X86_64UnknownLinuxGnu | Target::X86_64UnknownLinuxMusl
            ))
            || (cfg!(all(target_os = "linux", target_arch = "aarch64"))
                && matches!(
                    profile.target(),
                    Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
                ))
        {
            std::process::Command::new(std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()))
        } else {
            continue;
        };
        cc.args(["-std=gnu11", "-O0", "-S"])
            .arg(&input)
            .arg("-o")
            .arg(&output);
        let extra = if profile.target().is_windows() {
            Some(u32::MAX)
        } else {
            None
        };
        for &(expression, expected) in VALUES.iter().chain(std::iter::once(&("-1L", extra))) {
            let source =
                format!("__attribute__((min_vector_width({expression}))) int f(void){{return 0;}}");
            std::fs::write(&input, &source).unwrap();
            let result = cc.output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&result).unwrap(),
                !clang || expected.is_some(),
                "{profile:?} {source}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            if clang
                && let Some(expected) = expected
                && matches!(
                    profile.target(),
                    Target::X86_64UnknownLinuxGnu
                        | Target::X86_64UnknownLinuxMusl
                        | Target::X86_64AppleDarwin
                        | Target::X86_64PcWindowsMsvc
                )
            {
                let ir = std::fs::read_to_string(&output).unwrap();
                assert!(
                    ir.contains(&format!("\"min-legal-vector-width\"=\"{expected}\"")),
                    "{ir}"
                );
            }
        }
        for &(source, expected) in CASES {
            std::fs::write(&input, source).unwrap();
            let result = cc.output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&result).unwrap(),
                !clang || expected,
                "{profile:?} {source}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

#[test]
fn ignored_hints_still_check_argument_types_without_runtime_obligations() {
    for profile in CompilerProfile::ALL {
        for argument in ["oops+1", "\"x\"+\"y\"", "sizeof(int[-1])"] {
            let source = format!("__attribute__((min_vector_width({argument}))) int f(void);");
            assert!(check(&source, profile).is_err(), "{profile:?} {source}");
        }
        let a=check("__attribute__((min_vector_width(sizeof(struct T{int x;})))) int f(void){return sizeof(struct T);}",profile).unwrap();
        assert_eq!(
            a.unit()
                .records
                .iter()
                .filter(|r| r.name.as_deref() == Some("T"))
                .count(),
            1
        );
    }
    let clang = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    check("__attribute__((target(\"no-mmx\"))) int f(void){return sizeof(int __attribute__((min_vector_width(__builtin_ia32_emms()))));}",clang).unwrap();
}

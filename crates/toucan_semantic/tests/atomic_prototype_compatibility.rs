use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile};

// Compatibility with a function type whose parameter list is unspecified.
const CASES: &[(&str, bool, bool)] = &[
    ("_Atomic(float)", false, true),
    ("_Atomic(_Bool)", false, true),
    ("_Atomic(char)", false, true),
    ("_Atomic(unsigned char)", false, true),
    ("_Atomic(short)", false, true),
    ("_Atomic(unsigned short)", false, true),
    ("const AtomicFloat", false, true),
    ("_Atomic(ScalarFloat)", false, true),
    ("_Atomic(int)", true, true),
    ("_Atomic(double)", true, true),
    ("_Atomic(float) *", true, true),
    ("const float", false, false),
    ("volatile unsigned short", false, false),
    ("const double", true, true),
];

const PREFIX: &str = "typedef float ScalarFloat; typedef _Atomic(float) AtomicFloat;\n";

fn expected(gnu: bool, clang: bool, compiler: Compiler) -> bool {
    if compiler == Compiler::Gnu {
        gnu
    } else {
        clang
    }
}

fn queries(compiler: Compiler) -> String {
    let mut source = String::from(PREFIX);
    for &(parameter, gnu, clang) in CASES {
        let value = u8::from(expected(gnu, clang, compiler));
        for (left, right) in [
            ("void()".to_owned(), format!("void({parameter})")),
            (format!("void({parameter})"), "void()".to_owned()),
            ("void(*)()".to_owned(), format!("void(*)({parameter})")),
        ] {
            source.push_str(&format!(
                "_Static_assert(__builtin_types_compatible_p({left}, {right}) == {value}, \"{parameter}\");\n"
            ));
        }
    }
    source
}

fn redeclaration(parameter: &str, prototype_first: bool) -> String {
    let prototype = format!("void f({parameter});");
    if prototype_first {
        format!("{PREFIX}{prototype} void f();")
    } else {
        format!("{PREFIX}void f(); {prototype}")
    }
}

#[test]
fn atomic_parameter_compatibility_follows_profile_default_promotions() {
    for profile in CompilerProfile::ALL {
        for retain_code in [false, true] {
            let options = AnalysisOptions {
                retain_code,
                ..Default::default()
            };
            analyze_with_profile(&queries(profile.compiler()), profile, &options)
                .unwrap_or_else(|error| panic!("{profile:?}, retained={retain_code}: {error}"));
            for &(parameter, gnu, clang) in CASES {
                for prototype_first in [false, true] {
                    let source = redeclaration(parameter, prototype_first);
                    let result = analyze_with_profile(&source, profile, &options);
                    assert_eq!(
                        result.is_ok(),
                        expected(gnu, clang, profile.compiler()),
                        "{profile:?}, retained={retain_code}: {source}: {result:?}"
                    );
                    if let Err(error) = result {
                        assert!(error.message.contains("conflicting declaration"), "{error}");
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires native GNU GCC and Clang; run with --include-ignored"]
fn atomic_prototype_queries_and_redeclarations_match_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    for (compiler, profile) in [
        (
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            Compiler::Gnu,
        ),
        ("clang".into(), Compiler::Clang),
    ] {
        let mut sources = vec![(queries(profile), true)];
        for &(parameter, gnu, clang) in CASES {
            for prototype_first in [false, true] {
                sources.push((
                    redeclaration(parameter, prototype_first),
                    expected(gnu, clang, profile),
                ));
            }
        }
        for (source, accepted) in sources {
            let mut child = Command::new(&compiler)
                .args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(source.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(accepted),
                "{compiler}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if !accepted {
                assert!(String::from_utf8_lossy(&output.stderr).contains("conflicting types"));
            }
        }
    }
}

use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile};

// Each declarator defines F; the last columns give GNU and Clang acceptance.
const CASES: &[(&str, &str, bool, bool)] = &[
    ("int F(void)", "const int F(void)", true, false),
    ("int F(void)", "volatile int F(void)", true, false),
    ("void F(void)", "const void F(void)", true, false),
    ("int F(void)", "Result F(void)", true, false),
    (
        "int *F(void)",
        "int *const volatile restrict F(void)",
        true,
        false,
    ),
    ("int (*F)(int)", "const int (*F)(int)", true, false),
    (
        "int F(int (*)(void))",
        "int F(const int (*)(void))",
        true,
        false,
    ),
    (
        "_Atomic(int) F(void)",
        "const _Atomic(int) F(void)",
        true,
        false,
    ),
    ("int F(void)", "int F(void)", true, true),
    ("int *F(void)", "const int *F(void)", false, false),
    ("int F(void)", "_Atomic(int) F(void)", false, false),
    ("int F(void)", "long F(void)", false, false),
    ("int F(int)", "int F(float)", false, false),
    ("int F(void)", "int F()", false, false),
    ("int (*F(void))[4]", "int (*F(void))[]", false, false),
    ("int (*F)(void)", "int (*const F)(void)", false, false),
];

fn source(first: &str, second: &str, block: bool) -> String {
    let declarations = format!("typedef {first}; typedef {second};");
    let scope = if block {
        format!("void check(void) {{ {declarations} }}")
    } else {
        declarations
    };
    format!("typedef const int Result; {scope}\n")
}

#[test]
fn function_typedef_redeclarations_follow_profile_return_qualifiers() {
    for profile in CompilerProfile::ALL {
        for &(first, second, gnu, clang) in CASES {
            let accepted = if profile.compiler() == Compiler::Gnu {
                gnu
            } else {
                clang
            };
            for (first, second) in [(first, second), (second, first)] {
                for block in [false, true] {
                    let source = source(first, second, block);
                    for retain_code in [false, true] {
                        let result = analyze_with_profile(
                            &source,
                            profile,
                            &AnalysisOptions {
                                retain_code,
                                ..Default::default()
                            },
                        );
                        assert_eq!(
                            result.is_ok(),
                            accepted,
                            "{profile:?}, retained={retain_code}: {source}: {result:?}"
                        );
                        if let Err(error) = result {
                            assert!(
                                error.message.contains("conflicting")
                                    && error.message.contains("typedef"),
                                "{error}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires native GNU GCC and Clang; run with --include-ignored"]
fn function_typedef_qualifiers_match_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    for (compiler, gnu) in [
        (
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            true,
        ),
        ("clang".into(), false),
    ] {
        for &(first, second, gcc_accepts, clang_accepts) in CASES {
            let accepted = if gnu { gcc_accepts } else { clang_accepts };
            for (first, second) in [(first, second), (second, first)] {
                for block in [false, true] {
                    let source = source(first, second, block);
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
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    assert_eq!(
                        toucan_test_support::compiler_acceptance(&output),
                        Ok(accepted),
                        "{compiler}: {source}: {stderr}"
                    );
                    if !accepted {
                        assert!(
                            stderr.contains("conflicting") || stderr.contains("redefinition"),
                            "{stderr}"
                        );
                    }
                }
            }
        }
    }
}

use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{CompilerProfile, LanguageMode};

const STATEMENTS: &[(&str, bool)] = &[
    ("if (sizeof(enum { T })) (void)0;", true),
    ("switch (sizeof(enum { T })) { default: break; }", true),
    ("while (sizeof(enum { T })) (void)0;", true),
    ("do (void)sizeof(enum { T }); while (0);", true),
    ("for ((void)sizeof(enum { T });0;) (void)0;", true),
    ("if (1) { (void)sizeof(enum { T }); }", false),
    ("do { (void)sizeof(enum { T }); } while (0);", false),
    ("{ (void)sizeof(enum { T }); }", false),
];

fn probe(statement: &str, has_enumerator: bool, mode: LanguageMode) -> String {
    let expected = if mode.is_c90() && has_enumerator {
        "sizeof(int)"
    } else {
        "1"
    };
    format!(
        "typedef char T; void f(void) {{ {statement} (void)sizeof(char[sizeof(T) == {expected} ? 1 : -1]); }}\n"
    )
}

#[test]
fn c90_control_statements_keep_enumerators_in_the_enclosing_block() {
    for &(statement, has_enumerator) in STATEMENTS {
        for profile in CompilerProfile::ALL {
            for mode in [LanguageMode::C90, LanguageMode::Gnu90, LanguageMode::C11] {
                let source = probe(statement, has_enumerator, mode);
                for retain_code in [false, true] {
                    analyze_with_profile(
                        &source,
                        profile.with_language_mode(mode),
                        &AnalysisOptions {
                            retain_code,
                            ..Default::default()
                        },
                    )
                    .unwrap_or_else(|error| {
                        panic!("{profile:?}/{mode:?}/{retain_code}: {source}: {error}")
                    });
                }
            }
        }
    }
}

#[test]
fn unbraced_bodies_share_the_control_expression_scope_only_in_c90() {
    let source = "void f(void) { if (sizeof(enum { T })) (void)sizeof(enum { T }); }";
    for profile in CompilerProfile::ALL {
        for mode in [LanguageMode::C90, LanguageMode::Gnu90, LanguageMode::C11] {
            for retain_code in [false, true] {
                let result = analyze_with_profile(
                    source,
                    profile.with_language_mode(mode),
                    &AnalysisOptions {
                        retain_code,
                        ..Default::default()
                    },
                );
                assert_eq!(
                    result.is_ok(),
                    !mode.is_c90(),
                    "{profile:?}/{mode:?}: {result:?}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang"]
fn control_statement_scopes_match_native_compilers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scope.c");
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        std::env::var("TOUCAN_CLANG").unwrap_or_else(|_| "clang".into()),
    ] {
        for &(statement, has_enumerator) in STATEMENTS {
            for mode in [LanguageMode::C90, LanguageMode::Gnu90, LanguageMode::C11] {
                let source = probe(statement, has_enumerator, mode);
                std::fs::write(&path, &source).unwrap();
                let output = std::process::Command::new(&compiler)
                    .arg(format!("-std={mode}"))
                    .args(["-pedantic-errors", "-fsyntax-only"])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{compiler}/{mode}: {source}\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

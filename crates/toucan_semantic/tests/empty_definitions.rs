use std::process::Command;
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

// Clang warns, rather than rejects, when the prototype follows an empty definition.
const CASES: &[(&str, bool, bool)] = &[
    (
        "int (*f(int named))(int nested); int (*f())(int nested) {return 0;}",
        false,
        false,
    ),
    (
        "int (*f(void))(int nested); int (*f())(int nested) {return 0;}",
        true,
        false,
    ),
    ("int f(int named); int f() {return 0;}", false, false),
    ("int f(int); int f() {return 0;}", false, false),
    ("int f(int named, ...); int f() {return 0;}", false, false),
    ("int f() {return 0;} int f(int named);", false, true),
    ("int f() {return 0;} int f(int);", false, true),
    (
        "int f(int old); int f(int actual); int f() {return 0;}",
        false,
        false,
    ),
    ("int f(void); int f() {return 0;}", true, false),
    ("int f() {return 0;} int f(void);", true, false),
    ("int f(); int f() {return 0;}", true, false),
    (
        "int f(int old); int f(int actual) {return actual;}",
        true,
        false,
    ),
    ("int f(); int f(int actual) {return actual;}", true, false),
];

#[test]
fn empty_definitions_have_zero_parameters_even_with_an_earlier_prototype() {
    for target in Target::ALL {
        for &(source, accepted, _) in CASES {
            let ordinary = analyze(source, target);
            let retained = analyze_with_options(
                source,
                target,
                &AnalysisOptions {
                    retain_code: true,
                    ..Default::default()
                },
            );
            assert_eq!(
                ordinary.is_ok(),
                accepted,
                "{target}: {source}: {ordinary:?}"
            );
            match (ordinary, retained) {
                (Ok(ordinary), Ok(retained)) => {
                    assert_eq!(format!("{ordinary:?}"), format!("{:?}", retained.unit()))
                }
                (Err(ordinary), Err(retained)) => assert_eq!(
                    (ordinary.offset, ordinary.message),
                    (retained.offset, retained.message)
                ),
                other => panic!("retention changed acceptance: {other:?}"),
            }
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang; run with --include-ignored"]
fn definition_parameter_counts_match_compiler_constraints() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("definition.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc.as_str(), "clang"] {
        for &(source, accepted, clang_warning) in CASES {
            std::fs::write(&path, source).unwrap();
            let mut command = Command::new(compiler);
            command.args(["-std=c11", "-fsyntax-only"]);
            let result = command.arg(&path).output().unwrap();
            let warning = compiler == "clang" && clang_warning;
            let diagnostic = String::from_utf8_lossy(&result.stderr);
            assert_eq!(
                result.status.success(),
                accepted || warning,
                "{compiler}: {source}: {diagnostic}"
            );
            if warning {
                assert!(
                    diagnostic.contains("conflicting with a subsequent declaration"),
                    "{diagnostic}"
                );
            }
        }
    }
}

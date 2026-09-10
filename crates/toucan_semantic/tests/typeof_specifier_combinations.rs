use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::CompilerProfile;

const CASES: &[(&str, bool)] = &[
    ("int __typeof__(int) value;", false),
    ("__typeof__(int) unsigned value;", false),
    ("__typeof__(int) __typeof__(int) value;", false),
    ("_Complex __typeof__(double) value;", false),
    ("_Atomic(int) __typeof__(int) value;", false),
    ("int source; unsigned __typeof__(source) value;", false),
    (
        "struct Holder { int __typeof__(void(int)) *callback; };",
        false,
    ),
    ("const __typeof__(int) value;", true),
    ("_Atomic __typeof__(int) value;", true),
    ("int source; volatile __typeof__(source) value;", true),
    ("typedef const int CI; __typeof__(CI) value;", true),
    ("struct Holder { __typeof__(void(int)) *callback; };", true),
];

#[test]
fn typeof_accepts_qualifiers_but_rejects_other_type_specifiers() {
    for profile in CompilerProfile::ALL {
        for &(source, accepted) in CASES {
            for retain_code in [false, true] {
                let result = analyze_with_profile(
                    source,
                    profile,
                    &AnalysisOptions {
                        retain_code,
                        ..Default::default()
                    },
                );
                assert_eq!(
                    result.is_ok(),
                    accepted,
                    "{profile:?}, retain={retain_code}: {source}: {result:?}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang"]
fn typeof_specifier_combinations_match_native_compilers() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("typeof.c");
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        "clang".into(),
    ] {
        for &(source, accepted) in CASES {
            std::fs::write(&source_path, source).unwrap();
            let output = std::process::Command::new(&compiler)
                .args(["-std=gnu11", "-fsyntax-only"])
                .arg(&source_path)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(accepted),
                "{compiler}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

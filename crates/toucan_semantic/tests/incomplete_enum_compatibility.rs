use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{CompilerProfile, Target};

fn source(microsoft: bool) -> String {
    let mut source =
        String::from("enum E; enum F; typedef enum E Alias; typedef const enum E ConstAlias;\n");
    for (left, right, expected) in [
        ("enum E", "int", microsoft),
        ("enum E", "unsigned int", false),
        ("enum E", "long", false),
        ("enum E", "unsigned char", false),
        ("Alias", "const int", microsoft),
        ("enum E *", "int *", microsoft),
        ("enum E **", "int **", microsoft),
        ("const enum E **", "const int **", false),
        ("ConstAlias *", "const int *", false),
        ("volatile enum E *", "volatile int *", false),
        ("enum E (*)(void)", "int (*)(void)", microsoft),
        ("void(enum E)", "void(int)", microsoft),
        ("enum E", "enum E", true),
        ("enum E", "enum F", false),
        ("Alias *", "enum E *", true),
    ] {
        for (left, right) in [(left, right), (right, left)] {
            source.push_str(&format!(
                "_Static_assert(__builtin_types_compatible_p({left}, {right}) == {}, \"forward enum\");\n",
                u8::from(expected)
            ));
        }
    }
    source.push_str(&format!(
        "enum {{ BEFORE = __builtin_types_compatible_p(enum E, unsigned int) }};
         enum E {{ E_ZERO }};
         _Static_assert(BEFORE == 0, \"earlier query keeps its result\");
         _Static_assert(__builtin_types_compatible_p(Alias, {}) == 1, \"completed enum\");
         enum F {{ F_NEGATIVE = -1 }};
         _Static_assert(__builtin_types_compatible_p(enum F, int), \"signed enum\");
         _Static_assert(!__builtin_types_compatible_p(enum E, enum F), \"distinct tags\");\n",
        if microsoft { "int" } else { "unsigned int" }
    ));
    source
}

#[test]
fn incomplete_enums_compare_without_requesting_object_layout() {
    for profile in CompilerProfile::ALL {
        let source = source(profile.target().is_windows());
        for retain_code in [false, true] {
            analyze_with_profile(
                &source,
                profile,
                &AnalysisOptions {
                    retain_code,
                    ..Default::default()
                },
            )
            .unwrap_or_else(|error| panic!("{profile:?}, retained={retain_code}: {error}"));
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang with all target backends"]
fn incomplete_enum_queries_match_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let commands = std::iter::once((gcc.as_str(), None)).chain(
        Target::ALL
            .into_iter()
            .map(|target| ("clang", Some(target))),
    );
    for (compiler, target) in commands {
        let mut command = Command::new(compiler);
        if let Some(target) = target {
            command.args(["-target", target.triple()]);
        }
        let mut child = command
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
            .write_all(source(target.is_some_and(Target::is_windows)).as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(true),
            "{compiler} {target:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

use std::path::Path;

use toucan_preprocessor::{
    CommandLineMacroNormalizer, Config, LineComments, PredefinedMacroMode, Preprocessor,
};

fn config(line_comments: LineComments) -> Config {
    Config {
        line_comments,
        allow_filesystem: false,
        ..Config::default()
    }
}

fn expand(mode: LineComments, source: &str) -> Result<String, String> {
    Preprocessor::new(config(mode))
        .preprocess_str(Path::new("comments.c"), source)
        .map(|output| output.source)
        .map_err(|error| error.to_string())
}

fn compact(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn c90_slashes_preserve_division_and_macro_tokens() {
    for mode in [
        LineComments::GnuC90,
        LineComments::ClangC90,
        LineComments::ClangC90Preprocessing,
    ] {
        assert_eq!(
            compact(&expand(mode, "int x=6 //**/ 2;\n").unwrap()),
            "intx=6/2;"
        );
        assert_eq!(
            compact(&expand(mode, "const char *s=\"//literal\";\n").unwrap()),
            "constchar*s=\"//literal\";"
        );
    }
    for mode in [LineComments::GnuC90, LineComments::ClangC90Preprocessing] {
        let source = "#define UNUSED 1 // retained\n#define X 6 //**/ 2\nint x=X;\n";
        assert_eq!(compact(&expand(mode, source).unwrap()), "intx=6/2;");
        assert_eq!(
            compact(&expand(mode, "#define X 1 // retained\nX\n").unwrap()),
            "1//retained"
        );
    }
    for mode in [
        LineComments::Enabled,
        LineComments::GnuC90,
        LineComments::ClangC90,
        LineComments::ClangC90Preprocessing,
    ] {
        // Replacement tokens cannot become a comment during rescanning.
        assert_eq!(
            compact(&expand(mode, "#define S /\nS/kept\n").unwrap()),
            "//kept"
        );
    }
}

#[test]
fn compiler_and_preprocessing_comment_policies_are_distinct() {
    let source = "int x; // discarded\nint y;\n";
    assert!(
        expand(LineComments::GnuC90, source)
            .unwrap_err()
            .contains("ISO C90")
    );
    assert_eq!(
        compact(&expand(LineComments::ClangC90, source).unwrap()),
        "intx;inty;"
    );
    assert_eq!(
        compact(&expand(LineComments::ClangC90Preprocessing, source).unwrap()),
        "intx;//discardedinty;"
    );
    assert!(expand(LineComments::GnuC90, "#if 0\n// inactive\n#endif\nint x;\n").is_ok());
    for source in [
        "#pragma pack(push) // discarded\n",
        "_Pragma(\"pack(push) // discarded\")\n",
    ] {
        assert!(
            expand(LineComments::GnuC90, source)
                .unwrap_err()
                .contains("ISO C90")
        );
        assert!(expand(LineComments::ClangC90, source).is_ok());
    }
}

#[test]
fn clang_extension_state_is_per_file_including_skipped_groups() {
    for prefix in [
        "// enabled\n",
        "#if 0\n// enabled\n#endif\n",
        "#define UNUSED 1 // enabled\n",
    ] {
        let source = format!("{prefix}int x=6 //**/ 2;\n");
        assert_eq!(
            compact(&expand(LineComments::ClangC90, &source).unwrap()),
            "intx=6"
        );
    }
    let mut config = config(LineComments::ClangC90);
    config
        .virtual_headers
        .insert("extension.h".into(), "// enabled\n".into());
    config
        .virtual_headers
        .insert("division.h".into(), "int header=6 //**/ 2;\n".into());
    let mut pp = Preprocessor::new(config);
    let output = pp
        .preprocess_str(
            Path::new("outer.c"),
            "#include <extension.h>\nint x=6 //**/ 2;\n",
        )
        .unwrap();
    assert_eq!(compact(&output.source), "intx=6/2;");
    let output = pp
        .preprocess_str(
            Path::new("outer.c"),
            "// enabled\n#include <division.h>\nint x=6 //**/ 2;\n",
        )
        .unwrap();
    assert_eq!(compact(&output.source), "intheader=6/2;intx=6");
    // Reusing the preprocessor starts a fresh file lexer.
    let output = pp
        .preprocess_str(Path::new("outer.c"), "int x=6 //**/ 2;\n")
        .unwrap();
    assert_eq!(compact(&output.source), "intx=6/2;");
}

#[test]
fn ordered_definitions_preserve_comment_state_after_undefinition() {
    for enable_first in [false, true] {
        let mut config = config(LineComments::ClangC90);
        config.predefined_macro_mode = PredefinedMacroMode::ClangCommandLine;
        let mut normalizer = CommandLineMacroNormalizer::new(&config);
        for (name, value) in if enable_first {
            [("A", "1//first"), ("B", "6//**/2")]
        } else {
            [("B", "6//**/2"), ("A", "1//first")]
        } {
            let (name, value) = normalizer.prepare(name, value).unwrap();
            config.defines.insert(name, value);
            config.undefine("A");
        }
        config.predefined_macro_mode = PredefinedMacroMode::Tokens;
        let output = Preprocessor::new(config)
            .preprocess_str(Path::new("definitions.c"), "B\nint x=6 //**/ 2;\n")
            .unwrap();
        assert_eq!(
            compact(&output.source),
            if enable_first {
                "6intx=6/2;"
            } else {
                "6/2intx=6/2;"
            }
        );
    }
}

#[test]
fn ordered_definition_budget_counts_removed_comments() {
    let mut config = config(LineComments::ClangC90);
    config.predefined_macro_mode = PredefinedMacroMode::ClangCommandLine;
    config.max_source_bytes = 16;
    let mut normalizer = CommandLineMacroNormalizer::new(&config);
    let (name, value) = normalizer.prepare("A", "1//first").unwrap();
    config.defines.insert(name, value);
    config.undefine("A");
    assert!(
        normalizer
            .prepare("B", "2//next")
            .unwrap_err()
            .contains("byte limit")
    );
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn comment_policies_match_native_compilation_and_preprocessing() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let clang = std::env::var("TOUCAN_CLANG").unwrap_or_else(|_| "clang".into());
    let run = |compiler: &str, preprocessing: bool, source: &str| {
        let mut child = Command::new(compiler)
            .args([
                "-std=c90",
                if preprocessing { "-E" } else { "-fsyntax-only" },
                "-P",
                "-x",
                "c",
                "-",
            ])
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
        child.wait_with_output().unwrap()
    };
    let sources = [
        "_Static_assert(6 //**/ 2 == 3, \"division\");\n",
        "// enabled\n_Static_assert(6 //**/ 2\n == 6, \"comment\");\n",
        "#if 0\n// enabled\n#endif\n_Static_assert(6 //**/ 2\n == 6, \"comment\");\n",
        "#define UNUSED 1 // enabled\n_Static_assert(6 //**/ 2\n == 6, \"comment\");\n",
        "#define X 6 //**/ 2\n_Static_assert(X == 3, \"macro\");\n",
        "#if 0\n// ignored\n#endif\nint x;\n",
        "#if 1 // extension\nint x;\n#endif\n",
        "const char *s=\"//literal\";\n",
        "_Pragma(\"pack(push) // tail\")\nint x;\n",
    ];
    for (compiler, mode, preprocessing) in [
        (&gcc, LineComments::GnuC90, false),
        (&gcc, LineComments::GnuC90Preprocessing, true),
        (&clang, LineComments::ClangC90, false),
        (&clang, LineComments::ClangC90Preprocessing, true),
    ] {
        for source in sources {
            let native = run(compiler, preprocessing, source);
            match expand(mode, source) {
                Ok(expanded) if preprocessing => {
                    assert!(
                        native.status.success(),
                        "{compiler} {mode:?}: {source}\n{}",
                        String::from_utf8_lossy(&native.stderr)
                    );
                    assert_eq!(
                        compact(&expanded),
                        compact(&String::from_utf8(native.stdout).unwrap()),
                        "{compiler}: {source}"
                    );
                }
                Ok(expanded) => {
                    let result = run(compiler, false, &expanded);
                    assert_eq!(
                        native.status.success(),
                        result.status.success(),
                        "{compiler} {mode:?}: {source}\nexpanded: {expanded}\n{}",
                        String::from_utf8_lossy(&native.stderr)
                    );
                }
                Err(error) => assert!(
                    !native.status.success(),
                    "{compiler} {mode:?}: {source}\nToucan: {error}"
                ),
            }
        }
    }
}

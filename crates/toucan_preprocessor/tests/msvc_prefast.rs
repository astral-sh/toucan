use std::path::Path;

use toucan_preprocessor::{Config, Preprocessor};

const SOURCE: &str = concat!(
    "#define RULES 6001 28113\n",
    "#pragma prefast(push)\n",
    "#pragma prefast(disable: RULES, \"side effect is intentional\")\n",
    "#pragma prefast(suppress: 28113, \"reviewed expression\")\n",
    "#pragma prefast(suppress: __WARNING_BUFFER_UNDERFLOW, \"reviewed access\")\n",
    "_Pragma(\"prefast(pop)\")\n",
    "typedef int SDK_INT;\n",
);

#[test]
fn prefast_warning_state_does_not_change_sdk_declarations() {
    let result = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("sdk.h"), SOURCE)
        .unwrap();
    assert_eq!(result.source, "typedef int SDK_INT ;\n");
    // Warning-state stacks do not affect preprocessing or bindings, so a pop
    // is accepted even when a separate source file supplied the push.
    let result = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("sdk.h"), "#pragma prefast(pop)\nint value;\n")
        .unwrap();
    assert_eq!(result.source, "int value ;\n");
}

#[test]
fn malformed_prefast_pragmas_report_their_source_location() {
    for pragma in [
        "prefast",
        "prefast()",
        "prefast(push, 3)",
        "prefast(pop) unexpected",
        "prefast(disable:)",
        "prefast(disable: unknown_rule)",
        "prefast(disable: 6001, 28113)",
        "prefast(disable: 6001, reason)",
        "prefast(suppress: 28113, \"reason\", 6001)",
        "prefast(custom: 6001)",
    ] {
        let source = format!("int before;\n#pragma {pragma}\n");
        let error = Preprocessor::new(Config::default())
            .preprocess_str(Path::new("sdk.h"), &source)
            .unwrap_err();
        assert!(
            error.message.contains("unsupported pragma"),
            "{pragma}: {error}"
        );
        assert_eq!((error.path.as_path(), error.line), (Path::new("sdk.h"), 2));
    }
}

#[test]
#[ignore = "requires Clang for a Windows ARM64 PREfast pragma oracle"]
fn prefast_pragma_macro_expansion_matches_clang() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let actual = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("sdk.h"), SOURCE)
        .unwrap();
    let mut child = Command::new("clang")
        .args([
            "--target=aarch64-pc-windows-msvc",
            "-fms-extensions",
            "-E",
            "-P",
            "-std=c11",
            "-x",
            "c",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Clang is required for this differential test");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(SOURCE.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Clang: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expanded = String::from_utf8(output.stdout).unwrap();
    assert!(expanded.contains("#pragma prefast(push)\n"));
    assert!(
        expanded.contains("#pragma prefast(disable: 6001 28113, \"side effect is intentional\")\n")
    );
    assert!(expanded.contains("#pragma prefast(suppress: 28113, \"reviewed expression\")\n"));
    assert!(
        expanded.contains(
            "#pragma prefast(suppress: __WARNING_BUFFER_UNDERFLOW, \"reviewed access\")\n"
        )
    );
    assert!(expanded.contains("#pragma prefast(pop)\n"));
    let declarations = expanded
        .lines()
        .filter(|line| !line.starts_with("#pragma "))
        .collect::<String>();
    assert_eq!(
        declarations.split_whitespace().collect::<String>(),
        actual.source.split_whitespace().collect::<String>()
    );
}

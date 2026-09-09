use std::path::Path;

use toucan_preprocessor::{Config, Preprocessor};

const SOURCE: &str = concat!(
    "#define OBSOLETE UpdateICMRegKeyW\n",
    "#pragma deprecated(OBSOLETE, \"OBSOLETE\")\n",
    "_Pragma(\"deprecated(UpdateICMRegKeyW)\")\n",
    "int UpdateICMRegKeyW(void);\n",
    "__declspec(deprecated) int marked(void);\n",
);

#[test]
fn name_based_deprecation_does_not_change_declarations() {
    let result = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("wingdi.h"), SOURCE)
        .unwrap();
    assert!(result.source.contains("int UpdateICMRegKeyW ( void ) ;"));
    assert!(
        result
            .source
            .contains("__declspec ( deprecated ) int marked ( void ) ;")
    );
    assert!(!result.source.contains("#pragma deprecated"));
}

#[test]
fn malformed_deprecated_pragmas_report_their_source_location() {
    for pragma in [
        "deprecated",
        "deprecated()",
        "deprecated(123)",
        "deprecated(\"not an identifier\")",
        "deprecated(\"one,two\")",
        "deprecated(UpdateICMRegKeyW,)",
        "deprecated(,UpdateICMRegKeyW)",
        "deprecated(old newer)",
        "deprecated(old) unexpected",
    ] {
        let source = format!("int before;\n#pragma {pragma}\n");
        let error = Preprocessor::new(Config::default())
            .preprocess_str(Path::new("wingdi.h"), &source)
            .unwrap_err();
        assert!(
            error.message.contains("unsupported pragma"),
            "{pragma}: {error}"
        );
        assert_eq!(
            (error.path.as_path(), error.line),
            (Path::new("wingdi.h"), 2)
        );
    }
}

#[test]
#[ignore = "requires Clang for a Windows ARM64 deprecation oracle"]
fn quoted_and_macro_expanded_deprecated_names_match_clang() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let actual = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("wingdi.h"), SOURCE)
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
    assert!(expanded.contains("#pragma deprecated(UpdateICMRegKeyW, \"OBSOLETE\")\n"));
    assert!(expanded.contains("#pragma deprecated(UpdateICMRegKeyW)\n"));
    let declarations = expanded
        .lines()
        .filter(|line| !line.starts_with("#pragma "))
        .collect::<String>();
    assert_eq!(
        declarations.split_whitespace().collect::<String>(),
        actual.source.split_whitespace().collect::<String>()
    );
}

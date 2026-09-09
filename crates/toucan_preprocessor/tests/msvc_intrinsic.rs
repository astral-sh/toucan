use std::path::Path;

use toucan_preprocessor::{Config, Preprocessor};

#[test]
fn intrinsic_and_function_pragmas_accept_sdk_function_lists() {
    let source = concat!(
        "#define ROTATE _rotl\n",
        "#define ROTATES ROTATE, _rotr\n",
        "#pragma intrinsic(ROTATES)\n",
        "_Pragma(\"function(ROTATE)\")\n",
        "int rotate(void);\n",
    );
    let result = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("sdk.h"), source)
        .unwrap();
    assert_eq!(result.source, "int rotate ( void ) ;\n");
}

#[test]
fn malformed_intrinsic_and_function_pragmas_report_their_source_location() {
    for pragma in [
        "intrinsic",
        "intrinsic()",
        "intrinsic(12)",
        "intrinsic(\"_rotl\")",
        "intrinsic(_rotl,)",
        "intrinsic(,_rotl)",
        "intrinsic(_rotl _rotr)",
        "intrinsic(_rotl) unexpected",
        "function(_rotl, 12)",
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
#[ignore = "requires Clang for a Windows ARM64 pragma oracle"]
fn macro_expanded_intrinsic_and_function_pragmas_match_clang() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let source = concat!(
        "#define ROTATE _rotl\n",
        "#define ROTATES ROTATE, _rotr\n",
        "#pragma intrinsic(ROTATES)\n",
        "_Pragma(\"function(ROTATE)\")\n",
        "int rotate(void);\n",
    );
    let actual = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("sdk.h"), source)
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
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Clang: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expanded = String::from_utf8(output.stdout).unwrap();
    assert!(expanded.contains("#pragma intrinsic(_rotl, _rotr)\n"));
    assert!(expanded.contains("#pragma function(_rotl)\n"));
    let declarations = expanded
        .lines()
        .filter(|line| !line.starts_with("#pragma "))
        .collect::<String>();
    assert_eq!(
        declarations.split_whitespace().collect::<String>(),
        actual.source.split_whitespace().collect::<String>()
    );
}

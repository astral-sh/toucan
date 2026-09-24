use std::path::Path;

use toucan_preprocessor::{Config, Preprocessor};

const SOURCE: &str = concat!(
    "#if 1 ? 1 : (1, 2)\nint first;\n#endif\n",
    "#if 0 ? 1, 2 : 1\nint second;\n#endif\n",
    "#if 1 || (1, 2)\nint third;\n#endif\n",
    "#if 0 && (1, 2)\n#error selected short-circuited branch\n#endif\n",
    "#if (1 ? -1 : (1U, 0)) < 0\nint signed_value;\n#endif\n",
    "#if (1 ? -1 : (1, 0U)) > 0\nint unsigned_value;\n#endif\n",
    "#if 0 ? 1, 2, (3, 1 / 0) : 1\nint nested;\n#endif\n",
);

#[test]
fn commas_in_unevaluated_if_subexpressions_are_valid() {
    let output = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("conditional.h"), SOURCE)
        .unwrap();
    assert_eq!(
        output.source,
        concat!(
            "int first ;\n",
            "int second ;\n",
            "int third ;\n",
            "int signed_value ;\n",
            "int unsigned_value ;\n",
            "int nested ;\n",
        )
    );
}

#[test]
fn evaluated_commas_and_incomplete_expressions_still_fail() {
    for expression in [
        "(1, 2)",
        "1 ? 1, 2 : 0",
        "0 ? 0 : (1, 2)",
        "0 || (1, 2)",
        "1 && (1, 2)",
        "0 && 1, 2",
        "1 ? 1 : (1,)",
        "0 ? 1, : 1",
    ] {
        let source = format!("#if {expression}\n#endif\n");
        assert!(
            Preprocessor::new(Config::default())
                .preprocess_str(Path::new("conditional.h"), &source)
                .is_err(),
            "{expression}"
        );
    }
}

#[test]
#[ignore = "requires a native C compiler (set CC)"]
fn unevaluated_commas_match_a_pedantic_native_preprocessor() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .args(["-std=c11", "-pedantic-errors", "-E", "-P", "-x", "c", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(SOURCE.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("conditional.h"), SOURCE)
        .unwrap();
    let compact = |source: &str| {
        source
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>()
    };
    assert_eq!(
        compact(&actual.source),
        compact(&String::from_utf8(output.stdout).unwrap())
    );
}

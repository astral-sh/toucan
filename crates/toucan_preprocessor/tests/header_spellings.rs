use std::path::Path;

use toucan_preprocessor::{Config, Preprocessor};

const NAMES: &[(&str, &str)] = &[
    ("a<:b.h", "a[b.h"),
    ("a<%b.h", "a{b.h"),
    ("a%:b.h", "a#b.h"),
    ("a%:%:b.h", "a##b.h"),
];

fn case(written: &str, rewritten: &str) -> (Config, String, String) {
    let query = format!("only-{written}");
    let config = Config {
        allow_filesystem: false,
        virtual_headers: [
            (written.into(), "int written_header;\n".into()),
            (rewritten.into(), "int rewritten_header;\n".into()),
            (query.clone(), String::new()),
            ("ab.h".into(), "int pasted_header;\n".into()),
        ]
        .into(),
        ..Config::default()
    };
    let source = format!(
        "#if !__has_include(<{query}>) || !__has_include_next(<{query}>)\n\
         #error lost literal header spelling\n#endif\n\
         #include <{written}>\n#define HEADER <{written}>\n\
         #if !__has_include(HEADER)\n#error lost expanded header spelling\n#endif\n\
         #include HEADER\n"
    );
    // In a replacement list, %:%: still performs ordinary token pasting.
    let second = if written.contains("%:%:") {
        "pasted_header"
    } else {
        "written_header"
    };
    let expected = format!("int written_header ;\nint {second} ;\n");
    (config, source, expected)
}

#[test]
fn angle_header_names_preserve_written_digraphs() {
    for &(written, rewritten) in NAMES {
        let (config, source, expected) = case(written, rewritten);
        let result = Preprocessor::new(config)
            .preprocess_str(Path::new("input.h"), &source)
            .unwrap();
        assert_eq!(result.source, expected, "{written}");
    }
}

#[test]
#[cfg(unix)]
#[ignore = "requires a native C compiler (set CC) and POSIX header filenames"]
fn native_header_digraph_spellings_match() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let directory =
        std::env::temp_dir().join(format!("toucan-header-digraphs-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for &(written, rewritten) in NAMES {
        let (config, source, expected) = case(written, rewritten);
        for (name, contents) in &config.virtual_headers {
            std::fs::write(directory.join(name), contents).unwrap();
        }
        let actual = Preprocessor::new(Config {
            include_dirs: vec![directory.clone()],
            ..Config::default()
        })
        .preprocess_str(Path::new("input.h"), &source)
        .unwrap();
        assert_eq!(actual.source, expected, "{written}");
        let mut child = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
            .args(["-E", "-P", "-std=c11", "-I"])
            .arg(&directory)
            .args(["-x", "c", "-"])
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
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(true),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let compact = |text: &str| {
            text.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        };
        assert_eq!(
            compact(&String::from_utf8(output.stdout).unwrap()),
            compact(&expected),
            "{written}"
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

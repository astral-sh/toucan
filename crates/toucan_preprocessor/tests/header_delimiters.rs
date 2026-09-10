use std::path::Path;

use toucan_preprocessor::{Config, Preprocessor};

const NAMES: &[&str] = &[
    "a:", "a%", ":name.h", "%name.h", "=name.h", "<name.h", "a:>b.h", "a%>b.h",
];

fn config() -> Config {
    Config {
        allow_filesystem: false,
        virtual_headers: NAMES
            .iter()
            .map(|name| ((*name).into(), "int included;\n".into()))
            .collect(),
        ..Config::default()
    }
}

fn cases() -> Vec<(String, bool)> {
    let mut cases = Vec::new();
    for name in &NAMES[..6] {
        cases.push((
            format!(
                "#define QUERY __has_include\n\
                 #if !__has_include(<{name}>) || !__has_include_next(<{name}>) || !QUERY(<{name}>)\n\
                 #error missing literal header\n#endif\n#include <{name}>\n"
            ),
            true,
        ));
    }
    for suffix in [":", "%"] {
        for source in [
            format!("#define HEADER <a{suffix}>\n#include HEADER\n"),
            format!("#define OPEN <\n#include OPEN a{suffix}>\n"),
            format!("#define QUERY(x) __has_include(x)\n#if QUERY(<a{suffix}>)\n#endif\n"),
            format!("#define QUERY __has_include(<a{suffix}>)\n#if QUERY\n#endif\n"),
            format!("#if __has_include(<a{suffix}>b.h>)\n#endif\n"),
        ] {
            cases.push((source, false));
        }
        cases.push((
            format!(
                "#define HEADER <a{suffix}>b.h>\n\
                 #if !__has_include(HEADER)\n#error missing expanded header\n#endif\n\
                 #include HEADER\n"
            ),
            true,
        ));
    }
    cases
}

#[test]
fn direct_angle_header_delimiters_are_characters() {
    for (source, accepted) in cases() {
        let result = Preprocessor::new(config()).preprocess_str(Path::new("input.h"), &source);
        assert_eq!(result.is_ok(), accepted, "{source}\n{result:?}");
        if let Ok(result) = result {
            assert_eq!(result.source, "int included ;\n", "{source}");
        }
    }
    // Extra tokens are diagnosed instead of selecting the longer, wrong filename.
    for suffix in [":", "%"] {
        assert!(
            Preprocessor::new(config())
                .preprocess_str(Path::new("input.h"), &format!("#include <a{suffix}>b.h>\n"))
                .is_err()
        );
        // GNU token recombination also applies after an empty leading expansion.
        for source in [
            format!("#define EMPTY\n#include EMPTY <a{suffix}>\n"),
            format!("#define EMPTY\n#if __has_include(EMPTY <a{suffix}>)\n#endif\n"),
        ] {
            assert!(
                Preprocessor::new(config())
                    .preprocess_str(Path::new("input.h"), &source)
                    .is_err()
            );
        }
    }
}

#[test]
#[cfg(unix)]
#[ignore = "requires a native C compiler (set CC) and POSIX header filenames"]
fn native_header_delimiters_match() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let directory =
        std::env::temp_dir().join(format!("toucan-header-delimiters-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, contents) in config().virtual_headers {
        std::fs::write(directory.join(name), contents).unwrap();
    }
    for (source, accepted) in cases() {
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
            Ok(accepted),
            "{source}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result = Preprocessor::new(Config {
            include_dirs: vec![directory.clone()],
            ..Config::default()
        })
        .preprocess_str(Path::new("input.h"), &source);
        assert_eq!(result.is_ok(), accepted, "{source}\n{result:?}");
        if let Ok(result) = result {
            let compact = |text: &str| {
                text.chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
            };
            assert_eq!(
                compact(&result.source),
                compact(&String::from_utf8(output.stdout).unwrap())
            );
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}

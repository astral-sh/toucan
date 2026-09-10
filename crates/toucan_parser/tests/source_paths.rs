extern crate toucan_parser;

use std::process::Command;
use toucan_parser::driver::{parse, Config, Error};

#[test]
#[ignore = "requires GCC and Clang"]
fn source_paths_are_not_compiler_arguments() {
    if let Ok(compiler) = std::env::var("TOUCAN_SOURCE_PATH_COMPILER") {
        let config = Config {
            cpp_command: compiler,
            ..Config::default()
        };
        for path in [
            std::path::PathBuf::from("@options.c"),
            std::path::PathBuf::from("./@options.c"),
            std::env::current_dir().unwrap().join("@options.c"),
        ] {
            match parse(&config, &path).unwrap_err() {
                Error::PreprocessorError(error) => {
                    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
                }
                other => panic!("unexpected error: {}", other),
            }
            assert!(!std::path::Path::new("redirected.c").exists());
        }
        for name in ["-input.c", "ordinary.c", "@directory/input.c"] {
            let parsed = parse(&config, name).unwrap();
            assert_eq!(parsed.unit.0.len(), 1, "{}", name);
            assert!(parsed.source.contains("int expected;"), "{}", name);
            assert!(!std::path::Path::new("redirected.c").exists());
        }
        return;
    }

    // Each compiler runs in a child so relative paths do not require changing
    // the working directory of the other tests in this process.
    let directory = std::env::temp_dir().join(format!(
        "toucan-source-paths-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    for name in ["@options.c", "-input.c", "ordinary.c"] {
        std::fs::write(directory.join(name), "int expected;\n").unwrap();
    }
    std::fs::create_dir(directory.join("@directory")).unwrap();
    std::fs::write(directory.join("@directory/input.c"), "int expected;\n").unwrap();
    // Treating @options.c as a response file would consume this different file
    // and let its contents redirect the compiler's output.
    std::fs::write(directory.join("options.c"), "ordinary.c -o redirected.c\n").unwrap();
    for compiler in ["gcc", "clang"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "source_paths_are_not_compiler_arguments",
                "--ignored",
                "--nocapture",
            ])
            .env("TOUCAN_SOURCE_PATH_COMPILER", compiler)
            .current_dir(&directory)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}: {}{}",
            compiler,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

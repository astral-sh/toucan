use std::io::{self, Write};

use toucan_bindgen::{Builder, Formatter};

fn builder(directory: &std::path::Path) -> Builder {
    let header = directory.join("api.h");
    std::fs::write(
        &header,
        "struct Item { int x; long y; }; int accept(struct Item *);\n",
    )
    .unwrap();
    Builder::default()
        .header(header.to_str().unwrap())
        .layout_tests(false)
        .raw_line("#![allow(non_upper_case_globals)]")
        .raw_line("// caller: 🐦\npub type   Caller=  i32;")
}

#[test]
fn formatter_names_and_frontend_version_are_explicit() {
    assert_eq!(Formatter::default(), Formatter::Rustfmt);
    for (name, formatter) in [("none", Formatter::None), ("rustfmt", Formatter::Rustfmt)] {
        assert_eq!(name.parse::<Formatter>().unwrap(), formatter);
        assert_eq!(formatter.to_string(), name);
    }
    for name in ["prettyplease", "Rustfmt", ""] {
        assert_eq!(
            name.parse::<Formatter>().unwrap_err(),
            format!("`{name}` is not a valid formatter")
        );
    }
    let version = toucan_bindgen::clang_version();
    assert_eq!(version.parsed, None);
    assert_eq!(
        version.full,
        format!("Toucan {} (no libclang)", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn borrowed_writers_and_files_preserve_raw_lines_and_propagate_errors() {
    struct BrokenWriter(usize);
    impl Write for BrokenWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.0 == 0 {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "writer failed"));
            }
            let length = bytes.len().min(self.0);
            self.0 -= length;
            Ok(length)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let bindings = builder(directory.path())
        .formatter(Formatter::None)
        .generate()
        .unwrap();
    let mut bytes = Vec::new();
    bindings.write(Box::new(&mut bytes)).unwrap();
    let source = bindings.to_string();
    assert_eq!(bytes, source.as_bytes());
    assert!(source.find("Do not edit").unwrap() < source.find("pub type   Caller=  i32;").unwrap());
    assert!(
        source.find("pub type   Caller=  i32;").unwrap() < source.find("pub struct Item").unwrap()
    );
    assert_eq!(source.matches("pub type   Caller=  i32;").count(), 1);
    for budget in [0, 10, source.len() - 1] {
        assert_eq!(
            bindings
                .write(Box::new(BrokenWriter(budget)))
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
    }
    let file = directory.path().join("bindings.rs");
    std::fs::write(&file, "a longer previous file".repeat(source.len())).unwrap();
    bindings.write_to_file(&file).unwrap();
    assert_eq!(std::fs::read(file).unwrap(), bytes);
    assert!(
        bindings
            .write_to_file(directory.path().join("missing/bindings.rs"))
            .is_err()
    );
}

#[cfg(unix)]
#[test]
fn rustfmt_is_deferred_and_uses_target_edition_with_explicit_path() {
    let directory = tempfile::tempdir().unwrap();
    let formatter = mock(
        directory.path(),
        "cat > input\nprintf '%s\\n' \"$@\" > args\nprintf 'call\\n' >> calls\nprintf '// formatted\\n'\n",
    );
    let binding_builder = builder(directory.path()).with_rustfmt(&formatter);
    let bindings = binding_builder.clone().generate().unwrap();
    assert!(!directory.path().join("calls").exists());
    for _ in 0..2 {
        let source = bindings.to_string();
        assert!(source.ends_with("// formatted\n"));
        assert!(source.contains("pub type   Caller=  i32;"));
    }
    assert_eq!(
        std::fs::read_to_string(directory.path().join("calls")).unwrap(),
        "call\ncall\n"
    );
    let input = std::fs::read_to_string(directory.path().join("input")).unwrap();
    assert!(input.contains("pub struct Item"));
    assert!(!input.contains("Caller"));
    assert!(!input.contains("Do not edit"));
    assert_eq!(
        std::fs::read_to_string(directory.path().join("args")).unwrap(),
        "--edition\n2021\n"
    );
    binding_builder
        .clone()
        .rust_target(toucan_bindgen::RustTarget::stable(85, 0).unwrap())
        .formatter(Formatter::None)
        .rustfmt_configuration_file(Some("a config.toml".into()))
        .generate()
        .unwrap()
        .to_string();
    assert_eq!(
        std::fs::read_to_string(directory.path().join("args")).unwrap(),
        "--config-path\na config.toml\n--edition\n2024\n"
    );
    let calls = std::fs::read(directory.path().join("calls")).unwrap();
    binding_builder
        .formatter(Formatter::None)
        .generate()
        .unwrap()
        .to_string();
    assert_eq!(
        std::fs::read(directory.path().join("calls")).unwrap(),
        calls
    );
}

#[cfg(unix)]
#[test]
fn formatter_failures_fall_back_and_partial_success_preserves_output() {
    let directory = tempfile::tempdir().unwrap();
    let binding_builder = builder(directory.path());
    let unformatted = binding_builder
        .clone()
        .formatter(Formatter::None)
        .generate()
        .unwrap()
        .to_string();
    for body in [
        "cat >/dev/null\nprintf '// unusable\\n'\nexit 1\n",
        "cat >/dev/null\nprintf '// unusable\\n'\nexit 2\n",
        "cat >/dev/null\nprintf '\\377'\nexit 0\n",
        "cat >/dev/null\nprintf '\\377'\nexit 2\n",
    ] {
        let formatter = mock(directory.path(), body);
        assert_eq!(
            binding_builder
                .clone()
                .with_rustfmt(formatter)
                .generate()
                .unwrap()
                .to_string(),
            unformatted
        );
    }
    assert_eq!(
        binding_builder
            .clone()
            .with_rustfmt(directory.path().join("missing"))
            .generate()
            .unwrap()
            .to_string(),
        unformatted
    );
    let formatter = mock(
        directory.path(),
        "cat >/dev/null\nprintf '// partial\\n'\nexit 3\n",
    );
    assert!(
        binding_builder
            .with_rustfmt(formatter)
            .generate()
            .unwrap()
            .to_string()
            .ends_with("// partial\n")
    );
}

#[cfg(unix)]
fn mock(directory: &std::path::Path, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = directory.join("mock rustfmt");
    // The executable path deliberately contains a space to check process arguments.
    std::fs::write(
        &path,
        format!("#!/bin/sh\ncd \"$(dirname \"$0\")\" || exit 1\n{body}"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[test]
#[ignore = "requires rustfmt on PATH"]
fn installed_rustfmt_formats_only_generated_declarations() {
    let directory = tempfile::tempdir().unwrap();
    let source = builder(directory.path()).generate().unwrap().to_string();
    assert!(source.contains("pub struct Item {\n    pub x:"));
    assert!(source.contains("pub type   Caller=  i32;"));
    let output = directory.path().join("bindings.rs");
    std::fs::write(&output, source).unwrap();
    assert!(
        std::process::Command::new("rustc")
            .args(["--edition", "2021", "--crate-type", "lib"])
            .arg(&output)
            .arg("--out-dir")
            .arg(directory.path())
            .status()
            .unwrap()
            .success()
    );
}

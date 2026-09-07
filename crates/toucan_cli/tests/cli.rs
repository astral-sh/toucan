use std::process::Command;

#[test]
fn generation_does_not_need_a_compiler_on_path() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("api.h");
    let output = directory.path().join("bindings.rs");
    let report = directory.path().join("report.json");
    std::fs::write(&header, "#include <stddef.h>\n#define API_VERSION 7U\nstruct Item { size_t size; const char *name; };\nint consume(const struct Item *item);\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_toucan"))
        .env("PATH", directory.path())
        .arg("bindgen")
        .arg(&header)
        .arg("--target")
        .arg("x86_64-unknown-linux-gnu")
        .arg("--allowlist")
        .arg("consume")
        .arg("--allowlist")
        .arg("API_*")
        .arg("--output")
        .arg(&output)
        .arg("--report")
        .arg(&report)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source = std::fs::read_to_string(output).unwrap();
    assert!(source.contains("pub fn consume"));
    assert!(source.contains("pub const API_VERSION: ::core::primitive::u32 = 7;"));
    assert!(source.contains("pub struct Item"));
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(report["integer_macros"], 1);
}

#[test]
fn failed_generation_preserves_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("bad.h");
    let output = directory.path().join("bindings.rs");
    std::fs::write(&header, "#error deliberate failure\n").unwrap();
    std::fs::write(&output, "existing bindings\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_toucan"))
        .arg("bindgen")
        .arg(&header)
        .arg("--target")
        .arg("x86_64-unknown-linux-gnu")
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("deliberate failure"));
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        "existing bindings\n"
    );
}

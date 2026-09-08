use std::process::Command;

#[test]
fn cli_dll_rules_preserve_explicit_scope_without_a_compiler() {
    let dir = tempfile::tempdir().unwrap();
    let header = dir.path().join("api.h");
    std::fs::write(&header,"__declspec(dllimport) int a_value; __declspec(dllimport) int b_value; __declspec(dllimport) int omitted; int ordinary;").unwrap();
    let run = |rules: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_toucan"));
        command
            .env("PATH", dir.path())
            .arg("bindgen")
            .arg(&header)
            .args([
                "--target",
                "x86_64-pc-windows-msvc",
                "--allowlist",
                "a_*",
                "--allowlist",
                "b_*",
                "--allowlist",
                "ordinary",
            ]);
        for rule in rules {
            command.args(["--dll-import-library", rule]);
        }
        command.output().unwrap()
    };
    let output = run(&["a_*=old", "a_*=alpha", "b_value=beta", "ordinary=wrong"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = String::from_utf8(output.stdout).unwrap();
    assert_eq!(source.matches("#[link(").count(), 2, "{source}");
    assert!(source.contains("name = \"alpha\""));
    assert!(source.contains("name = \"beta\""));
    assert!(!source.contains("old"));
    assert!(!source.contains("wrong"));
    assert!(!source.contains("omitted"));
    for rule in ["missing_equals", "=library", "bad*=", "a.*=library"] {
        let output = run(&[rule]);
        assert!(!output.status.success(), "{rule}");
    }
    let output = run(&["a_*=alpha"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("b_value"));
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
#[ignore = "requires Clang, llvm-readobj, Python, and the Rust MSVC toolchain"]
fn native_windows_dll_data_calls_and_addresses() {
    let temporary = tempfile::tempdir().unwrap();
    let output = std::env::var_os("TOUCAN_WINDOWS_DLL_OUTPUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned());
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/verify_windows_dll_imports.py");
    let mut command = Command::new("python");
    command
        .arg(script)
        .arg("--toucan")
        .arg(env!("CARGO_BIN_EXE_toucan"))
        .arg("--output")
        .arg(output)
        .arg("--native");
    if let Ok(rustcs) = std::env::var("TOUCAN_WINDOWS_DLL_RUSTC_JSON") {
        let rustcs: Vec<String> = serde_json::from_str(&rustcs)
            .expect("TOUCAN_WINDOWS_DLL_RUSTC_JSON must be an array of rustc paths");
        assert!(!rustcs.is_empty(), "at least one rustc path is required");
        for rustc in rustcs {
            command.arg("--rustc").arg(rustc);
        }
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

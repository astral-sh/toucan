use std::process::Command;

#[test]
fn inspection_records_compiler_with_vla_identity_shape_versions() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("profile.h");
    std::fs::write(
        &header,
        "#ifndef __clang__\n#error expected Clang\n#endif\nint f(int n){ return n; }\nvoid visit(int n, int a[][n]);\n",
    )
    .unwrap();
    for checked in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_toucan"));
        command
            .env("PATH", directory.path())
            .arg("inspect")
            .arg(&header)
            .args([
                "--target",
                "x86_64-unknown-linux-gnu",
                "--compiler",
                "clang",
            ]);
        if checked {
            command.arg("--checked-code");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["schema_version"], if checked { 5 } else { 3 });
        assert_eq!(json["translation_unit"]["compiler"], "clang");
        assert_eq!(json["translation_unit"]["target"], "X86_64UnknownLinuxGnu");
        let visit = json["translation_unit"]["declarations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == "visit")
            .unwrap();
        assert!(visit["ty"]["kind"]["Function"]["parameters"][1]["ty"]["kind"]
            ["Pointer"]["kind"]["VariableArray"]["identity"].as_u64().unwrap() > 0);
    }
}

#[test]
fn invalid_profiles_fail_before_reading_input_or_replacing_output() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.h");
    let output = directory.path().join("bindings.rs");
    std::fs::write(&output, "existing\n").unwrap();
    for (target, compiler) in [
        ("x86_64-apple-darwin", "gcc"),
        ("x86_64-pc-windows-msvc", "gcc"),
        ("x86_64-unknown-linux-gnu", "msvc"),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_toucan"))
            .arg("bindgen")
            .arg(&missing)
            .args(["--target", target, "--compiler", compiler, "--output"])
            .arg(&output)
            .output()
            .unwrap();
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains("compiler"), "{error}");
        assert!(!error.contains("failed to read"), "{error}");
        assert_eq!(std::fs::read_to_string(&output).unwrap(), "existing\n");
    }
}

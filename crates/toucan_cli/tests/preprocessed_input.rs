use std::process::Command;

#[test]
fn compiler_output_keeps_header_locations_through_checked_inspection() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("main.c");
    let header = directory.path().join("helper.h");
    let preprocessed = directory.path().join("main.i");
    std::fs::write(&header, "static int helper(int x) { return x + 1; }\n").unwrap();
    std::fs::write(
        &source,
        "#include \"helper.h\"\nint public(int x) { return helper(x); }\n",
    )
    .unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc.as_str(), "clang"] {
        let output = Command::new(compiler)
            .arg("-E")
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(&preprocessed, output.stdout).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_toucan"))
            .env("PATH", directory.path())
            .arg("inspect")
            .arg(&preprocessed)
            .arg("--checked-code")
            .args(["--target", "x86_64-unknown-linux-gnu"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let analysis: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            analysis["checked_code"]["bodies"].as_array().unwrap().len(),
            2
        );
        let generated = analysis["preprocessed"]["source"].as_str().unwrap();
        let mappings = analysis["preprocessed"]["mappings"].as_array().unwrap();
        for (name, path, line) in [("helper", &header, 1), ("public", &source, 2)] {
            let offset = generated.find(name).unwrap() as u64;
            let location = mappings
                .iter()
                .find(|mapping| {
                    mapping["generated"]["start"].as_u64().unwrap() <= offset
                        && offset < mapping["generated"]["end"].as_u64().unwrap()
                })
                .unwrap();
            assert_eq!(location["origin"]["path"], path.to_str().unwrap());
            assert_eq!(location["origin"]["line"], line);
        }
    }
}

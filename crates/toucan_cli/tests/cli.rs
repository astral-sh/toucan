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

#[test]
fn source_date_epoch_controls_preprocessing_and_bindings() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("date.h");
    let output = directory.path().join("bindings.rs");
    std::fs::write(&header, "#include \"inner.h\"\nconst char date[] = __DATE__;\n#define BUILD_DATE __DATE__\n#define BUILD_TIME __TIME__\n").unwrap();
    std::fs::write(
        directory.path().join("inner.h"),
        "const char time[] = __TIME__;\n",
    )
    .unwrap();
    for (epoch, date, time) in [
        ("0", "Jan  1 1970", "00:00:00"),
        ("1709251199", "Feb 29 2024", "23:59:59"),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_toucan"))
            .env("SOURCE_DATE_EPOCH", epoch)
            .env("TZ", "Pacific/Honolulu")
            .env("PATH", directory.path())
            .arg("preprocess")
            .arg(&header)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let source = String::from_utf8(result.stdout).unwrap();
        assert!(
            source.contains(&format!("const char time [ ] = \"{time}\" ;")),
            "{source}"
        );
        assert!(
            source.contains(&format!("const char date [ ] = \"{date}\" ;")),
            "{source}"
        );
    }
    let result = Command::new(env!("CARGO_BIN_EXE_toucan"))
        .env("SOURCE_DATE_EPOCH", "0")
        .arg("bindgen")
        .arg(&header)
        .args(["--allowlist", "BUILD*", "--generate-cstr"])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source = std::fs::read_to_string(&output).unwrap();
    assert!(
        source.contains("pub const BUILD_DATE: &::core::ffi::CStr"),
        "{source}"
    );
    assert!(
        source.contains("pub const BUILD_TIME: &::core::ffi::CStr"),
        "{source}"
    );
}

#[test]
fn invalid_source_date_epoch_preserves_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("date.h");
    let output = directory.path().join("bindings.rs");
    std::fs::write(&header, "int value;\n").unwrap();
    std::fs::write(&output, "existing bindings\n").unwrap();
    let mut invalid: Vec<std::ffi::OsString> = [
        "",
        "-1",
        "+1",
        " 1",
        "1 ",
        "1.5",
        "253402300800",
        "18446744073709551616",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        invalid.push(std::ffi::OsString::from_vec(vec![0xff]));
    }
    for epoch in invalid {
        let result = Command::new(env!("CARGO_BIN_EXE_toucan"))
            .env("SOURCE_DATE_EPOCH", &epoch)
            .arg("bindgen")
            .arg(&header)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(!result.status.success(), "{epoch:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("invalid SOURCE_DATE_EPOCH"));
        assert_eq!(
            std::fs::read_to_string(&output).unwrap(),
            "existing bindings\n"
        );
    }
}

#[test]
fn cli_captures_the_current_utc_time_without_source_date_epoch() {
    use std::time::{SystemTime, UNIX_EPOCH};
    use toucan::{PreprocessingTimestamp, Preprocessor, PreprocessorConfig};

    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("clock.h");
    std::fs::write(&header, "__DATE__ __TIME__\n").unwrap();
    let before = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let result = Command::new(env!("CARGO_BIN_EXE_toucan"))
        .env_remove("SOURCE_DATE_EPOCH")
        .env("TZ", "Pacific/Honolulu")
        .arg("preprocess")
        .arg(&header)
        .output()
        .unwrap();
    let after = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let actual = String::from_utf8(result.stdout).unwrap();
    let matched = (before..=after).any(|seconds| {
        let config = PreprocessorConfig {
            timestamp: PreprocessingTimestamp::from_unix_seconds(seconds).unwrap(),
            ..PreprocessorConfig::default()
        };
        actual.ends_with(
            &Preprocessor::new(config)
                .preprocess_str(&header, "__DATE__ __TIME__\n")
                .unwrap()
                .source,
        )
    });
    assert!(matched, "{actual}");
}

#[test]
fn unsupported_floating_macros_are_reported_and_can_fail_generation() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("floating.h");
    let output = directory.path().join("bindings.rs");
    let report = directory.path().join("report.json");
    std::fs::write(
        &header,
        "#define FINITE 0.1f\n#define UNSUPPORTED 1.0L\n#define NAN_VALUE __builtin_nan(\"\")\n",
    )
    .unwrap();
    std::fs::write(&output, "existing bindings\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_toucan"))
        .arg("bindgen")
        .arg(&header)
        .arg("--deny-skipped-macros")
        .arg("--output")
        .arg(&output)
        .arg("--report")
        .arg(&report)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(
        std::fs::read_to_string(&output).unwrap(),
        "existing bindings\n"
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(report["floating_macros"], 1);
    assert!(
        report["skipped_macros"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "UNSUPPORTED"
                && item["reason"].as_str().unwrap().contains("long double"))
    );
    assert!(
        report["skipped_macros"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "NAN_VALUE"
                && item["reason"].as_str().unwrap().contains("non-finite"))
    );
    let result = Command::new(env!("CARGO_BIN_EXE_toucan"))
        .arg("bindgen")
        .arg(&header)
        .args([
            "--deny-skipped-macros",
            "--allowlist",
            "FINITE",
            "--rust-target",
            "1.64",
        ])
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source = std::fs::read_to_string(output).unwrap();
    assert!(source.contains("pub const FINITE: ::core::primitive::f32"));
    assert!(source.contains("0x3dcccccd"));
}

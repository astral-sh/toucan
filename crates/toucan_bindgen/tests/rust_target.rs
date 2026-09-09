use std::io;

use toucan_bindgen::{Builder, Formatter, RustTarget};

#[test]
fn stable_strings_accept_patch_versions_and_the_existing_numeric_range() {
    for (input, minor, patch) in [
        ("1.64", 64, 0),
        ("1.64.0", 64, 0),
        ("1.64.9", 64, 9),
        ("1.070.001", 70, 1),
        ("1.+85.+2", 85, 2),
        ("1.65535.18446744073709551615", 65535, u64::MAX),
    ] {
        // The error type is part of bindgen's parsing API, including `?` callers.
        let parsed: io::Result<RustTarget> = input.parse();
        assert_eq!(parsed.unwrap(), RustTarget::stable(minor, patch).unwrap());
    }
    assert_eq!("1.64".parse::<RustTarget>().unwrap(), RustTarget::default());
}

#[test]
fn invalid_and_unsupported_targets_are_invalid_input_errors() {
    for (input, message) in [
        ("nightly", "nightly Rust targets are not supported"),
        ("1.63.0", "require Rust 1.64 or newer"),
        ("1.65536", "minor version exceeds 65535"),
        ("1.18446744073709551615", "minor version exceeds 65535"),
        ("1.18446744073709551616", "minor version number must be"),
        ("1.64.18446744073709551616", "patch version number must be"),
        ("1.-64", "minor version number must be"),
        ("1.64.-1", "patch version number must be"),
        ("1.64.0.0", "patch version number must be"),
        ("1.64.", "patch version number must be"),
        ("1.64-beta", "minor version number must be"),
        ("1..0", "minor version number must be"),
        ("1.", "minor version number must be"),
        (" 1.64", "largest major version"),
        ("1.64 ", "minor version number must be"),
        ("1.64\0", "minor version number must be"),
        ("2.64", "largest major version"),
        ("64", "accepted stable targets"),
        ("", "accepted stable targets"),
    ] {
        let error = input.parse::<RustTarget>().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{input}");
        assert!(error.to_string().contains(message), "{input}: {error}");
    }
}

#[test]
fn parsed_targets_keep_constructor_output_and_reports() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let header = directory.path().join("target.h");
    std::fs::write(
        &header,
        "struct Pair {int x; int y;}; void use_pair(struct Pair*);\n",
    )?;
    let builder = Builder::default()
        .header(header.to_str().unwrap())
        .clang_arg("--target=x86_64-unknown-linux-gnu")
        .formatter(Formatter::None);
    for (input, minor) in [("1.64", 64), ("1.77.1", 77), ("1.82.0", 82), ("1.85", 85)] {
        let parsed = builder.clone().rust_target(input.parse()?).generate()?;
        let explicit = builder
            .clone()
            .rust_target(RustTarget::stable(minor, 0)?)
            .generate()?;
        assert_eq!(parsed.to_string(), explicit.to_string());
        assert_eq!(parsed.report().rust_target, explicit.report().rust_target);
    }
    Ok(())
}

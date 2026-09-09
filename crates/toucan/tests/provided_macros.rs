use std::collections::BTreeMap;
use std::path::Path;

use toucan::{
    BindingOptions, BindingSelection, Config, EnumConstantStyle, MacroEvaluation, MacroType,
    MacroValue, RustTarget, SkippedMacro, Target,
};

fn compilation() -> toucan::Compilation {
    toucan::parse_source(
        Path::new("provided.h"),
        "#define NATIVE 123u\nenum Kind { ITEM=1 };\nstruct Record { unsigned value; };\n",
        &Config::new(Target::X86_64UnknownLinuxGnu),
    )
    .unwrap()
}

#[test]
fn supplied_values_keep_c_declarations_and_report_their_provenance() {
    let compilation = compilation();
    let options = BindingOptions {
        prepend_enum_name: true,
        enum_constant_style: EnumConstantStyle::Bindgen,
        macro_type: MacroType::Unsigned,
        ..BindingOptions::default()
    };
    let (native, native_report) = compilation.bindings(&options).unwrap();
    assert!(native.contains("pub const NATIVE:"));
    assert_eq!(native_report.macro_evaluation, MacroEvaluation::CExpression);
    assert!(
        serde_json::to_value(native_report)
            .unwrap()
            .get("macro_evaluation")
            .is_none()
    );
    let macros = BTreeMap::from([
        (
            "ITEM".into(),
            Some(MacroValue::RustInteger {
                value: 2,
                bits: 8,
                signed: false,
            }),
        ),
        (
            "NEGATIVE".into(),
            Some(MacroValue::RustInteger {
                value: 128,
                bits: 8,
                signed: true,
            }),
        ),
        ("FLOAT".into(), Some(MacroValue::RustFloat(-0.0))),
    ]);
    let (source, report) = compilation
        .bindings_with_macros(
            &options,
            &macros,
            vec![SkippedMacro {
                name: "NATIVE".into(),
                reason: "not supplied by this provider".into(),
            }],
        )
        .unwrap();
    assert!(source.contains("pub struct Record"));
    assert!(source.contains("pub const Kind_ITEM: ::core::primitive::u32 = 1;"));
    assert!(source.contains("pub const ITEM: ::core::primitive::u8 = 2;"));
    assert!(source.contains("pub const NEGATIVE: ::core::primitive::i8 = -128;"));
    assert!(!source.contains("pub const NATIVE:"));
    assert_eq!(report.macro_evaluation, MacroEvaluation::Provided);
    assert_eq!((report.integer_macros, report.floating_macros), (2, 1));
    assert!(report.macro_types.is_empty(), "no invented C macro types");
    assert_eq!(report.skipped_macros[0].name, "NATIVE");
    assert_eq!(
        serde_json::to_value(report).unwrap()["macro_evaluation"],
        "provided"
    );
    // Supplying values never changes the frontend's final C macro environment.
    assert_eq!(compilation.bindings(&options).unwrap().0, native);

    let selected = BindingOptions {
        selection: Some(Box::new(BindingSelection {
            macros: ["ITEM".into()].into_iter().collect(),
            ..BindingSelection::default()
        })),
        ..options
    };
    let (source, report) = compilation
        .bindings_with_macros(&selected, &macros, Vec::new())
        .unwrap();
    assert!(source.contains("pub const ITEM:"));
    assert!(!source.contains("pub const Kind_ITEM:"));
    assert!(!source.contains("pub const FLOAT:"));
    assert!(!source.contains("pub struct Record"));
    assert_eq!((report.integer_macros, report.floating_macros), (1, 0));
}

#[test]
fn supplied_integer_storage_is_validated_before_emission() {
    let compilation = compilation();
    for (bits, value, message) in [
        (7, 1, "unsupported integer width"),
        (8, 256, "integer constant exceeds its declared width"),
    ] {
        let macros = BTreeMap::from([(
            "INVALID".into(),
            Some(MacroValue::RustInteger {
                value,
                bits,
                signed: true,
            }),
        )]);
        let error = compilation
            .bindings_with_macros(&BindingOptions::default(), &macros, Vec::new())
            .unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
#[ignore = "requires native rustc; TOUCAN_TEST_RUST_TOOLCHAIN selects Rust 1.64"]
fn supplied_integer_and_float_bits_survive_rust_compilation() {
    use std::process::Command;
    use toucan_test_support::compiler_acceptance;

    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let compilation = toucan::parse_source(Path::new("empty.h"), "", &Config::new(target)).unwrap();
    let macros = BTreeMap::from([
        (
            "MINIMUM".into(),
            Some(MacroValue::RustInteger {
                value: 128,
                bits: 8,
                signed: true,
            }),
        ),
        (
            "MAXIMUM".into(),
            Some(MacroValue::RustInteger {
                value: u128::MAX,
                bits: 128,
                signed: false,
            }),
        ),
        ("NEGATIVE_ZERO".into(), Some(MacroValue::RustFloat(-0.0))),
        (
            "PAYLOAD".into(),
            Some(MacroValue::RustFloat(f64::from_bits(0x7ff0000000000042))),
        ),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let (source, _) = compilation
        .bindings_with_macros(
            &BindingOptions {
                rust_target: RustTarget::RUST_1_64,
                ..BindingOptions::default()
            },
            &macros,
            Vec::new(),
        )
        .unwrap();
    let input = directory.path().join("probe.rs");
    let output = directory.path().join("probe");
    std::fs::write(
        &input,
        format!(
            "{source}\nfn main() {{\nassert_eq!(MINIMUM, i8::MIN);\nassert_eq!(MAXIMUM, u128::MAX);\nassert_eq!(NEGATIVE_ZERO.to_bits(), 0x8000000000000000);\nassert_eq!(PAYLOAD.to_bits(), 0x7ff0000000000042);\n}}\n"
        ),
    )
    .unwrap();
    let mut command = Command::new("rustc");
    if let Some(toolchain) = std::env::var_os("TOUCAN_TEST_RUST_TOOLCHAIN") {
        command.arg(format!("+{}", toolchain.to_string_lossy()));
    }
    let result = command
        .args(["--edition=2021", "-Dwarnings"])
        .arg(input)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(
        compiler_acceptance(&result),
        Ok(true),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(Command::new(output).status().unwrap().success());
}

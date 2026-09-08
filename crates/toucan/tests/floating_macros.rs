use std::path::Path;
use std::process::Command;

use toucan::{BindingOptions, Config, RustTarget, Target};

const CASES: &[(&str, &str, &str)] = &[
    ("ZERO", "0.0f", "float"),
    ("NEGATIVE_ZERO", "-0.0f", "float"),
    ("DECIMAL", "0.1f", "float"),
    ("SUBNORMAL", "0x1p-149f", "float"),
    ("MAXIMUM", "0x1.fffffep127f", "float"),
    ("UNDERFLOW", "-0x1p-149f / 2.0f", "float"),
    ("TIE", "1.0f + 0x1p-24f", "float"),
    ("INTEGER_CAST", "(float)0x8000008000000001ULL", "float"),
    ("DOUBLE_CAST", "(float)(1.0 + 0x1p-24)", "float"),
    ("ROUNDING", "(16777216.0f + 1.0f) - 16777216.0f", "float"),
    ("DOUBLE_ZERO", "-0.0", "double"),
    ("DOUBLE_DECIMAL", "0.1", "double"),
    ("THIRD", "1.0 / 3.0", "double"),
    ("DOUBLE_SUBNORMAL", "0x1p-1074", "double"),
    ("DOUBLE_MAXIMUM", "0x1.fffffffffffffp1023", "double"),
    ("PROMOTED", "1 ? 7 : 0.0", "double"),
    ("LONG_CAST", "(double)(~0UL)", "double"),
    (
        "EXTENDED_CAST",
        "(double)((0x1p63L + 1.0L) - 0x1p63L)",
        "double",
    ),
    (
        "QUAD_CAST",
        "(double)((0x1p100L + 1.0L) - 0x1p100L)",
        "double",
    ),
];

fn parse(source: &str, target: Target) -> toucan::Compilation {
    let mut config = Config::new(target);
    config.preprocessor.allow_filesystem = false;
    config.preprocessor.defines.clear();
    toucan::parse_source(Path::new("floating.h"), source, &config).unwrap()
}

#[test]
fn floating_macros_keep_types_and_report_unsupported_values() {
    let source = "#define FLOAT 0.1f\n#define DOUBLE (-0.0)\n#define INTEGER (int)(1.0 + 2.0)\n#define COMPARE (1.0 < 2.0)\n#define TEXT \"same\"\n#define OLD_INTEGER 7U\n#define WIDE 1.0L\n#define OVERFLOW 1e9999\n#define DIVZERO (1.0 / 0.0)\n#define NAN_VALUE __builtin_nanf(\"\")\n#define INFINITY_VALUE __builtin_inf()\n";
    for target in Target::ALL {
        let compilation = parse(source, target);
        for minor in [64, 82, 83, 96] {
            let (bindings, report) = compilation
                .bindings(&BindingOptions {
                    rust_target: RustTarget::stable(minor).unwrap(),
                    ..BindingOptions::default()
                })
                .unwrap();
            assert_eq!(report.floating_macros, 2);
            assert_eq!(report.integer_macros, 3);
            assert_eq!(report.string_macros, 1);
            assert!(bindings.contains("pub const FLOAT: ::core::primitive::f32"));
            assert!(bindings.contains("pub const DOUBLE: ::core::primitive::f64"));
            assert!(bindings.contains("0x3dcccccd") && bindings.contains("0x8000000000000000"));
            assert_eq!(bindings.contains("::from_bits("), minor >= 83);
            assert_eq!(bindings.contains("::mem::transmute"), minor < 83);
            assert!(bindings.contains("pub const OLD_INTEGER: ::core::primitive::u32 = 7;"));
            assert!(bindings.contains("pub const INTEGER: ::core::primitive::i32 = 3;"));
            assert!(bindings.contains("pub const COMPARE: ::core::primitive::i32 = 1;"));
            assert!(bindings.contains(
                "pub const TEXT: &[::core::primitive::u8; 5] = &[115, 97, 109, 101, 0];"
            ));
            for (name, reason) in [
                ("WIDE", "long double"),
                ("OVERFLOW", "overflow"),
                ("DIVZERO", "division by zero"),
                ("NAN_VALUE", "non-finite"),
                ("INFINITY_VALUE", "non-finite"),
            ] {
                assert!(
                    report
                        .skipped_macros
                        .iter()
                        .any(|item| item.name == name && item.reason.contains(reason)),
                    "{name}: {:?}",
                    report.skipped_macros
                );
                assert!(!bindings.contains(&format!("pub const {name}:")));
            }
        }
    }
}

#[test]
fn floating_macro_shadowing_and_name_collisions_follow_existing_rules() {
    let compilation = parse(
        "enum { VALUE = 1, OMITTED = 2, self = 3, __toucan_self = 4 };\n#define VALUE 0.5f\n#define OMITTED 1.0L\n#define self -0.0\n",
        Target::X86_64UnknownLinuxGnu,
    );
    let (bindings, report) = compilation.bindings(&BindingOptions::default()).unwrap();
    assert!(bindings.contains("pub const VALUE: ::core::primitive::f32"));
    assert!(!bindings.contains("pub const OMITTED:"));
    assert_eq!(report.renamed_macros["__toucan_self_"], "self");
    assert_eq!(compilation.unit.constants["VALUE"].value, 1);
    assert_eq!(report.enum_constants[0].emitted.len(), 1);
    for declaration in [
        "extern double value;",
        "typedef float value;",
        "struct value { int x; };",
        "double value(void);",
    ] {
        let compilation = parse(
            &format!("{declaration}\n#define value 0.5\n"),
            Target::X86_64UnknownLinuxGnu,
        );
        assert!(
            compilation
                .bindings(&BindingOptions::default())
                .unwrap_err()
                .to_string()
                .contains("macro `value` conflicts")
        );
        assert!(
            compilation
                .bindings(&BindingOptions {
                    allowlist: vec!["unrelated".into()],
                    ..Default::default()
                })
                .is_ok()
        );
    }
}

#[test]
#[ignore = "requires native C compilers, ar, and rustc; run with --include-ignored"]
fn generated_floating_constants_match_native_c_calls() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let mut header = String::new();
    let mut implementation = String::from("#include \"api.h\"\n");
    let mut assertions = String::new();
    for (index, &(name, expression, c_type)) in CASES.iter().enumerate() {
        header.push_str(&format!(
            "#define {name} ({expression})\n{c_type} native_{index}(void);\n"
        ));
        implementation.push_str(&format!("_Static_assert(_Generic(({name}), {c_type}:1, default:0), \"macro type\");\n{c_type} native_{index}(void) {{ return {name}; }}\n"));
        assertions.push_str(&format!(
            "assert_eq!({name}.to_bits(), unsafe {{ native_{index}() }}.to_bits(), \"{name}\");\n"
        ));
    }
    let compilation = parse(&header, target);
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("api.h"), &header).unwrap();
    std::fs::write(directory.path().join("probe.c"), implementation).unwrap();
    let compilers: &[&str] = if cfg!(target_os = "linux") {
        &["gcc", "clang"]
    } else {
        &["clang"]
    };
    let toolchain = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN").ok();
    let versions: &[u16] = if toolchain.is_some() {
        &[64]
    } else {
        &[64, 83, 96]
    };
    for compiler in compilers {
        let output = Command::new(compiler)
            .args([
                "-std=c11",
                "-O2",
                "-ffp-contract=off",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-c",
                "probe.c",
                "-o",
                "probe.o",
            ])
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            Command::new("ar")
                .args(["rcs", "libprobe.a", "probe.o"])
                .current_dir(directory.path())
                .status()
                .unwrap()
                .success()
        );
        for &minor in versions {
            let (bindings, report) = compilation
                .bindings(&BindingOptions {
                    rust_target: RustTarget::stable(minor).unwrap(),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(report.floating_macros, CASES.len());
            assert!(report.skipped_macros.is_empty());
            let source = format!(
                "#![allow(dead_code, non_camel_case_types, non_snake_case)]\n{bindings}\nfn main() {{ {assertions} }}\n"
            );
            std::fs::write(directory.path().join("probe.rs"), source).unwrap();
            let mut command = match &toolchain {
                Some(toolchain) => {
                    let mut command = Command::new("rustup");
                    command.args(["run", toolchain, "rustc"]);
                    command
                }
                None => Command::new("rustc"),
            };
            let output = command
                .args([
                    "--edition=2021",
                    "-D",
                    "improper_ctypes",
                    "probe.rs",
                    "-L",
                    ".",
                    "-l",
                    "static=probe",
                    "-o",
                    "probe",
                ])
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "Rust {minor}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                Command::new(directory.path().join("probe"))
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
}

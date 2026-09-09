use std::path::Path;
use std::process::Command;

use toucan::{BindingOptions, Config, RustTarget, Target};

fn compile(source: &str, target: Target) -> toucan::Compilation {
    let mut config = Config::new(target);
    config.preprocessor.defines.clear();
    toucan::parse_source(Path::new("api.h"), source, &config).unwrap()
}

#[test]
fn rust_version_boundaries_select_supported_syntax() {
    let compilation = compile(
        "struct Record { char first; int second; }; int call(void);",
        Target::X86_64UnknownLinuxGnu,
    );
    for minor in [64, 76, 77, 81, 82, 96] {
        let (source, report) = compilation
            .bindings(&BindingOptions {
                rust_target: RustTarget::stable(minor).unwrap(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(source.contains("unsafe extern \"C\" {"), minor >= 82);
        assert_eq!(source.contains("::core::mem::offset_of!"), minor >= 77);
        assert_eq!(source.contains("::core::ptr::addr_of!"), minor < 77);
        assert!(source.contains("assert!(::core::mem::size_of::<Record>() == 8)"));
        assert_eq!(report.rust_target, format!("1.{minor}"));
    }
    assert_eq!("1.64".parse::<RustTarget>().unwrap(), RustTarget::RUST_1_64);
    for invalid in ["1.63", "2.0", "stable", "1.64.0"] {
        assert!(invalid.parse::<RustTarget>().is_err());
    }
}

#[test]
fn old_rust_rejects_128_bit_abi_types_but_preserves_constants() {
    for source in [
        "typedef __int128_t Wide;",
        "__int128_t call(__int128_t value);",
        "void call(const __uint128_t *value);",
        "struct Wide { __int128_t value; };",
    ] {
        let compilation = compile(source, Target::X86_64UnknownLinuxGnu);
        let error = compilation
            .bindings(&BindingOptions {
                rust_target: RustTarget::RUST_1_64,
                ..Default::default()
            })
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("128-bit C ABI types require Rust 1.78"),
            "{error}"
        );
        assert!(compilation.bindings(&BindingOptions::default()).is_ok());
    }
    let compilation = compile("", Target::X86_64UnknownLinuxGnu);
    let (_, report) = compilation
        .bindings(&BindingOptions {
            rust_target: RustTarget::RUST_1_64,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(report.skipped_declarations, ["__int128_t", "__uint128_t"]);
    assert!(
        compilation
            .bindings(&BindingOptions {
                rust_target: RustTarget::RUST_1_64,
                allowlist: vec!["__int128_t".into()],
                ..Default::default()
            })
            .is_err()
    );
    let compilation = compile(
        "#define BIG ((__int128_t)1 << 100)\n",
        Target::X86_64UnknownLinuxGnu,
    );
    let (source, _) = compilation
        .bindings(&BindingOptions {
            rust_target: RustTarget::RUST_1_64,
            ..Default::default()
        })
        .unwrap();
    assert!(
        source
            .contains("pub const BIG: ::core::primitive::i128 = 1267650600228229401496703205376;")
    );
}

#[test]
fn rustified_128_bit_enums_require_rust_1_89() {
    let compilation = compile(
        "enum Wide { WIDE = (__int128_t)1 << 100 };",
        Target::X86_64UnknownLinuxGnu,
    );
    let options = BindingOptions {
        allowlist: vec!["Wide".into()],
        rustified_enums: true,
        rust_target: RustTarget::stable(88).unwrap(),
        ..Default::default()
    };
    assert!(
        compilation
            .bindings(&options)
            .unwrap_err()
            .to_string()
            .contains("require Rust 1.89")
    );
    let (source, _) = compilation
        .bindings(&BindingOptions {
            rust_target: RustTarget::stable(89).unwrap(),
            ..options
        })
        .unwrap();
    assert!(source.contains("#[repr(u128)]"));
}

#[test]
#[ignore = "requires rustc; TOUCAN_TEST_RUST_TOOLCHAIN selects an installed rustup toolchain"]
fn legacy_bindings_run_layout_and_cstr_checks() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let compilation = compile(
        "struct Record { char first; long long second; int third[2]; };\n\
         #pragma pack(push, 1)\n\
         struct Packed { char first; long long second; };\n\
         #pragma pack(pop)\n\
         union Item { long long number; char bytes[8]; };\n\
         struct Bits { unsigned first:3; unsigned second:5; unsigned char tail; };\n\
         struct Nest { union Item item; struct { int value; } member; };\n\
         int __toucan_layout_0(void); int __toucan_layout_1(void);\n\
         #define TEXT \"toucan\"\n\
         #define BIG ((__int128_t)1 << 100)\n",
        target,
    );
    let (bindings, _) = compilation
        .bindings(&BindingOptions {
            rust_target: RustTarget::RUST_1_64,
            generate_cstr: true,
            ..Default::default()
        })
        .unwrap();
    let source = format!(
        "#![allow(dead_code, non_camel_case_types, non_snake_case, non_upper_case_globals)]\n{bindings}\n#[test] fn constants() {{ assert_eq!(TEXT.to_bytes(), b\"toucan\"); assert_eq!(BIG, 1i128 << 100); }}\n"
    );
    let directory = tempfile::tempdir().unwrap();
    for (filename, source, expected_success) in [
        ("correct", source.clone(), true),
        (
            "incorrect",
            source.replacen(
                "_base as ::core::primitive::usize == 8);",
                "_base as ::core::primitive::usize == 9);",
                1,
            ),
            false,
        ),
    ] {
        let input = directory.path().join(format!("{filename}.rs"));
        let executable = directory.path().join(filename);
        std::fs::write(&input, source).unwrap();
        let mut command = match std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
            Ok(toolchain) => {
                let mut command = Command::new("rustup");
                command.args(["run", &toolchain, "rustc"]);
                command
            }
            Err(_) => Command::new("rustc"),
        };
        let result = command
            .args(["--edition=2021", "--test", "-D", "improper_ctypes"])
            .arg(&input)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = Command::new(executable).output().unwrap();
        assert_eq!(
            result.status.success(),
            expected_success,
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
    }
}

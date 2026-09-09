use std::path::Path;
use toucan::{BindingOptions, CompilerProfile, Config};

#[test]
fn binary128_macros_require_explicit_scalar_conversion() {
    for profile in CompilerProfile::ALL {
        let mut config = Config::with_profile(profile);
        config.preprocessor.allow_filesystem = false;
        config.preprocessor.defines.clear();
        let source = "#define QUAD 1.0Q\n#define COMPLEX 2.0Qi\n#define DOUBLE ((double)(1.0Q+0x1p-112Q))\n#define NEGATIVE_ZERO ((double)__real__ __builtin_complex(-0.0Q,2.0Q))\n";
        let parsed = toucan::parse_source(Path::new("quad.h"), source, &config).unwrap();
        let (bindings, report) = parsed.bindings(&BindingOptions::default()).unwrap();
        assert_eq!(report.floating_macros, 2);
        assert_eq!(report.skipped_macros.len(), 2);
        for name in ["QUAD", "COMPLEX"] {
            let skip = report
                .skipped_macros
                .iter()
                .find(|entry| entry.name == name)
                .unwrap();
            assert!(skip.reason.contains("Rust"), "{profile:?}: {}", skip.reason);
        }
        assert!(bindings.contains("0x3ff0000000000000"));
        assert!(bindings.contains("0x8000000000000000"));
    }
}

#[test]
fn unproved_binary128_storage_and_calls_have_explicit_diagnostics() {
    for profile in CompilerProfile::ALL {
        for declaration in [
            "Q value;",
            "void call(Q);",
            "Q call(void);",
            "void call(Q*);",
            "typedef Q (*Callback)(Q);",
            "struct S{Q q;};struct S call(void);",
            "extern _Atomic(Q) value;",
            "C call(C);",
            "void call(C*);",
        ] {
            let source =
                format!("typedef __typeof__(1.0Q) Q;typedef __typeof__(1.0Qi) C;{declaration}");
            let parsed =
                toucan::parse_source(Path::new("quad.h"), &source, &Config::with_profile(profile))
                    .unwrap();
            let error = parsed
                .bindings(&Default::default())
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("ABI") || error.contains("complex storage"),
                "{profile:?}: {declaration}: {error}"
            );
        }
    }
}

#[test]
#[ignore = "requires rustc; supports TOUCAN_TEST_RUST_TOOLCHAIN"]
fn explicitly_converted_macros_compile_for_rust_1_64() {
    let host = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => toucan::Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => toucan::Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => toucan::Target::X86_64AppleDarwin,
        ("aarch64", "macos") => toucan::Target::Aarch64AppleDarwin,
        ("x86_64", "windows") => toucan::Target::X86_64PcWindowsMsvc,
        _ => panic!("unsupported native generated-code host"),
    };
    let directory = tempfile::tempdir().unwrap();
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.target() == host)
    {
        let mut config = Config::with_profile(profile);
        config.preprocessor.defines.clear();
        let parsed=toucan::parse_source(Path::new("quad.h"),"#define ONE ((double)(1.0Q+0x1p-112Q))\n#define NEGATIVE_ZERO ((double)__real__ __builtin_complex(-0.0Q,2.0Q))\n",&config).unwrap();
        let (bindings, _) = parsed
            .bindings(&BindingOptions {
                rust_target: "1.64".parse().unwrap(),
                ..Default::default()
            })
            .unwrap();
        let source = format!(
            "{bindings}\nfn main(){{assert_eq!(ONE.to_bits(),0x3ff0000000000000);assert_eq!(NEGATIVE_ZERO.to_bits(),0x8000000000000000);}}\n"
        );
        let input = directory.path().join("main.rs");
        let binary = directory.path().join("probe");
        std::fs::write(&input, source).unwrap();
        let mut command = std::process::Command::new("rustc");
        if let Ok(toolchain) = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
            command.arg(format!("+{toolchain}"));
        }
        let output = command
            .args(["--edition=2021"])
            .arg(&input)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{profile:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = std::process::Command::new(binary).output().unwrap();
        assert!(output.status.success(), "{profile:?}: {output:?}");
    }
}

use toucan::{Compiler, CompilerProfile, Config, Target};

#[test]
fn narrow_macros_require_an_explicit_supported_rust_type() {
    for profile in CompilerProfile::ALL {
        let source = "#define HALF 1.5f16\n#define BRAIN ((__bf16)1.5)\n#define HALF_FLOAT ((float)1.5f16)\n#define BRAIN_DOUBLE ((double)(__bf16)-0.0)\n";
        let parsed = toucan::parse_source(
            std::path::Path::new("half.h"),
            source,
            &Config::with_profile(profile),
        )
        .unwrap();
        let (bindings, report) = parsed.bindings(&toucan::BindingOptions::default()).unwrap();
        assert_eq!(report.floating_macros, 2);
        for (name, ty) in [("HALF", "_Float16"), ("BRAIN", "__bf16")] {
            let skip = report
                .skipped_macros
                .iter()
                .find(|m| m.name == name)
                .unwrap();
            assert!(skip.reason.contains(ty), "{}", skip.reason);
        }
        assert!(bindings.contains("pub const HALF_FLOAT: ::core::primitive::f32"));
        assert!(bindings.contains("0x8000000000000000"));
    }
}

#[test]
fn generated_vector_storage_does_not_claim_a_rust_scalar_call_abi() {
    let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    for source in [
        "_Float16 x;",
        "__bf16 *p;",
        "_Float16 call(_Float16);",
        "typedef __bf16 (*Callback)(__bf16);",
        "typedef __bf16 V __attribute__((vector_size(16))); V f(V);",
    ] {
        let parsed = toucan::parse_source(
            std::path::Path::new("api.h"),
            source,
            &Config::with_profile(profile),
        )
        .unwrap();
        let error = parsed
            .bindings(&Default::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("ABI") || error.contains("vectors"),
            "{source}: {error}"
        );
    }
    let header = "typedef __bf16 B __attribute__((vector_size(16))); \
                  typedef _Float16 H __attribute__((vector_size(16))); \
                  void use_vectors(B*,H*);";
    let parsed = toucan::parse_source(
        std::path::Path::new("api.h"),
        header,
        &Config::with_profile(profile),
    )
    .unwrap();
    let (source, _) = parsed.bindings(&Default::default()).unwrap();
    assert!(source.contains("pub bytes: [::core::primitive::u8; 16]"));
    assert!(source.contains("pub fn use_vectors"));
}

#[test]
#[ignore = "requires native Linux GCC/Clang and rustc; supports TOUCAN_TEST_RUST_TOOLCHAIN"]
fn generated_vector_pointers_and_cast_macros_work_with_c() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let target = match std::env::consts::ARCH {
        "x86_64" => Target::X86_64UnknownLinuxGnu,
        "aarch64" => Target::Aarch64UnknownLinuxGnu,
        _ => return,
    };
    let directory = tempfile::tempdir().unwrap();
    let header = r#"
 typedef _Float16 H __attribute__((vector_size(16)));
 typedef __bf16 B __attribute__((vector_size(16)));
 void initialize(H*,B*);
 unsigned first(const void*);
 #define HALF_FLOAT ((float)(_Float16)1.00048828125)
 #define BRAIN_DOUBLE ((double)(__bf16)-0.0)
 "#;
    let implementation = format!(
        "{header}\nvoid initialize(H*h,B*b){{*h=(H){{(_Float16)1.5,(_Float16)-2}};*b=(B){{(__bf16)1.5,(__bf16)-2}};}}\nunsigned first(const void*p){{unsigned short n;__builtin_memcpy(&n,p,2);return n;}}\n"
    );
    std::fs::write(directory.path().join("api.c"), &implementation).unwrap();
    std::fs::write(directory.path().join("main.rs"),r#"
#![allow(non_camel_case_types,non_snake_case,dead_code)]
include!("bindings.rs");
fn main(){unsafe {
 let mut h:H=core::mem::zeroed();let mut b:B=core::mem::zeroed();
 initialize(&mut h,&mut b);
 assert_eq!(&h.bytes[..4],&[0x00,0x3e,0x00,0xc0]);
 assert_eq!(&b.bytes[..4],&[0xc0,0x3f,0x00,0xc0]);
 h.bytes[..2].copy_from_slice(&[0x55,0x35]);b.bytes[..2].copy_from_slice(&[0x77,0x33]);
 assert_eq!(first((&h as *const H).cast()),0x3555);assert_eq!(first((&b as *const B).cast()),0x3377);
 assert_eq!(HALF_FLOAT.to_bits(),0x3f800000);assert_eq!(BRAIN_DOUBLE.to_bits(),0x8000000000000000);
}}
"#).unwrap();
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(target, compiler).unwrap();
        toucan::parse_source(
            std::path::Path::new("api.c"),
            &implementation,
            &Config::with_profile(profile),
        )
        .unwrap();
        let compilation = toucan::parse_source(
            std::path::Path::new("api.h"),
            header,
            &Config::with_profile(profile),
        )
        .unwrap();
        let (bindings, _) = compilation
            .bindings(&toucan::BindingOptions {
                rust_target: "1.64".parse().unwrap(),
                ..Default::default()
            })
            .unwrap();
        std::fs::write(directory.path().join("bindings.rs"), bindings).unwrap();
        let cc = if compiler == Compiler::Gnu {
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into())
        } else {
            "clang".into()
        };
        let output = std::process::Command::new(cc)
            .current_dir(directory.path())
            .args(["-std=gnu11", "-O2", "-c", "api.c", "-o", "api.o"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut rustc = std::process::Command::new("rustc");
        if let Ok(toolchain) = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
            rustc.arg(format!("+{toolchain}"));
        }
        let output = rustc
            .current_dir(directory.path())
            .args([
                "--edition=2021",
                "main.rs",
                "-C",
                "link-arg=api.o",
                "-o",
                "probe",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = std::process::Command::new(directory.path().join("probe"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

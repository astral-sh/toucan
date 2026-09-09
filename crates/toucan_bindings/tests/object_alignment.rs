use toucan_bindings::{Options, RustTarget, generate};
use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};

#[test]
fn selected_objects_with_reduced_alignment_require_a_diagnostic() {
    for profile in CompilerProfile::ALL {
        for source in [
            "extern int bad __attribute__((aligned(1))); int safe(void);",
            "extern int bad[4] __attribute__((aligned(1))); int safe(void);",
            "struct S{int x;}; extern struct S bad __attribute__((aligned(1))); int safe(void);",
        ] {
            let analysis =
                analyze_with_profile(source, profile, &AnalysisOptions::default()).unwrap();
            let error = generate(analysis.unit(), &Options::default()).unwrap_err();
            assert!(
                error.to_string().contains("reduced C alignment"),
                "{profile:?}: {source}: {error}"
            );
            let selected = generate(
                analysis.unit(),
                &Options {
                    allowlist: vec!["safe".into()],
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(selected.source.contains("pub fn safe"));
        }
    }
}

const HEADER: &str = "_Alignas(32) extern int x; extern int y __attribute__((aligned(32))); unsigned long long x_alignment(void); unsigned long long y_alignment(void); int sum(void);";

#[test]
fn ordinary_and_increased_alignment_preserve_the_generated_storage_type() {
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile(HEADER, profile, &AnalysisOptions::default()).unwrap();
        let output = generate(analysis.unit(), &Options::default()).unwrap();
        assert!(
            output
                .source
                .contains("pub static mut x: ::core::ffi::c_int;")
        );
        assert!(
            output
                .source
                .contains("pub static mut y: ::core::ffi::c_int;")
        );
    }
}

#[test]
#[ignore = "requires native GNU GCC, Clang and Rust; run with --include-ignored"]
fn generated_storage_reads_writes_and_c_queries_match_native_compilers() {
    use std::process::Command;
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("api.h"), HEADER).unwrap();
    std::fs::write(
        dir.path().join("native.c"),
        r#"
#include "api.h"
_Alignas(32) int x=3;
int y __attribute__((aligned(32)))=7;
unsigned long long x_alignment(void){return __alignof__(x);}
unsigned long long y_alignment(void){return __alignof__(y);}
int sum(void){return x+y;}
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("consumer.rs"),
        r#"
#![allow(dead_code, non_camel_case_types)]
include!("bindings.rs");
fn main(){unsafe {
    assert_eq!(x_alignment(),32); assert_eq!(y_alignment(),32);
    assert_eq!(core::ptr::addr_of!(x) as usize % 32,0);
    assert_eq!(core::ptr::addr_of!(y) as usize % 32,0);
    let x_value=x; let y_value=y; assert_eq!(x_value,3); assert_eq!(y_value,7); y=9; assert_eq!(sum(),12);
}}
"#,
    )
    .unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(identity.status.success());
    assert!(
        !String::from_utf8_lossy(&identity.stdout)
            .to_ascii_lowercase()
            .contains("clang")
    );
    let rustc = std::env::var("TOUCAN_TEST_RUSTC").unwrap_or_else(|_| "rustc".into());
    for (compiler, kind) in [(gcc.as_str(), Compiler::Gnu), ("clang", Compiler::Clang)] {
        // GNU profiles are defined for Linux. The shared scalar storage contract
        // also permits a native GNU compiler oracle on macOS.
        let profile =
            CompilerProfile::new(target, kind).unwrap_or(CompilerProfile::default_for(target));
        let analysis = analyze_with_profile(HEADER, profile, &AnalysisOptions::default()).unwrap();
        let output = generate(
            analysis.unit(),
            &Options {
                rust_target: RustTarget::RUST_1_64,
                ..Default::default()
            },
        )
        .unwrap();
        std::fs::write(dir.path().join("bindings.rs"), output.source).unwrap();
        for c_opt in ["-O0", "-O2"] {
            let output = Command::new(compiler)
                .current_dir(dir.path())
                .args(["-std=gnu11", c_opt, "-c", "native.c", "-o", "native.o"])
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(true),
                "{compiler} {c_opt}"
            );
            let output = Command::new("ar")
                .current_dir(dir.path())
                .args(["crs", "libnative.a", "native.o"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            for rust_opt in ["0", "3"] {
                let output = Command::new(&rustc)
                    .current_dir(dir.path())
                    .args([
                        "--edition=2021",
                        "-Dwarnings",
                        "-C",
                        &format!("opt-level={rust_opt}"),
                        "consumer.rs",
                        "-L",
                        "native=.",
                        "-l",
                        "static=native",
                        "-o",
                        "consumer",
                    ])
                    .output()
                    .unwrap();
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&output),
                    Ok(true),
                    "{compiler} {c_opt} Rust {rust_opt}"
                );
                let output = Command::new(dir.path().join("consumer")).output().unwrap();
                assert!(
                    output.status.success(),
                    "{compiler} {c_opt} Rust {rust_opt}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

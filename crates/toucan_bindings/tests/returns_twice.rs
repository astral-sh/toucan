use std::process::Command;

use toucan_bindings::{Options, RustTarget, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn selected_returns_twice_functions_require_c_wrappers() {
    for target in Target::ALL {
        for source in [
            "int checkpoint(void) __attribute__((returns_twice)); int wrapper(void);",
            "int checkpoint(void); void f(void) { int checkpoint(void) __attribute__((returns_twice)); } int wrapper(void);",
            "int __attribute__((returns_twice)) checkpoint(void) { return 1; } int wrapper(void);",
        ] {
            let unit = analyze(source, target).unwrap();
            for rust_target in [RustTarget::RUST_1_64, RustTarget::default()] {
                for allowlist in [vec![], vec!["checkpoint".into()]] {
                    let error = generate(
                        &unit,
                        &Options {
                            rust_target,
                            allowlist,
                            ..Options::default()
                        },
                    )
                    .unwrap_err();
                    assert!(
                        error.0.contains("returns_twice function `checkpoint`"),
                        "{error}"
                    );
                }
                for options in [
                    Options {
                        rust_target,
                        allowlist: vec!["wrapper".into()],
                        ..Options::default()
                    },
                    Options {
                        rust_target,
                        blocklist_functions: vec!["checkpoint".into()],
                        ..Options::default()
                    },
                ] {
                    let bindings = generate(&unit, &options).unwrap();
                    assert!(bindings.source.contains("fn wrapper"));
                    assert!(!bindings.source.contains("checkpoint"));
                }
            }
        }
    }
}

#[test]
#[ignore = "requires native GNU GCC, Clang, ar and rustc; run with --include-ignored"]
fn c_wrapper_keeps_repeated_returns_inside_c() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        other => panic!("unsupported native fixture target {other:?}"),
    };
    let unit = analyze(
        "int checkpoint(void) __attribute__((returns_twice)); int wrapper(int jump);",
        target,
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let bindings = generate(
        &unit,
        &Options {
            allowlist: vec!["wrapper".into()],
            rust_target: RustTarget::RUST_1_64,
            ..Options::default()
        },
    )
    .unwrap();
    std::fs::write(directory.path().join("bindings.rs"), bindings.source).unwrap();
    std::fs::write(directory.path().join("probe.rs"),r#"
        include!("bindings.rs");
        fn main() { for _ in 0..100 { unsafe { assert_eq!(wrapper(0),0); assert_eq!(wrapper(1),38); } } }
    "#).unwrap();
    std::fs::write(
        directory.path().join("wrapper.c"),
        r#"
        #include <setjmp.h>
        static void jump_back(jmp_buf state) { longjmp(state, 37); }
        int wrapper(int jump) {
            jmp_buf state;
            volatile int visits = 0;
            int result = setjmp(state);
            if (result == 0 && jump) { visits++; jump_back(state); }
            return result + visits;
        }
    "#,
    )
    .unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let version = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert!(
        !String::from_utf8_lossy(&version.stdout)
            .to_lowercase()
            .contains("clang"),
        "GNU GCC is required"
    );
    for compiler in [gcc.as_str(), "clang"] {
        for optimization in ["-O0", "-O2"] {
            let output = Command::new(compiler)
                .args([
                    "-std=c11",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    optimization,
                    "-c",
                    "wrapper.c",
                    "-o",
                    "wrapper.o",
                ])
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler} {optimization}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new("ar")
                .args(["rcs", "libwrapper.a", "wrapper.o"])
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let rustc = std::env::var("TOUCAN_TEST_RUSTC").unwrap_or_else(|_| "rustc".into());
            let output = Command::new(rustc)
                .args([
                    "--edition=2021",
                    "-O",
                    "probe.rs",
                    "-L",
                    ".",
                    "-l",
                    "static=wrapper",
                    "-o",
                    "probe",
                ])
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                Command::new(directory.path().join("probe"))
                    .status()
                    .unwrap()
                    .success(),
                "{compiler} {optimization}"
            );
        }
    }
}

//! Exercise the native calling convention of a generated `va_list` parameter.

use std::process::Command;

use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

fn run(command: &mut Command) {
    let output = command.output().expect("native compiler must be installed");
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires native cc, ar, and rustc; run with --include-ignored"]
fn generated_va_list_crosses_c_and_rust_call_boundaries() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => panic!("native va_list test is configured only for Linux and macOS"),
    };
    let directory = tempfile::tempdir().unwrap();
    let header = r#"
        typedef __builtin_va_list ProbeVaList;
        typedef double (*ProbeCallback)(unsigned pairs, ProbeVaList args);
        double probe_consume(unsigned pairs, ProbeVaList args);
        double probe_bridge(ProbeCallback callback, unsigned pairs, ...);
    "#;
    let unit = analyze(header, target).unwrap();
    let bindings = generate(&unit, &Options::default()).unwrap();
    std::fs::write(directory.path().join("api.h"), header).unwrap();
    std::fs::write(directory.path().join("bindings.rs"), bindings.source).unwrap();
    std::fs::write(
        directory.path().join("api.c"),
        r#"
        #include "api.h"
        #include <stdarg.h>

        double probe_consume(unsigned pairs, ProbeVaList args) {
            ProbeVaList copy;
            va_copy(copy, args);
            double total = 0;
            for (unsigned i = 0; i < pairs; ++i) {
                total += va_arg(copy, int);
                total += va_arg(copy, double);
            }
            va_end(copy);
            return total;
        }

        double probe_bridge(ProbeCallback callback, unsigned pairs, ...) {
            ProbeVaList args;
            va_start(args, pairs);
            double result = callback(pairs, args);
            va_end(args);
            return result;
        }
    "#,
    )
    .unwrap();
    std::fs::write(
        directory.path().join("main.rs"),
        r#"
        #![allow(non_camel_case_types, dead_code)]
        use std::sync::atomic::{AtomicUsize, Ordering};
        include!("bindings.rs");

        // C adjusts an array parameter to a pointer to its element. The other
        // native targets pass the generated struct or pointer typedef directly.
        #[cfg(target_arch = "x86_64")]
        type VaListArgument = *mut <ProbeVaList as IntoIterator>::Item;
        #[cfg(target_arch = "aarch64")]
        type VaListArgument = ProbeVaList;

        static CALLS: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "C" fn forward(pairs: core::ffi::c_uint, args: VaListArgument) -> f64 {
            CALLS.fetch_add(1, Ordering::Relaxed);
            unsafe { probe_consume(pairs, args) }
        }
        const _: ProbeCallback = Some(forward);

        fn main() {
            unsafe {
                assert_eq!(probe_bridge(Some(forward), 0), 0.0);
                assert_eq!(probe_bridge(Some(forward), 1, -7i32, 0.5f64), -6.5);
                // Twelve pairs exhaust the integer and floating-point argument
                // registers, so the C consumer must also read stack arguments.
                assert_eq!(probe_bridge(Some(forward), 12,
                    1i32, 0.25f64, 2i32, 0.5f64, 3i32, 0.75f64, 4i32, 1.0f64,
                    5i32, 1.25f64, 6i32, 1.5f64, 7i32, 1.75f64, 8i32, 2.0f64,
                    9i32, 2.25f64, 10i32, 2.5f64, 11i32, 2.75f64, 12i32, 3.0f64,
                ), 97.5);
            }
            assert_eq!(CALLS.load(Ordering::Relaxed), 3);
        }
    "#,
    )
    .unwrap();
    run(
        Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
            .current_dir(directory.path())
            .args([
                "-std=c11", "-Wall", "-Wextra", "-Werror", "-O2", "-c", "api.c", "-o", "api.o",
            ]),
    );
    run(Command::new("ar")
        .current_dir(directory.path())
        .args(["rcs", "libprobe.a", "api.o"]));
    run(Command::new("rustc").current_dir(directory.path()).args([
        "--edition=2024",
        "-O",
        "-D",
        "improper_ctypes",
        "-D",
        "improper_ctypes_definitions",
        "main.rs",
        "-L",
        ".",
        "-l",
        "static=probe",
        "-o",
        "probe",
    ]));
    run(&mut Command::new(directory.path().join("probe")));
}

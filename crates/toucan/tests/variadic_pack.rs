#![cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]

use std::path::Path;
use std::process::Command;

use toucan::{BindingOptions, Config, Target};

const HEADER: &str =
    "int invoke_pack(int left, int right); int invoke_empty(void); int format_pack(char *output);";

const SOURCE: &str = r#"
int sum_values(int count, ...) {
    __builtin_va_list arguments;
    __builtin_va_start(arguments, count);
    int result = 0;
    for (int i = 0; i < count; i++) result += __builtin_va_arg(arguments, int);
    __builtin_va_end(arguments);
    return result;
}
static inline __attribute__((always_inline)) int forward(int count, ...) {
    return sum_values(count, __builtin_va_arg_pack());
}
static inline __attribute__((always_inline)) int pack_count(int unused, ...) {
    return __builtin_va_arg_pack_len();
}
static inline __attribute__((always_inline)) int format_values(char *output, ...) {
    return __builtin___sprintf_chk(output, 0, 64, "%d/%.1f", __builtin_va_arg_pack());
}
int invoke_pack(int left, int right) {
    return forward(2, left, right) + 100 * pack_count(0, left, 1.5, right);
}
int invoke_empty(void) { return forward(0) + pack_count(0); }
int format_pack(char *output) { return format_values(output, 17, (float)2.5); }
"#;

#[test]
#[ignore = "requires native GCC, ar, and rustc; run with --include-ignored"]
fn inlined_argument_packs_cross_the_c_rust_boundary() {
    let target = if cfg!(target_arch = "aarch64") {
        Target::Aarch64UnknownLinuxGnu
    } else {
        Target::X86_64UnknownLinuxGnu
    };
    let mut config = Config::new(target);
    config.analysis.retain_code = true;
    let implementation = toucan::parse_source(Path::new("pack.c"), SOURCE, &config).unwrap();
    assert!(implementation.checked().is_some());
    let compilation = toucan::parse_source(Path::new("pack.h"), HEADER, &config).unwrap();
    let (bindings, report) = compilation
        .bindings(&BindingOptions {
            allowlist: vec!["invoke*".into(), "format_pack".into()],
            ..Default::default()
        })
        .unwrap();
    assert!(report.skipped_declarations.is_empty());
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("pack.c"),
        format!("{HEADER}\n{SOURCE}"),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("probe.rs"),
        format!(
            r#"
{bindings}
fn main() {{
    unsafe {{
        assert_eq!(invoke_pack(7, 9), 316);
        assert_eq!(invoke_empty(), 0);
        let mut buffer = [0i8; 64];
        assert_eq!(format_pack(buffer.as_mut_ptr().cast()), 6);
        assert_eq!(std::ffi::CStr::from_ptr(buffer.as_ptr().cast()).to_bytes(), b"17/2.5");
    }}
}}
"#
        ),
    )
    .unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for optimization in ["-O0", "-O2"] {
        for command in [
            vec![
                gcc.as_str(),
                "-std=gnu11",
                optimization,
                "-c",
                "pack.c",
                "-o",
                "pack.o",
            ],
            vec!["ar", "rcs", "libpack.a", "pack.o"],
            vec![
                "rustc",
                "--edition=2024",
                "probe.rs",
                "-L",
                ".",
                "-l",
                "static=pack",
                "-o",
                "probe",
            ],
            vec!["./probe"],
        ] {
            let output = Command::new(command[0])
                .args(&command[1..])
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{optimization}: {command:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

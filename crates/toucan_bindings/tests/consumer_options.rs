use std::process::Command;

use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn size_t_requires_a_matching_unsigned_c_type() {
    for source in ["typedef int size_t;", "typedef unsigned short size_t;"] {
        let unit = analyze(source, Target::X86_64UnknownLinuxGnu).unwrap();
        assert!(
            generate(
                &unit,
                &Options {
                    size_t_is_usize: true,
                    ..Options::default()
                }
            )
            .is_err()
        );
    }
}

#[test]
#[ignore = "requires native C compilers, ar, and rustc; run with --include-ignored"]
fn rust_enum_variants_and_usize_match_native_c_calls() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let source = "typedef unsigned long size_t;\n\
        typedef enum { FIRST = -1, NEXT = 7, ALIAS = 7 } Mode;\n\
        enum Tagged { OFF = 0, ON = 1 };\n\
        Mode echo_mode(Mode); enum Tagged echo_tag(enum Tagged); size_t echo_size(size_t);\n";
    let unit = analyze(
        &format!("{source}\nvoid local(enum Local {{ self = 1, __toucan_self = 2 }} value);"),
        target,
    )
    .unwrap();
    let bindings = generate(
        &unit,
        &Options {
            rustified_enums: true,
            size_t_is_usize: true,
            ..Options::default()
        },
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("bindings.rs"), bindings.source).unwrap();
    std::fs::write(directory.path().join("probe.c"), format!("{source}\nMode echo_mode(Mode x) {{ return x; }}\nenum Tagged echo_tag(enum Tagged x) {{ return x; }}\nsize_t echo_size(size_t x) {{ return x; }}\n")).unwrap();
    std::fs::write(
        directory.path().join("probe.rs"),
        r#"
        #![allow(dead_code, non_camel_case_types)]
        include!("bindings.rs");
        fn main() {
            use Mode::*;
            use Tagged::*;
            assert_eq!(Mode::ALIAS, NEXT);
            unsafe {
                assert_eq!(echo_mode(FIRST), FIRST);
                assert_eq!(echo_mode(Mode::ALIAS), NEXT);
                assert_eq!(echo_tag(ON), ON);
                let length: usize = echo_size(usize::MAX);
                assert_eq!(length, usize::MAX);
            }
        }
    "#,
    )
    .unwrap();
    let mut checked = 0;
    for compiler in ["gcc", "clang"] {
        if Command::new(compiler).arg("--version").output().is_err() {
            continue;
        }
        for (program, arguments) in [
            (
                compiler,
                vec![
                    "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c", "probe.c", "-o",
                    "probe.o",
                ],
            ),
            ("ar", vec!["rcs", "libprobe.a", "probe.o"]),
            (
                "rustc",
                vec![
                    "--edition=2024",
                    "probe.rs",
                    "-L",
                    ".",
                    "-l",
                    "static=probe",
                    "-o",
                    "probe",
                ],
            ),
        ] {
            let output = Command::new(program)
                .args(arguments)
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{program}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert!(
            Command::new(directory.path().join("probe"))
                .status()
                .unwrap()
                .success()
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "a C compiler is required for this differential test"
    );
}

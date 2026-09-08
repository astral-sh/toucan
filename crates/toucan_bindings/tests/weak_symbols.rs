use std::process::Command;
use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn selected_weak_symbols_require_explicit_binding_support() {
    for target in Target::ALL {
        for source in [
            "extern int optional __attribute__((weak)); int ordinary(void);",
            "int optional(void) __attribute__((weak)); int ordinary(void);",
            "int __attribute__((weak)) optional(void) { return 1; } int ordinary(void);",
        ] {
            let unit = analyze(source, target).unwrap();
            for allowlist in [vec![], vec!["optional".into()]] {
                let error = generate(
                    &unit,
                    &Options {
                        allowlist,
                        ..Options::default()
                    },
                )
                .unwrap_err();
                assert!(error.0.contains("weak symbol `optional`"), "{error}");
            }
            let bindings = generate(
                &unit,
                &Options {
                    allowlist: vec!["ordinary".into()],
                    ..Options::default()
                },
            )
            .unwrap();
            assert!(bindings.source.contains("fn ordinary"));
            assert!(!bindings.source.contains("optional"));
        }
    }
}

#[test]
#[ignore = "requires native GNU GCC, Clang, ar and rustc; run with --include-ignored"]
fn c_wrappers_observe_missing_weak_symbols_and_strong_overrides() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let source = r#"
        extern int optional_value __attribute__((weak));
        extern int optional_function(void) __attribute__((weak));
        int __attribute__((weak)) replaceable_value = 7;
        int __attribute__((weak)) replaceable_function(void) { return 11; }
        int wrapper_optional(void) {
            return (&optional_value && optional_function) ? optional_value + optional_function() : -1;
        }
        int wrapper_override(void) { return replaceable_value + replaceable_function(); }
    "#;
    let unit = analyze(source, target).unwrap();
    // Wrapper prototypes are generated independently; function definitions are
    // already represented and checked in `unit` above.
    let header =
        format!("{source}\nint wrapper_decl_optional(void); int wrapper_decl_override(void);");
    let unit_with_header = analyze(&header, target).unwrap();
    let bindings = generate(
        &unit_with_header,
        &Options {
            allowlist: vec!["wrapper_decl_*".into()],
            ..Options::default()
        },
    )
    .unwrap();
    assert!(
        unit.declarations
            .iter()
            .any(|item| item.name == "optional_value"
                && item.symbol_binding == toucan_semantic::SymbolBinding::Weak)
    );
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("bindings.rs"), bindings.source).unwrap();
    std::fs::write(directory.path().join("weak.c"),format!("{source}\nint wrapper_decl_optional(void) {{ return wrapper_optional(); }}\nint wrapper_decl_override(void) {{ return wrapper_override(); }}\n")).unwrap();
    std::fs::write(directory.path().join("strong.c"),"int optional_value=17; int optional_function(void) { return 14; } int replaceable_value=21; int replaceable_function(void) { return 9; }\n").unwrap();
    std::fs::write(
        directory.path().join("probe.rs"),
        r#"
        #![allow(dead_code, non_camel_case_types)]
        include!("bindings.rs");
        fn main() {
            let present=std::env::args().any(|arg|arg=="present");
            unsafe {
                assert_eq!(wrapper_decl_optional(),if present {31} else {-1});
                assert_eq!(wrapper_decl_override(),if present {30} else {18});
            }
        }
    "#,
    )
    .unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc.as_str(), "clang"] {
        let version = Command::new(compiler).arg("--version").output().unwrap();
        assert!(version.status.success());
        if compiler != "clang" {
            assert!(
                !String::from_utf8_lossy(&version.stdout)
                    .to_lowercase()
                    .contains("clang"),
                "GNU GCC is required"
            );
        }
        for input in ["weak", "strong"] {
            let output = Command::new(compiler)
                .args([
                    "-std=c11",
                    "-O2",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-c",
                    &format!("{input}.c"),
                    "-o",
                    &format!("{input}.o"),
                ])
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let output = Command::new("ar")
            .args(["rcs", "libprobe.a", "weak.o"])
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for present in [false, true] {
            let mut command = Command::new("rustc");
            command
                .args([
                    "--edition=2024",
                    "-O",
                    "probe.rs",
                    "-L",
                    ".",
                    "-l",
                    "static=probe",
                    "-o",
                    "probe",
                ])
                .current_dir(directory.path());
            if present {
                command.args(["-C", "link-arg=strong.o"]);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "rustc: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let mut command = Command::new(directory.path().join("probe"));
            if present {
                command.arg("present");
            }
            assert!(command.status().unwrap().success());
        }
    }
}

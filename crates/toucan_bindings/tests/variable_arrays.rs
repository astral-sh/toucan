use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn variable_array_parameters_adjust_without_inventing_an_extent() {
    let unit = analyze(
        "void f(int a[*]); void g(int n, const int a[static restrict n]);",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let bindings = generate(&unit, &Options::default()).unwrap();
    assert!(bindings.source.contains("*mut ::core::ffi::c_int"));
    assert!(bindings.source.contains("*const ::core::ffi::c_int"));
    assert!(!bindings.source.contains("[::core::ffi::c_int; 0]"));

    for source in ["void f(int n, int a[n][n]);", "void f(int (*a)[*]);"] {
        let unit = analyze(source, Target::X86_64UnknownLinuxGnu).unwrap();
        let error = generate(&unit, &Options::default()).unwrap_err();
        assert!(
            error
                .0
                .contains("variable-length arrays have no fixed Rust representation")
        );
    }
}

#[test]
#[ignore = "requires native C compiler and rustc; run with --include-ignored"]
fn calls_c_function_with_variable_array_parameter() {
    use std::process::Command;
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => panic!("native test requires Linux or macOS"),
    };
    let unit = analyze("int sum(int n, const int values[static n]);", target).unwrap();
    let bindings = generate(&unit, &Options::default()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("sum.c"), "int sum(int n, const int values[static n]) { int total = 0; for (int i = 0; i < n; ++i) total += values[i]; return total; }\n").unwrap();
    std::fs::write(directory.path().join("main.rs"), format!("{}\nfn main() {{ let values = [4, 8, 15, 16, 23, 42]; assert_eq!(unsafe {{ sum(values.len() as _, values.as_ptr()) }}, 108); }}\n", bindings.source)).unwrap();
    for (command, arguments) in [
        (
            "cc",
            vec!["-std=c11", "-pedantic-errors", "-c", "sum.c", "-o", "sum.o"],
        ),
        (
            "rustc",
            vec![
                "--edition=2024",
                "-D",
                "improper_ctypes",
                "main.rs",
                "-C",
                "link-arg=sum.o",
                "-o",
                "probe",
            ],
        ),
        ("./probe", vec![]),
    ] {
        let output = Command::new(command)
            .current_dir(directory.path())
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

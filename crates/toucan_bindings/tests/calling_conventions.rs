use toucan_bindings::{Options, RustTarget, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

const HEADER: &str = r#"
    struct Pair { long long integer; double real; };
    typedef struct Pair (__attribute__((ms_abi)) *WinCallback)(int, double, int, double, long long, double, long long, double);
    typedef struct Pair (__attribute__((sysv_abi)) *SysvCallback)(int, double, int, double, long long, double, long long, double);
    typedef int __attribute__((ms_abi)) WinFunction(int);
    typedef int __attribute__((sysv_abi)) SysvFunction(int);
    struct Pair __attribute__((ms_abi)) win_mix(int, double, int, double, long long, double, long long, double);
    struct Pair __attribute__((sysv_abi)) sysv_mix(int, double, int, double, long long, double, long long, double);
    struct Pair __attribute__((ms_abi)) call_win(WinCallback);
    struct Pair __attribute__((sysv_abi)) call_sysv(SysvCallback);
"#;

#[test]
fn emits_function_and_callback_abis_for_x86_64_targets() {
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::X86_64AppleDarwin,
        Target::X86_64PcWindowsMsvc,
    ] {
        let unit = analyze(HEADER, target).unwrap();
        let bindings = generate(&unit, &Options::default()).unwrap();
        let nondefault = if target == Target::X86_64PcWindowsMsvc {
            "sysv64"
        } else {
            "win64"
        };
        assert!(
            bindings
                .source
                .contains(&format!("unsafe extern \"{nondefault}\" {{"))
        );
        assert!(
            bindings
                .source
                .contains(&format!("Option<unsafe extern \"{nondefault}\" fn"))
        );
        assert!(
            bindings
                .source
                .contains(&format!("= unsafe extern \"{nondefault}\" fn"))
        );
        assert!(bindings.source.contains("unsafe extern \"C\" {"));
        let legacy = generate(
            &unit,
            &Options {
                rust_target: RustTarget::RUST_1_64,
                ..Options::default()
            },
        )
        .unwrap();
        assert!(
            legacy
                .source
                .contains(&format!("\nextern \"{nondefault}\" {{"))
        );
        assert!(
            !legacy
                .source
                .contains(&format!("unsafe extern \"{nondefault}\" {{"))
        );
        assert!(
            legacy
                .source
                .contains(&format!("Option<unsafe extern \"{nondefault}\" fn"))
        );
    }
}

#[test]
#[ignore = "requires native x86-64 C compilers and rustc; run with --include-ignored"]
fn x86_64_calls_and_callbacks_preserve_register_stack_and_aggregate_abis() {
    use std::process::Command;
    if std::env::consts::ARCH != "x86_64" {
        return;
    }
    let target = match std::env::consts::OS {
        "linux" => Target::X86_64UnknownLinuxGnu,
        "macos" => Target::X86_64AppleDarwin,
        "windows" => Target::X86_64PcWindowsMsvc,
        _ => panic!("native FFI test requires Linux, macOS, or Windows"),
    };
    let unit = analyze(HEADER, target).unwrap();
    let bindings = generate(&unit, &Options::default()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let implementation = format!(
        r#"
        {HEADER}
        struct Pair __attribute__((ms_abi)) win_mix(int a, double b, int c, double d, long long e, double f, long long g, double h) {{
            struct Pair result = {{ a+c+e+g, b+d+f+h }}; return result;
        }}
        struct Pair __attribute__((sysv_abi)) sysv_mix(int a, double b, int c, double d, long long e, double f, long long g, double h) {{
            struct Pair result = {{ a+c+e+g, b+d+f+h }}; return result;
        }}
        struct Pair __attribute__((ms_abi)) call_win(WinCallback callback) {{ return callback(1, 2.5, 3, 4.5, 5, 6.5, 7, 8.5); }}
        struct Pair __attribute__((sysv_abi)) call_sysv(SysvCallback callback) {{ return callback(1, 2.5, 3, 4.5, 5, 6.5, 7, 8.5); }}
    "#
    );
    std::fs::write(directory.path().join("calls.c"), implementation).unwrap();
    let win_abi = if target == Target::X86_64PcWindowsMsvc {
        "C"
    } else {
        "win64"
    };
    let sysv_abi = if target == Target::X86_64PcWindowsMsvc {
        "sysv64"
    } else {
        "C"
    };
    let rust = format!(
        r#"
        {}
        unsafe extern "{win_abi}" fn win_callback(a:i32,b:f64,c:i32,d:f64,e:i64,f:f64,g:i64,h:f64) -> Pair {{
            Pair {{ integer: a as i64 + c as i64 + e + g + 100, real: b+d+f+h+200.0 }}
        }}
        unsafe extern "{sysv_abi}" fn sysv_callback(a:i32,b:f64,c:i32,d:f64,e:i64,f:f64,g:i64,h:f64) -> Pair {{
            Pair {{ integer: a as i64 + c as i64 + e + g + 300, real: b+d+f+h+400.0 }}
        }}
        fn main() {{ unsafe {{
            let win = win_mix(1,2.5,3,4.5,5,6.5,7,8.5);
            let sysv = sysv_mix(1,2.5,3,4.5,5,6.5,7,8.5);
            assert_eq!((win.integer,win.real), (16,22.0));
            assert_eq!((sysv.integer,sysv.real), (16,22.0));
            let win = call_win(Some(win_callback));
            let sysv = call_sysv(Some(sysv_callback));
            assert_eq!((win.integer,win.real), (116,222.0));
            assert_eq!((sysv.integer,sysv.real), (316,422.0));
        }} }}
    "#,
        bindings.source
    );
    std::fs::write(directory.path().join("main.rs"), rust).unwrap();
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        "clang".into(),
    ] {
        let output = Command::new(&compiler)
            .current_dir(directory.path())
            .args(["-std=gnu11", "-O2", "-c", "calls.c", "-o", "calls.o"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let binary = directory
            .path()
            .join(if cfg!(windows) { "probe.exe" } else { "probe" });
        let output = Command::new("rustc")
            .current_dir(directory.path())
            .args([
                "--edition=2024",
                "-D",
                "improper_ctypes",
                "-O",
                "main.rs",
                "-C",
                "link-arg=calls.o",
                "-o",
            ])
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(&binary).output().unwrap();
        assert!(
            output.status.success(),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[ignore = "requires native x86-64 rustc; run with --include-ignored"]
fn variadic_nondefault_abi_declarations_compile_as_rust() {
    use std::process::Command;
    if std::env::consts::ARCH != "x86_64" {
        return;
    }
    let target = match std::env::consts::OS {
        "linux" => Target::X86_64UnknownLinuxGnu,
        "macos" => Target::X86_64AppleDarwin,
        "windows" => Target::X86_64PcWindowsMsvc,
        _ => panic!("native ABI test requires Linux, macOS, or Windows"),
    };
    let unit = analyze("int __attribute__((ms_abi)) win(int first, ...); int __attribute__((sysv_abi)) sysv(int first, ...);", target).unwrap();
    let bindings = generate(&unit, &Options::default()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("bindings.rs");
    std::fs::write(&input, bindings.source).unwrap();
    let output = Command::new("rustc")
        .current_dir(directory.path())
        .args([
            "--edition=2024",
            "--crate-type=lib",
            "--emit=metadata",
            "-D",
            "improper_ctypes",
        ])
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unsupported_conventions_in_public_ir_fail_generation() {
    let mut unit = analyze("void f(void);", Target::Aarch64UnknownLinuxGnu).unwrap();
    let toucan_semantic::TypeKind::Function(function) = &mut unit.declarations[0].ty.kind else {
        panic!("function");
    };
    function.calling_convention = toucan_semantic::CallingConvention::Win64;
    let error = generate(&unit, &Options::default()).unwrap_err();
    assert!(error.0.contains("unsupported on this target"));
}

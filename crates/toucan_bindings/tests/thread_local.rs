use std::process::{Command, Output};

use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn direct_tls_bindings_require_accessor_functions() {
    for target in Target::ALL {
        for declaration in [
            "extern _Thread_local int counter;",
            "_Thread_local const int counter=7;",
            "extern __thread int counter;",
            "extern _Thread_local struct Incomplete counter;",
        ] {
            let unit =
                analyze(&format!("{declaration} int *access_counter(void);"), target).unwrap();
            for allowlist in [vec![], vec!["counter".into()]] {
                let error = generate(
                    &unit,
                    &Options {
                        allowlist,
                        ..Options::default()
                    },
                )
                .unwrap_err();
                assert!(error.0.contains("thread-local object `counter`"), "{error}");
                assert!(error.0.contains("C accessor functions"), "{error}");
            }
            let source = generate(
                &unit,
                &Options {
                    allowlist: vec!["access_counter".into()],
                    ..Options::default()
                },
            )
            .unwrap()
            .source;
            assert!(source.contains("fn access_counter"));
            assert!(!source.contains("static counter"));
        }
    }
}

const HEADER: &str = r#"
extern _Thread_local int counter;
int *api_address(void);
int *api_local_address(void);
const char *api_text(void);
void api_visit(void (*callback)(int *, int *, int), int amount);
"#;
const IMPLEMENTATION: &str = r#"
_Thread_local int counter=7;
static _Thread_local const char *text="thread";
int *api_address(void) { return &counter; }
int *api_local_address(void) { static _Thread_local int local=11; return &local; }
const char *api_text(void) { return text; }
void api_visit(void (*callback)(int *, int *, int), int amount) {
    callback(&counter, api_local_address(), amount);
}
"#;
fn run(command: &mut Command) -> Output {
    let output = command.output().expect("test compiler must be installed");
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
#[ignore = "requires native GNU GCC, Clang and Rust; run with --include-ignored"]
fn rust_threads_access_distinct_c_objects_and_callback_addresses() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let directory = tempfile::tempdir().unwrap();
    // Analyze the definitions too; the binding boundary selects their prototypes.
    analyze(&format!("{HEADER}{IMPLEMENTATION}"), target).unwrap();
    let unit = analyze(HEADER, target).unwrap();
    let bindings = generate(
        &unit,
        &Options {
            allowlist: vec!["api_*".into()],
            rust_target: "1.64".parse().unwrap(),
            ..Options::default()
        },
    )
    .unwrap();
    std::fs::write(directory.path().join("bindings.rs"), bindings.source).unwrap();
    std::fs::write(
        directory.path().join("api.c"),
        format!("{HEADER}{IMPLEMENTATION}"),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("main.rs"),
        r#"
#![allow(dead_code, non_camel_case_types)]
include!("bindings.rs");
use std::sync::{Arc, Barrier, mpsc};
unsafe extern "C" fn update(first: *mut i32, second: *mut i32, amount: i32) {
    assert_eq!(first, api_address());
    assert_eq!(second, api_local_address());
    *first += amount;
    *second += amount;
}
fn main() { unsafe {
    let first=api_address(); let second=api_local_address();
    assert_eq!((*first,*second),(7,11));
    *first=101; *second=202;
    let gate=Arc::new(Barrier::new(5));
    let (send, receive)=mpsc::channel();
    let mut threads=Vec::new();
    for index in 0..4 {
        let gate=gate.clone();let send=send.clone();
        threads.push(std::thread::spawn(move || {
            let first=api_address(); let second=api_local_address();
            assert_eq!((*first,*second),(7,11));
            assert_eq!(std::ffi::CStr::from_ptr(api_text()).to_bytes(),b"thread");
            *first=100+index; *second=200+index;
            api_visit(Some(update),index+1);
            assert_eq!((*first,*second),(101+2*index,201+2*index));
            send.send((first as usize,second as usize)).unwrap();
            gate.wait();
            assert_eq!(api_address(),first);assert_eq!(api_local_address(),second);
            assert_eq!((*first,*second),(101+2*index,201+2*index));
            gate.wait();
        }));
    }
    let mut addresses=vec![first as usize,second as usize];
    for _ in 0..4 { let (a,b)=receive.recv().unwrap();addresses.push(a);addresses.push(b); }
    // All worker threads remain alive until both barriers complete. Compare the
    // addresses without dereferencing an object owned by another thread.
    addresses.sort_unstable();addresses.dedup();assert_eq!(addresses.len(),10);
    assert_eq!((*first,*second),(101,202));
    gate.wait();gate.wait();
    for thread in threads { thread.join().unwrap(); }
    assert_eq!((*first,*second),(101,202));
}}
"#,
    )
    .unwrap();
    for (name, variable) in [("gcc", "TOUCAN_GCC"), ("clang", "TOUCAN_CLANG")] {
        let compiler = std::env::var(variable).unwrap_or_else(|_| name.into());
        let version = run(Command::new(&compiler).arg("--version"));
        assert_eq!(
            String::from_utf8_lossy(&version.stdout)
                .to_ascii_lowercase()
                .contains("clang"),
            name == "clang"
        );
        for optimization in ["-O0", "-O2"] {
            run(Command::new(&compiler)
                .args([
                    "-std=c11",
                    "-pedantic-errors",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    optimization,
                    "-c",
                    "api.c",
                    "-o",
                    "api.o",
                ])
                .current_dir(directory.path()));
            let mut rustc = Command::new("rustc");
            if let Ok(toolchain) = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
                rustc.arg(format!("+{toolchain}"));
            }
            run(rustc
                .args([
                    "--edition=2021",
                    "-O",
                    "main.rs",
                    "-C",
                    "link-arg=api.o",
                    "-o",
                    "probe",
                ])
                .current_dir(directory.path()));
            run(&mut Command::new(directory.path().join("probe")));
        }
    }
}

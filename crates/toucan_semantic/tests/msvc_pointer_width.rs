use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::analyze;
use toucan_target::Target;

const ARM64_POINTERS: &str = r#"
    typedef void * __ptr64 HANDLE64;
    typedef void * __ptr32 P32;
    _Static_assert(sizeof(HANDLE64) == 8 && _Alignof(HANDLE64) == 8, "__ptr64 layout");
    _Static_assert(sizeof(P32) == 8 && _Alignof(P32) == 8, "ARM64 __ptr32 layout");
    _Static_assert(__builtin_types_compatible_p(HANDLE64, void *), "__ptr64 identity");
    _Static_assert(!__builtin_types_compatible_p(P32, void *), "__ptr32 identity");
    _Static_assert(!__builtin_types_compatible_p(P32, HANDLE64), "widths remain distinct");
    typedef void (*Callback32)(void * __ptr32);
    typedef void (*Callback64)(void *);
    _Static_assert(!__builtin_types_compatible_p(Callback32, Callback64), "callback argument identity");
    _Static_assert(__builtin_types_compatible_p(const P32, P32), "top-level const is ignored");
    void *from32(void * __ptr32 value) { return (void *)value; }
    void * __ptr32 to32(void *value) { return (void * __ptr32)value; }
"#;

#[test]
fn windows_arm64_pointer_widths_keep_layout_and_type_identity() {
    let unit = analyze(ARM64_POINTERS, Target::Aarch64PcWindowsMsvc).unwrap();
    assert!(unit.typedefs["P32"].qualifiers.is_msvc_ptr32);
    assert!(!unit.typedefs["HANDLE64"].qualifiers.is_msvc_ptr32);
    assert_eq!(unit.layout(&unit.typedefs["P32"]).unwrap().size_bytes(), 8);
    assert_eq!(
        unit.layout(&unit.typedefs["HANDLE64"])
            .unwrap()
            .size_bytes(),
        8
    );
}

#[test]
fn x64_does_not_report_eight_byte_layout_for_four_byte_pointers() {
    let error = analyze("typedef void * __ptr32 P32;", Target::X86_64PcWindowsMsvc)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("__ptr32 pointer ABI is unsupported"),
        "{error}"
    );
}

#[test]
#[ignore = "requires Clang with the Windows ARM64 and x64 cross targets"]
fn clang_windows_pointer_width_oracle() {
    for (target, source) in [
        ("aarch64-pc-windows-msvc", ARM64_POINTERS),
        (
            "x86_64-pc-windows-msvc",
            "typedef void * __ptr32 P32; _Static_assert(sizeof(P32) == 4 && _Alignof(P32) == 4, \"x64 __ptr32 ABI\");",
        ),
    ] {
        let mut process = Command::new("clang")
            .args([
                "-target",
                target,
                "-std=c11",
                "-Werror",
                "-fsyntax-only",
                "-x",
                "c",
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Clang Windows target is required");
        process
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let result = process.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

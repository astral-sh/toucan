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
    assert!(unit.typedefs["P32"].qualifiers.is_msvc_ptr32());
    assert!(!unit.typedefs["HANDLE64"].qualifiers.is_msvc_ptr32());
    assert_eq!(unit.layout(&unit.typedefs["P32"]).unwrap().size_bytes(), 8);
    assert_eq!(
        unit.layout(&unit.typedefs["HANDLE64"])
            .unwrap()
            .size_bytes(),
        8
    );
}

const X64_POINTERS: &str = r#"
    typedef void * __ptr32 P32;
    typedef P32 Alias;
    typedef void * __ptr64 P64;
    _Static_assert(sizeof(Alias) == 4 && _Alignof(Alias) == 4, "x64 __ptr32 layout");
    _Static_assert(sizeof(P64) == 8 && _Alignof(P64) == 8, "x64 native pointer");
    _Static_assert(!__builtin_types_compatible_p(P32, P64), "pointer width identity");
    _Static_assert(__builtin_types_compatible_p(const Alias, P32), "top-level const");
    struct Holder { char tag; Alias pointer; char tail; };
    _Static_assert(sizeof(struct Holder) == 12 && _Alignof(struct Holder) == 4, "record layout");
    _Static_assert(__builtin_offsetof(struct Holder, pointer) == 4, "field offset");
    struct Nested { Alias pointers[3]; P64 wide; };
    _Static_assert(sizeof(struct Nested) == 24, "nested pointer array");
    _Static_assert(__builtin_offsetof(struct Nested, wide) == 16, "native field offset");
    P32 narrow(P64 value) { return (P32)value; }
    P64 widen(P32 value) { return (P64)value; }
    P32 implicit_narrow(P64 value) { return value; }
    P64 implicit_widen(P32 value) { return value; }
    int * __ptr32 narrow_pointer;
    int *native_pointer;
    _Static_assert(sizeof(1 ? narrow_pointer : narrow_pointer) == 4, "conditional narrow width");
    _Static_assert(sizeof(1 ? narrow_pointer : native_pointer) == 8, "conditional mixed width");
    _Static_assert(sizeof(1 ? native_pointer : narrow_pointer) == 8, "conditional reversed width");
    _Static_assert(sizeof(1 ? narrow_pointer : 0) == 4, "conditional null width");
    _Static_assert(sizeof(1 ? narrow_pointer : (void *)0) == 4, "conditional void null width");
    _Static_assert(sizeof(narrow_pointer + 1) == 4, "arithmetic width");
    _Static_assert(sizeof((native_pointer, narrow_pointer)) == 4, "comma width");
"#;

#[test]
fn windows_x64_pointer_widths_keep_layout_and_type_identity() {
    analyze(X64_POINTERS, Target::X86_64PcWindowsMsvc).unwrap();
}

#[test]
#[ignore = "requires Clang with the Windows ARM64 and x64 cross targets"]
fn clang_windows_pointer_width_oracle() {
    for (target, source) in [
        ("aarch64-pc-windows-msvc", ARM64_POINTERS),
        ("x86_64-pc-windows-msvc", X64_POINTERS),
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

use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn x64_ptr32_projection_rejects_direct_and_transitive_uses() {
    for source in [
        "typedef void * __ptr32 Selected;",
        "typedef void * __ptr32 P32; typedef P32 Selected;",
        "typedef void * __ptr32 P32; typedef P32 Selected[2];",
        "struct Selected { void * __ptr32 value; };",
        "struct Inner { void * __ptr32 value; }; struct Selected { struct Inner inner; };",
        "extern void * __ptr32 Selected;",
        "void * __ptr32 Selected(void);",
        "void Selected(void * __ptr32);",
        "void Selected(void * __ptr32 *);",
        "typedef void (*Selected)(void * __ptr32);",
        "typedef void (* __ptr32 Selected)(void);",
    ] {
        let unit = analyze(source, Target::X86_64PcWindowsMsvc).unwrap();
        let error = generate(
            &unit,
            &Options {
                allowlist: vec!["Selected".into()],
                ..Options::default()
            },
        )
        .unwrap_err();
        assert!(error.0.contains("x64 __ptr32"), "{source}: {error}");
    }
}

#[test]
fn x64_ptr32_projection_checks_caller_owned_types() {
    for source in [
        "typedef void * __ptr32 External; void Selected(External);",
        "typedef void * __ptr32 P32; typedef P32 External; void Selected(External *);",
        "struct External { void * __ptr32 pointer; }; void Selected(struct External);",
        "struct External { void * __ptr32 pointer; }; void Selected(struct External *);",
        "typedef void (*External)(void * __ptr32); void Selected(External);",
    ] {
        let unit = analyze(source, Target::X86_64PcWindowsMsvc).unwrap();
        let error = generate(
            &unit,
            &Options {
                allowlist: vec!["Selected".into()],
                blocklist_types: vec!["External".into()],
                ..Options::default()
            },
        )
        .unwrap_err();
        assert!(error.0.contains("x64 __ptr32"), "{source}: {error}");
    }
}

#[test]
fn unselected_x64_ptr32_does_not_block_native_pointer_bindings() {
    let unit = analyze(
        "typedef void * __ptr32 P32; typedef void * __ptr64 Selected;",
        Target::X86_64PcWindowsMsvc,
    )
    .unwrap();
    let bindings = generate(
        &unit,
        &Options {
            allowlist: vec!["Selected".into()],
            ..Options::default()
        },
    )
    .unwrap();
    assert!(
        bindings
            .source
            .contains("pub type Selected = *mut ::core::ffi::c_void;")
    );
    assert!(!bindings.source.contains("pub type P32"));
}

#[test]
fn arm64_ptr32_uses_native_eight_byte_rust_pointers() {
    let unit = analyze(
        "typedef void * __ptr32 P32; void accept(P32);",
        Target::Aarch64PcWindowsMsvc,
    )
    .unwrap();
    let bindings = generate(&unit, &Options::default()).unwrap();
    assert!(
        bindings
            .source
            .contains("pub type P32 = *mut ::core::ffi::c_void;")
    );
}

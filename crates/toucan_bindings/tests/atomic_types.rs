use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn selected_atomic_storage_requires_an_explicit_rust_representation() {
    for source in [
        "typedef _Atomic(int) Atomic;",
        "_Atomic(int) value;",
        "void f(_Atomic(int)*);",
        "_Atomic(float) f(_Atomic(float));",
        "struct S{_Atomic(int) value;};",
        "struct V{float x,y;}; _Atomic(struct V) f(_Atomic(struct V));",
        "typedef void(*Callback)(_Atomic(int));",
    ] {
        for target in Target::ALL {
            let unit = analyze(source, target).unwrap();
            let error = generate(&unit, &Options::default()).unwrap_err();
            assert!(error.to_string().contains("atomic"), "{source}: {error}");
        }
    }
}

use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn native_neon_storage_remains_pointer_only_at_the_rust_boundary() {
    let target = Target::Aarch64UnknownLinuxGnu;
    let unit = analyze("typedef __Float32x4_t V; void fill(V*);", target).unwrap();
    let output = generate(&unit, &Options::default()).unwrap().source;
    assert!(output.contains("#[repr(C, align(16))]"));
    assert!(output.contains("pub type __Float32x4_t = __toucan_vector_16_align_16;"));
    let unit = analyze("typedef __Float32x4_t V; V call(V);", target).unwrap();
    assert!(
        generate(&unit, &Options::default())
            .unwrap_err()
            .to_string()
            .contains("cannot cross an FFI call by value")
    );
}

#[test]
fn sizeless_storage_and_vector_pcs_need_c_wrappers() {
    for target in [Target::Aarch64UnknownLinuxGnu, Target::Aarch64AppleDarwin] {
        for source in [
            "typedef __SVFloat32_t V;",
            "extern __SVBool_t *pointer;",
            "void callback(__SVFloat64_t*);",
            "typedef __SVFloat32_t (*Callback)(__SVBool_t);",
        ] {
            let unit = analyze(source, target).unwrap();
            let error = generate(&unit, &Options::default()).unwrap_err();
            assert!(
                error.to_string().contains("sizeless SVE"),
                "{target}: {source}: {error}"
            );
        }
        for source in [
            "void __attribute__((aarch64_vector_pcs)) f(void);",
            "typedef void (__attribute__((aarch64_vector_pcs)) *Callback)(void);",
        ] {
            let unit = analyze(source, target).unwrap();
            assert!(
                generate(&unit, &Options::default())
                    .unwrap_err()
                    .to_string()
                    .contains("no stable Rust extern ABI")
            );
        }
        // Unsupported declarations can remain in an analyzed translation unit
        // while an unrelated, explicitly selected C wrapper gets bindings.
        let unit = analyze(
            "typedef __SVFloat32_t V; V raw(V); int wrapper(int);",
            target,
        )
        .unwrap();
        let options = Options {
            allowlist: vec!["wrapper".into()],
            ..Options::default()
        };
        assert!(
            generate(&unit, &options)
                .unwrap()
                .source
                .contains("pub fn wrapper")
        );
    }
}

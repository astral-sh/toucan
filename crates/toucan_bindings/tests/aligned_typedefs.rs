use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn selected_changed_alignment_fails_before_emitting_an_invalid_rust_alias() {
    for target in Target::ALL {
        for source in [
            "typedef int A __attribute__((aligned(1))); A read(A *);",
            "typedef int A __attribute__((aligned(16))); struct S { char c; A field; };",
            "typedef int A[3] __attribute__((aligned(16)));",
            "typedef struct S { int x; } A __attribute__((aligned(1)));",
        ] {
            let unit = analyze(source, target).unwrap();
            let error = generate(&unit, &Options::default()).unwrap_err();
            assert!(error.0.contains("typedef alignment"), "{target:?}: {error}");
        }
    }
}

#[test]
fn redundant_linux_int128_aliases_and_unselected_changed_aliases_are_usable() {
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
        Target::X86_64AppleDarwin,
        Target::Aarch64AppleDarwin,
    ] {
        let unit=analyze("typedef __signed__ __int128 __s128 __attribute__((aligned(16))); typedef unsigned __int128 __u128 __attribute__((aligned(16))); __s128 identity(__s128);",target).unwrap();
        let bindings = generate(&unit, &Options::default()).unwrap();
        assert!(
            bindings
                .source
                .contains("pub type __s128 = ::core::primitive::i128;")
        );
        assert!(
            bindings
                .source
                .contains("pub type __u128 = ::core::primitive::u128;")
        );
    }
    let unit = analyze(
        "typedef int Hidden __attribute__((aligned(1))); int public_function(int);",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let bindings = generate(
        &unit,
        &Options {
            allowlist: vec!["public_function".into()],
            ..Options::default()
        },
    )
    .unwrap();
    assert!(bindings.source.contains("public_function"));
    assert!(!bindings.source.contains("pub type Hidden"));
}

#[test]
fn normalized_size_t_checks_the_discarded_alias_layout() {
    for target in Target::ALL {
        let integer = if target == Target::X86_64PcWindowsMsvc {
            "unsigned long long"
        } else {
            "unsigned long"
        };
        for alignment in [1, 8, 16] {
            let source = format!(
                "typedef {integer} internal __attribute__((aligned({alignment}))); \
                 typedef internal size_t; size_t length(size_t);"
            );
            let unit = analyze(&source, target).unwrap();
            let result = generate(
                &unit,
                &Options {
                    allowlist: vec!["length".into()],
                    size_t_is_usize: true,
                    ..Options::default()
                },
            );
            // MSVC explicit alignment also changes the required alignment under packing.
            if alignment == 8 && target != Target::X86_64PcWindowsMsvc {
                let bindings = result.unwrap();
                assert!(bindings.source.contains("arg0: ::core::primitive::usize"));
                assert!(!bindings.source.contains("pub type internal"));
            } else {
                let error = result.unwrap_err();
                assert!(
                    error.0.contains("usize-compatible alignment"),
                    "{target:?}: {error}"
                );
            }
        }
    }
}

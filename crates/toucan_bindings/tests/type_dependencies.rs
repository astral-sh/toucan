use toucan_bindings::{BindingSelection, Options, TypeDependencies, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn opt_in_dependencies_keep_abi_types_and_terminate_shared_cycles() {
    let unit = analyze("typedef int A[4]; typedef int B[8]; typedef void Callback(int*); struct Holder {Callback *callback;}; extern struct Holder *chosen;", Target::X86_64UnknownLinuxGnu).unwrap();
    let mut options = Options {
        selection: Some(Box::new(BindingSelection {
            declarations: [3].into(),
            ..Default::default()
        })),
        ..Default::default()
    };
    let plain = generate(&unit, &options).unwrap().source;
    assert!(!plain.contains("pub type A"));
    options.type_dependencies = Some(Box::new(TypeDependencies {
        records: [(0, ["A".into()].into())].into(),
        typedefs: [
            ("Callback".into(), ["B".into()].into()),
            ("A".into(), ["B".into()].into()),
            ("B".into(), ["A".into()].into()),
        ]
        .into(),
    }));
    let retained = generate(&unit, &options).unwrap().source;
    assert!(retained.contains("pub type A = [::core::ffi::c_int; 4]"));
    assert!(retained.contains("pub type B = [::core::ffi::c_int; 8]"));
    let without_added = retained
        .lines()
        .filter(|line| !line.starts_with("pub type A =") && !line.starts_with("pub type B ="))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(without_added, plain.trim_end());
}

#[test]
fn caller_supplied_dependencies_validate_owner_and_alias_identities() {
    let unit = analyze(
        "typedef int A[4]; struct Holder { int value; };",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    for dependencies in [
        TypeDependencies {
            records: [(usize::MAX, ["A".into()].into())].into(),
            ..Default::default()
        },
        TypeDependencies {
            typedefs: [("Missing".into(), ["A".into()].into())].into(),
            ..Default::default()
        },
        TypeDependencies {
            records: [(0, ["Missing".into()].into())].into(),
            ..Default::default()
        },
    ] {
        let options = Options {
            type_dependencies: Some(Box::new(dependencies)),
            ..Default::default()
        };
        assert!(
            generate(&unit, &options)
                .unwrap_err()
                .to_string()
                .contains("dependency")
        );
    }
}

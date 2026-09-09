use toucan_bindings::{BindingSelection, Options, RustTarget, generate};
use toucan_semantic::{DeclarationKind, analyze};
use toucan_target::Target;

fn options(roots: BindingSelection) -> Options {
    Options {
        selection: Some(Box::new(roots)),
        ..Default::default()
    }
}

#[test]
fn explicit_roots_keep_tag_and_ordinary_namespaces_separate() {
    let unit = analyze(
        "struct Same { int value; }; extern int Same; enum E {VALUE=4};",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let record = generate(
        &unit,
        &options(BindingSelection {
            records: [unit
                .records
                .iter()
                .position(|record| record.name.as_deref() == Some("Same"))
                .unwrap()]
            .into(),
            ..Default::default()
        }),
    )
    .unwrap()
    .source;
    assert!(record.contains("pub struct Same"));
    assert!(!record.contains("pub static mut Same"));
    assert!(!record.contains("pub const VALUE"));
    let object = generate(
        &unit,
        &options(BindingSelection {
            declarations: [0].into(),
            ..Default::default()
        }),
    )
    .unwrap()
    .source;
    assert!(object.contains("pub static mut Same"));
    assert!(!object.contains("pub struct Same"));
    let constant = generate(
        &unit,
        &options(BindingSelection {
            constants: ["VALUE".into()].into(),
            ..Default::default()
        }),
    )
    .unwrap()
    .source;
    assert!(constant.contains("pub const VALUE"));
    assert!(!constant.contains("pub struct Same"));
}

#[test]
fn explicit_roots_compose_with_patterns_dependencies_and_size_t() {
    let unit=analyze("typedef unsigned long size_t; struct S {int x;}; void chosen(struct S*,size_t); void extra(void);",Target::X86_64UnknownLinuxGnu).unwrap();
    let id = unit
        .declarations
        .iter()
        .position(|d| d.name == "chosen")
        .unwrap();
    let mut config = options(BindingSelection {
        declarations: [id].into(),
        ..Default::default()
    });
    config.allowlist.push("extra".into());
    config.size_t_is_usize = true;
    let source = generate(&unit, &config).unwrap().source;
    assert!(source.contains("pub struct S"));
    assert!(source.contains("pub fn chosen("));
    assert!(source.contains("pub fn extra("));
    assert!(!source.contains("pub type size_t"));
    let alias = unit
        .declarations
        .iter()
        .position(|d| d.kind == DeclarationKind::Typedef)
        .unwrap();
    config
        .selection
        .as_mut()
        .unwrap()
        .declarations
        .insert(alias);
    assert!(
        generate(&unit, &config)
            .unwrap()
            .source
            .contains("pub type size_t = ::core::primitive::usize")
    );
    let empty = generate(&unit, &options(BindingSelection::default()))
        .unwrap()
        .source;
    assert!(!empty.contains("pub fn "));
    assert!(!empty.contains("pub struct "));
}

#[test]
fn explicit_roots_validate_indices_and_retain_abi_guards() {
    let unit = analyze(
        "struct S{int x;}; void wide(__int128);",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    for roots in [
        BindingSelection {
            declarations: [99].into(),
            ..Default::default()
        },
        BindingSelection {
            records: [99].into(),
            ..Default::default()
        },
        BindingSelection {
            enums: [99].into(),
            ..Default::default()
        },
        BindingSelection {
            typedefs: ["Missing".into()].into(),
            ..Default::default()
        },
        BindingSelection {
            constants: ["Missing".into()].into(),
            ..Default::default()
        },
    ] {
        assert!(generate(&unit, &options(roots)).is_err());
    }
    let mut config = options(BindingSelection {
        declarations: [0].into(),
        ..Default::default()
    });
    config.rust_target = RustTarget::RUST_1_64;
    assert!(
        generate(&unit, &config)
            .unwrap_err()
            .to_string()
            .contains("1.78")
    );
}

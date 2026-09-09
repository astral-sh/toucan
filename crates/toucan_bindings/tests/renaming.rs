use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn rust_symbol_renames_preserve_original_and_explicit_link_names() {
    let unit=analyze("extern int prefix_value; int prefix_call(int); int original(void) __asm__(\"native_symbol\");",Target::X86_64UnknownLinuxGnu).unwrap();
    let config = Options {
        generated_names: [
            ("prefix_value".into(), "value".into()),
            ("prefix_call".into(), "call".into()),
            ("original".into(), "renamed".into()),
        ]
        .into(),
        ..Default::default()
    };
    let output = generate(&unit, &config).unwrap().source;
    assert!(output.contains("#[link_name = \"prefix_value\"]\n    pub static mut value:"));
    assert!(output.contains("#[link_name = \"prefix_call\"]\n    pub fn call("));
    assert!(output.contains("#[link_name = \"native_symbol\"]\n    pub fn renamed("));
}

#[test]
fn prefix_link_names_uses_original_c_names_for_functions_and_objects() {
    let unit = analyze(
        "int c_function(void) __asm__(\"native_call\"); extern const int c_value;",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let options = Options {
        generated_names: [("c_function".into(), "rust_function".into())].into(),
        link_name_prefix: Some("aws_lc_0_44_0_".into()),
        ..Default::default()
    };
    let output = generate(&unit, &options).unwrap().source;
    assert!(output.contains("#[link_name = \"aws_lc_0_44_0_c_function\"]"));
    assert!(output.contains("pub fn rust_function("));
    assert!(output.contains("#[link_name = \"aws_lc_0_44_0_c_value\"]"));
    assert!(output.contains("pub static c_value:"));
}

#[test]
fn generated_names_reject_collisions_and_invalid_requests() {
    let unit = analyze(
        "int left(void); int right(void); enum E{VALUE=1}; typedef int Alias;",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    for renames in [
        [("left", "right")],
        [("left", "VALUE")],
        [("left", "")],
        [("left", "bad-name")],
        [("Alias", "other")],
        [("missing", "other")],
    ] {
        let options = Options {
            generated_names: renames
                .into_iter()
                .map(|(a, b)| (a.into(), b.into()))
                .collect(),
            ..Default::default()
        };
        assert!(generate(&unit, &options).is_err());
    }
    let config = Options {
        allowlist: vec!["left".into()],
        generated_names: [("left".into(), "right".into())].into(),
        ..Default::default()
    };
    assert!(
        generate(&unit, &config)
            .unwrap()
            .source
            .contains("pub fn right(")
    );
}

#[test]
fn generated_names_share_reserved_identifier_and_helper_collision_rules() {
    let unit = analyze(
        "int prefix_self(void); int __toucan_self(void); struct S{int x;}; int helper(void);",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let config = Options {
        generated_names: [
            ("prefix_self".into(), "self".into()),
            ("helper".into(), "__toucan_layout_S".into()),
        ]
        .into(),
        ..Default::default()
    };
    let output = generate(&unit, &config).unwrap().source;
    assert!(output.contains("pub fn __toucan_self_("));
    assert!(output.contains("pub fn __toucan_self("));
    assert!(output.contains("#[link_name = \"helper\"]"));
}

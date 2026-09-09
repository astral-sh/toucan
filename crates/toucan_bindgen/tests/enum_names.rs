use toucan_bindgen::{Builder, Formatter};

#[test]
fn enum_prefix_setting_changes_names_and_preserves_scoped_variants() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("enum.h");
    std::fs::write(
        &header,
        "enum Named { A=1 }; typedef enum { B=2 } Alias; enum { FREE=3 };\n",
    )
    .unwrap();
    let builder = Builder::default()
        .header(header.to_str().unwrap())
        .clang_arg("--target=x86_64-unknown-linux-gnu")
        .formatter(Formatter::None);
    let default = builder.clone().generate().unwrap().to_string();
    assert!(default.contains("pub const Named_A:"));
    assert!(default.contains("pub const Alias_B:"));
    assert!(default.contains("pub const FREE:"));
    let disabled = builder
        .clone()
        .prepend_enum_name(false)
        .generate()
        .unwrap()
        .to_string();
    assert!(disabled.contains("pub const A:"));
    assert!(!disabled.contains("pub const Named_A:"));
    assert_eq!(
        builder
            .clone()
            .prepend_enum_name(false)
            .prepend_enum_name(true)
            .generate()
            .unwrap()
            .to_string(),
        default
    );
    let scoped = builder.rustified_enum(".*").generate().unwrap().to_string();
    assert!(!scoped.contains("pub const Named_A:"));
    assert!(!scoped.contains("pub const A:"));
    assert!(scoped.contains("pub const FREE: _bindgen_ty_1 = _bindgen_ty_1::FREE;"));
}

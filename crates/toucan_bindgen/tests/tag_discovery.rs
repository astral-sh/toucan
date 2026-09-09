use toucan_bindgen::{Builder, Formatter};

#[test]
fn file_selection_follows_discovery_and_preserves_definition_identity() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("root.h");
    std::fs::write(
        directory.path().join("first.h"),
        "enum Outer { COUNT=sizeof(enum Inner { VALUE=7 }) };\n",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("second.h"),
        "struct Owner { enum Inner field; };\n",
    )
    .unwrap();
    std::fs::write(&root, "#include \"first.h\"\n#include \"second.h\"\n").unwrap();
    for (pattern, outer, inner) in [
        (r".*[/\\]first\.h", true, false),
        (r".*[/\\]second\.h", false, true),
    ] {
        let source = Builder::default()
            .header(root.to_str().unwrap())
            .clang_arg("--target=x86_64-unknown-linux-gnu")
            .allowlist_file(pattern)
            .formatter(Formatter::None)
            .generate()
            .unwrap()
            .to_string();
        assert_eq!(source.contains("pub const Outer_COUNT:"), outer, "{source}");
        assert_eq!(
            source.contains("pub const Owner_Inner_VALUE:"),
            inner,
            "{source}"
        );
        assert_eq!(source.contains("pub type Owner_Inner ="), inner, "{source}");
    }
}

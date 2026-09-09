use toucan_bindgen::{Builder, Formatter};

#[test]
fn ignored_expression_type_operands_do_not_leak_to_later_declarators() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("types.h"),
        "typedef int Array[4]; void original(Array);\n",
    )
    .unwrap();
    let header = dir.path().join("selected.h");
    for (declarations, keep_array) in [
        ("int first=sizeof(void(*)(Array)), object;", false),
        ("int object[sizeof(void(*)(Array))];", false),
        ("__typeof__(sizeof(__typeof__(original)*)) object;", false),
        (
            "int object __attribute__((aligned(sizeof(void(*)(Array)))));",
            false,
        ),
        (
            "struct Holder { int values[sizeof(void(*)(Array))]; }; extern struct Holder *object;",
            false,
        ),
        (
            "int value=sizeof(struct Holder {void(*callback)(Array);}); extern struct Holder *object;",
            true,
        ),
        (
            "int first=sizeof(void(*)(Array)), object; __typeof__(original) after;",
            true,
        ),
    ] {
        std::fs::write(&header, format!("#include \"types.h\"\n{declarations}\n")).unwrap();
        let output = Builder::default()
            .header(header.to_str().unwrap())
            .allowlist_file(r".*[/\\]selected\.h")
            .clang_arg("--target=x86_64-unknown-linux-gnu")
            .formatter(Formatter::None)
            .layout_tests(false)
            .generate()
            .unwrap()
            .to_string();
        assert_eq!(
            output.contains("pub type Array ="),
            keep_array,
            "{declarations}\n{output}"
        );
    }
}

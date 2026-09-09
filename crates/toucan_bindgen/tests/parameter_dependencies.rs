use toucan_bindgen::{Builder, Formatter};

fn aliases(source: &str) -> Vec<&str> {
    source
        .lines()
        .filter_map(|line| {
            line.strip_prefix("pub type ")?
                .split_once(" = ")
                .map(|(name, _)| name)
        })
        .collect()
}

#[test]
fn array_alias_roots_follow_signature_occurrences_and_physical_files() {
    let cases = [
        ("void f(A);", "void f(D);", vec!["A"]),
        ("void f(int*);", "void f(A);", vec![]),
        (
            "void f(void(*old)(A));",
            "void f(void(*new)(D));",
            vec!["D"],
        ),
        ("extern void(*f)(A);", "extern void(*f)(D);", vec!["D"]),
        (
            "",
            "void f(void(*a)(A)); void f(void(*d)(D));",
            vec!["A", "D"],
        ),
        ("", "static void f(A);", vec!["A"]),
        ("", "inline void f(A value) {}", vec!["A"]),
        ("", "#line 400 \"elsewhere.h\"\nDECLARE(f);", vec!["A"]),
    ];
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root.h");
    std::fs::write(
        dir.path().join("types.h"),
        "typedef int A[4]; typedef int D[8];\n#define DECLARE(name) void name(A)\n",
    )
    .unwrap();
    std::fs::write(
        &root,
        "#include \"types.h\"\n#include \"excluded.h\"\n#include \"selected.h\"\n",
    )
    .unwrap();
    for (excluded, selected, expected) in cases {
        std::fs::write(dir.path().join("excluded.h"), excluded).unwrap();
        std::fs::write(dir.path().join("selected.h"), selected).unwrap();
        let source = Builder::default()
            .header(root.to_str().unwrap())
            .allowlist_file(r".*[/\\]selected\.h")
            .clang_arg("--target=x86_64-unknown-linux-gnu")
            .clang_arg("-std=c11")
            .formatter(Formatter::None)
            .layout_tests(false)
            .generate()
            .unwrap()
            .to_string();
        assert_eq!(
            aliases(&source),
            expected,
            "{excluded}\n{selected}\n{source}"
        );
    }
}

#[test]
fn transitively_selected_records_and_callback_typedefs_keep_array_aliases() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("types.h"), "typedef int Array[4]; typedef Array Chain; typedef void Callback(Chain); struct Holder { void(*callback)(Array); }; struct Typed { __typeof__(void(Chain)) *callback; }; typedef void Unused(Array);\n").unwrap();
    let root = dir.path().join("root.h");
    std::fs::write(&root, "#include \"types.h\"\nextern Callback *callback; extern struct Holder *holder; extern struct Typed *typed;\n").unwrap();
    let source = Builder::default()
        .header(root.to_str().unwrap())
        .allowlist_file(r".*[/\\]root\.h")
        .clang_arg("--target=x86_64-unknown-linux-gnu")
        .formatter(Formatter::None)
        .layout_tests(false)
        .generate()
        .unwrap()
        .to_string();
    assert_eq!(aliases(&source), ["Array", "Callback", "Chain"]);
    assert!(source.contains("pub struct Holder"));
    assert!(source.contains("pub struct Typed"));
    assert!(source.contains("*mut ::core::ffi::c_int"));
}

#[test]
fn function_type_queries_use_resolved_entities_and_keep_shadowing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("types.h"), "typedef int A[4]; typedef int D[8]; void original(void(*cb)(A)); void original(void(*cb)(D)); extern void(*pointer)(A);\n").unwrap();
    let root = dir.path().join("root.h");
    for (declarations, expected) in [
        ("__typeof__(original) chosen;", vec!["A"]),
        ("__typeof__(*pointer) chosen;", vec!["A"]),
        ("__typeof__(&original) chosen;", vec![]),
        ("__typeof__(pointer) chosen;", vec![]),
        (
            "void chosen(int (*original)(int), __typeof__(original) value);",
            vec![],
        ),
    ] {
        std::fs::write(&root, format!("#include \"types.h\"\n{declarations}\n")).unwrap();
        let source = Builder::default()
            .header(root.to_str().unwrap())
            .allowlist_file(r".*[/\\]root\.h")
            .clang_arg("--target=x86_64-unknown-linux-gnu")
            .formatter(Formatter::None)
            .layout_tests(false)
            .generate()
            .unwrap()
            .to_string();
        assert_eq!(aliases(&source), expected, "{declarations}\n{source}");
    }
}

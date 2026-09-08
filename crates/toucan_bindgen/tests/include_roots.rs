use toucan_bindgen::Builder;

#[test]
fn system_directory_class_changes_search_priority_without_losing_spelling() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    std::fs::write(a.join("api.h"), "int from_a(void);").unwrap();
    std::fs::write(b.join("api.h"), "int from_b(void);").unwrap();
    let header = dir.path().join("root.h");
    std::fs::write(&header, "#include <api.h>\n").unwrap();
    let output = Builder::default()
        .header(header.to_str().unwrap())
        .clang_args([
            "-I",
            a.to_str().unwrap(),
            "-I",
            b.to_str().unwrap(),
            "-isystem",
            a.to_str().unwrap(),
        ])
        .allowlist_file(regex::escape(b.join("api.h").to_str().unwrap()))
        .generate()
        .unwrap()
        .to_string();
    assert!(output.contains("pub fn from_b("), "{output}");
    assert!(!output.contains("from_a"));
}

#[test]
fn undefining_clang_macro_does_not_change_the_compiler_include_next_rules() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    std::fs::write(a.join("entry.h"), "#include \"child.h\"\n").unwrap();
    std::fs::write(a.join("child.h"), "#include_next <next.h>\n").unwrap();
    std::fs::write(a.join("next.h"), "int wrong(void);").unwrap();
    std::fs::write(b.join("next.h"), "int correct(void);").unwrap();
    let header = dir.path().join("root.h");
    std::fs::write(&header, "#undef __clang__\n#include <entry.h>\n").unwrap();
    let output = Builder::default()
        .header(header.to_str().unwrap())
        .clang_args(["-I", a.to_str().unwrap(), "-I", b.to_str().unwrap()])
        .generate()
        .unwrap()
        .to_string();
    assert!(output.contains("pub fn correct("), "{output}");
    assert!(!output.contains("pub fn wrong("));
}

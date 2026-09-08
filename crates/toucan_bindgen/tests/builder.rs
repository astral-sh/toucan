use toucan_bindgen::{Builder, RustTarget};

#[test]
fn ordered_headers_include_paths_and_options_generate_one_module() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("first.h"), "#include <stddef.h>\n#define VALUE INPUT\ntypedef enum Mode { MODE_A, MODE_B } Mode;\nstruct Record { char tag; long value; };\n").unwrap();
    std::fs::write(
        dir.path().join("second.h"),
        "void keep(struct Record *, Mode, size_t); void omitted(void);\n",
    )
    .unwrap();
    let builder = Builder::default()
        .header("first.h")
        .header("second.h")
        .clang_args([
            "-I",
            dir.path().to_str().unwrap(),
            "-DINPUT=7",
            "--target=x86_64-unknown-linux-gnu",
        ])
        .blocklist_type("max_align_t")
        .blocklist_function("^omitted$")
        .rustified_enum(".*")
        .rust_target(RustTarget::stable(64, 0).unwrap())
        .size_t_is_usize(true)
        .use_core();
    let checked = builder.clone().generate().unwrap();
    assert!(checked.to_string().contains("#[test]"));
    let bindings = builder.layout_tests(false).generate().unwrap();
    let source = bindings.to_string();
    assert!(source.contains("pub fn keep("));
    assert!(!source.contains("pub fn omitted("));
    assert!(source.contains("pub enum Mode"));
    assert!(source.contains("::core::primitive::usize"));
    assert!(
        source
            .lines()
            .any(|line| line.starts_with("pub const VALUE:") && line.ends_with("= 7;")),
        "{source}"
    );
    assert!(!source.contains("#[test]"));
    assert!(source.contains("::core::mem::align_of::<Record>()"));
    assert_eq!(bindings.report().dependencies.len(), 2);
    let output = dir.path().join("bindings.rs");
    bindings.write_to_file(&output).unwrap();
    assert_eq!(std::fs::read_to_string(output).unwrap(), source);
    assert!(
        bindings
            .write_to_file(dir.path().join("missing/bindings.rs"))
            .is_err()
    );
}

#[test]
fn declared_blocked_types_and_unsupported_patterns_are_explicit_errors() {
    let dir = tempfile::tempdir().unwrap();
    let header = dir.path().join("input.h");
    std::fs::write(&header, "typedef int Replacement; void f(Replacement);\n").unwrap();
    let builder = Builder::default().header(header.to_str().unwrap());
    assert!(
        builder
            .clone()
            .blocklist_type("Replacement")
            .generate()
            .unwrap_err()
            .to_string()
            .contains("external Rust type replacements")
    );
    assert!(
        builder
            .clone()
            .blocklist_function("f|g")
            .generate()
            .unwrap_err()
            .to_string()
            .contains("regex syntax")
    );
    assert!(
        builder
            .rustified_enum("OneEnum")
            .generate()
            .unwrap_err()
            .to_string()
            .contains("selective")
    );
}

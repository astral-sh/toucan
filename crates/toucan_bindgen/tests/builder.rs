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
        .header(dir.path().join("second.h").to_str().unwrap())
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
fn external_types_are_referenced_and_unsupported_patterns_are_explicit_errors() {
    let dir = tempfile::tempdir().unwrap();
    let header = dir.path().join("input.h");
    std::fs::write(&header, "typedef int Replacement; void f(Replacement);\n").unwrap();
    let builder = Builder::default().header(header.to_str().unwrap());
    let bindings = builder
        .clone()
        .blocklist_type("Replacement")
        .raw_line("pub type Replacement = ::core::ffi::c_int;")
        .generate()
        .unwrap();
    assert_eq!(bindings.report().blocked_types.len(), 1);
    assert!(bindings.report().blocked_types[0].referenced);
    assert_eq!(
        bindings.to_string().matches("pub type Replacement").count(),
        1
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
            .rustified_enum("OneEnum|OtherEnum")
            .generate()
            .unwrap_err()
            .to_string()
            .contains("regex syntax")
    );
}

#[test]
fn selective_enum_patterns_preserve_other_integer_enum_representations() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("enums.h");
    std::fs::write(&header, "typedef enum { COMPRESSED=2, UNCOMPRESSED=4, HYBRID=6 } point_conversion_form_t;\nenum Flags { OFF=0, ON=1 }; struct Bits { enum Flags value:1; };\n").unwrap();
    for pattern in [
        "point_conversion_form_t",
        "^point_conversion_form_t$",
        "point_conversion_.*",
    ] {
        let bindings = Builder::default()
            .header(header.to_str().unwrap())
            .rustified_enum(pattern)
            .generate()
            .unwrap()
            .to_string();
        assert!(bindings.contains("pub enum point_conversion_form_t {"));
        assert!(!bindings.contains("pub enum Flags"));
        assert!(bindings.contains("pub type Flags ="));
    }
}

#[test]
fn derive_options_preserve_eq_dependencies_and_enum_traits() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("traits.h");
    std::fs::write(&header, "struct Value { int field; }; typedef enum { TWO=2, FOUR=4 } Form; struct Holder { Form value; };\n").unwrap();
    let builder = Builder::default()
        .header(header.to_str().unwrap())
        .rustified_enum("Form")
        .derive_copy(false)
        .derive_debug(false)
        .derive_default(true)
        .derive_eq(true);
    let source = builder.clone().generate().unwrap().to_string();
    assert!(source.contains("#[derive(PartialEq, Eq)]\npub struct Value"));
    assert!(source.contains("#[derive(Clone, PartialEq, Eq, Hash)]\npub enum Form"));
    assert!(source.contains("impl ::core::default::Default for Value"));
    assert!(!source.contains("impl ::core::default::Default for Holder"));
    let source = builder
        .clone()
        .derive_eq(false)
        .generate()
        .unwrap()
        .to_string();
    assert!(source.contains("#[derive(PartialEq)]\npub struct Value"));
    let source = builder
        .derive_partialeq(false)
        .generate()
        .unwrap()
        .to_string();
    assert!(source.contains("#[repr(C)]\npub struct Value"));
}

#[test]
fn musl_sysroots_use_the_selected_libc_include_directory() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("api.h");
    std::fs::write(
        &header,
        "#include <target_marker.h>\nvoid accept(target_marker);\n",
    )
    .unwrap();
    for arch in ["x86_64", "aarch64"] {
        let include = directory
            .path()
            .join(format!("usr/include/{arch}-linux-musl"));
        std::fs::create_dir_all(&include).unwrap();
        std::fs::write(
            include.join("target_marker.h"),
            "typedef unsigned long target_marker;\n",
        )
        .unwrap();
        let source = Builder::default()
            .header(header.to_str().unwrap())
            .clang_arg(format!("--target={arch}-unknown-linux-musl"))
            .clang_arg(format!("--sysroot={}", directory.path().display()))
            .generate()
            .unwrap()
            .to_string();
        assert!(source.contains("target_env = \"musl\""));
        assert!(source.contains("pub fn accept("));
    }
}

#[test]
fn dll_library_rules_use_builder_patterns_and_explicit_scope() {
    let dir = tempfile::tempdir().unwrap();
    let header = dir.path().join("api.h");
    std::fs::write(&header,"__declspec(dllimport) int api_value; __declspec(dllimport) int api_special; __declspec(dllimport) int omitted(void); int ordinary(void);").unwrap();
    let builder = Builder::default()
        .header(header.to_str().unwrap())
        .clang_arg("--target=x86_64-pc-windows-msvc")
        .blocklist_function("omitted")
        .dll_import_library("api_.*", "old")
        .dll_import_library("^api_.*$", "alpha")
        .dll_import_library("^api_special$", "beta");
    let source = builder.clone().generate().unwrap().to_string();
    assert!(source.contains("name = \"alpha\""));
    assert!(source.contains("name = \"beta\""));
    assert!(!source.contains("name = \"old\""));
    assert!(!source.contains("pub fn omitted"));
    assert!(
        source.contains("}\n\nextern \"C\" {\n    pub fn ordinary("),
        "{source}"
    );
    assert!(
        builder
            .clone()
            .dll_import_library("api_[a-z]", "bad")
            .generate()
            .unwrap_err()
            .to_string()
            .contains("regex syntax")
    );
    assert!(
        builder
            .dll_import_library("api_.*", "")
            .generate()
            .unwrap_err()
            .to_string()
            .contains("nonempty")
    );
}

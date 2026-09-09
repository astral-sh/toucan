use toucan_bindgen::{Builder, Formatter};

#[derive(Debug)]
struct Rename;
impl toucan_bindgen::callbacks::ParseCallbacks for Rename {
    fn generated_name_override(
        &self,
        item: toucan_bindgen::callbacks::ItemInfo<'_>,
    ) -> Option<String> {
        Some(format!("renamed_{}", item.name))
    }
}

#[test]
fn forward_typedefs_do_not_copy_alias_comments_to_the_tag() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("forward.h");
    for source in [
        "/** ALIAS */ typedef struct Record Alias; /** DEFINITION */ struct Record {int value;};",
        "/** ALIAS */ typedef enum Record Alias; /** DEFINITION */ enum Record {VALUE=1};",
    ] {
        std::fs::write(&header, source).unwrap();
        let output = Builder::default()
            .header(header.to_string_lossy())
            .formatter(Formatter::None)
            .layout_tests(false)
            .generate()
            .unwrap()
            .to_string();
        assert_eq!(output.matches("#[doc = ").count(), 2, "{output}");
        assert!(
            output.contains("#[doc = \" ALIAS\"]\npub type Alias"),
            "{output}"
        );
        assert!(output.contains("#[doc = \" DEFINITION\"]"), "{output}");
    }
    for (source, comments) in [
        (
            "/** ALIAS */ typedef struct Record Record; struct Record {int value;};",
            0,
        ),
        ("/** ALIAS */ typedef struct Record Alias;", 1),
        (
            "/** FORWARD */ struct Record; /** DEFINITION */ struct Record {int value;};",
            1,
        ),
        ("struct Owner {/** FIELD */ struct Record *field;};", 2),
    ] {
        std::fs::write(&header, source).unwrap();
        let output = Builder::default()
            .header(header.to_string_lossy())
            .formatter(Formatter::None)
            .layout_tests(false)
            .generate()
            .unwrap()
            .to_string();
        assert_eq!(output.matches("#[doc = ").count(), comments, "{output}");
        if source.starts_with("/** FORWARD") {
            assert!(output.contains("#[doc = \" FORWARD\"]"), "{output}");
        }
    }
}

#[test]
fn object_projection_keeps_docs_on_renamed_scalar_and_string_constants() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("objects.h");
    std::fs::write(&header, "/** SCALAR */ int scalar=7; /** STRING */ const char text[]=\"abc\"; /** WIDE */ static const __int128 wide=((__int128)1)<<100;").unwrap();
    let source = Builder::default()
        .header(header.to_string_lossy())
        .clang_arg("--target=x86_64-unknown-linux-gnu")
        .parse_callbacks(Box::new(Rename))
        .formatter(Formatter::None)
        .layout_tests(false)
        .generate()
        .unwrap()
        .to_string();
    for (comment, name) in [
        ("SCALAR", "renamed_scalar"),
        ("STRING", "renamed_text"),
        ("WIDE", "wide"),
    ] {
        assert!(
            source.contains(&format!("#[doc = \" {comment}\"]\npub const {name}:")),
            "{source}"
        );
    }
    assert!(
        source.contains("1267650600228229401496703205376"),
        "{source}"
    );
    assert_eq!(source.matches("#[doc = ").count(), 3, "{source}");
}

#[test]
fn comments_follow_declarations_fields_and_enum_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.h");
    std::fs::write(&path, "/** RECORD */ struct Record { /** FIELD */ int value; }; /** ENUM */ enum Mode { VALUE=1 /**< VALUE */ }; /** TYPEDEF */ typedef int Alias; /** FUNCTION */ int function(Alias); /** OBJECT */ extern int object;").unwrap();
    let builder = Builder::default()
        .header(path.to_string_lossy())
        .formatter(Formatter::None)
        .layout_tests(false);
    let source = builder.clone().generate().unwrap().to_string();
    for text in [
        "RECORD", "FIELD", "ENUM", "VALUE", "TYPEDEF", "FUNCTION", "OBJECT",
    ] {
        assert!(source.contains(&format!(" {text}")), "{source}");
    }
    assert_eq!(source.matches("#[doc = ").count(), 7, "{source}");
    assert!(
        !builder
            .generate_comments(false)
            .generate()
            .unwrap()
            .to_string()
            .contains("#[doc = ")
    );
}

#[test]
fn configured_container_macros_do_not_copy_parent_docs_to_members() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.h");
    std::fs::write(&path, "/** OWNER */\nDECL\n").unwrap();
    for value in [
        "struct Owner { int field; };",
        "enum Mode { VALUE=1 };",
        "typedef struct { int field; } Alias;",
    ] {
        let source = Builder::default()
            .header(path.to_string_lossy())
            .clang_arg(format!("-DDECL={value}"))
            .formatter(Formatter::None)
            .layout_tests(false)
            .generate()
            .unwrap()
            .to_string();
        assert_eq!(source.matches("#[doc = ").count(), 1, "{source}");
    }
}

#[test]
fn macro_comments_use_invocation_then_declaration_begin_spelling() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.h");
    std::fs::write(&path, "#define DECL(name) /** REPLACEMENT */ int name(void);\n/** INVOCATION */\nDECL(first);\nDECL(second)\n#define OWNER struct Record { int field; };\n/** RECORD */ OWNER\n").unwrap();
    let source = Builder::default()
        .header(path.to_string_lossy())
        .formatter(Formatter::None)
        .layout_tests(false)
        .generate()
        .unwrap()
        .to_string();
    assert!(source.contains("#[doc = \" INVOCATION\"]"), "{source}");
    assert!(source.contains("#[doc = \" REPLACEMENT\"]"), "{source}");
    assert_eq!(source.matches("#[doc = ").count(), 3, "{source}");
}

#[test]
fn synthetic_anonymous_fields_and_repeated_enum_aliases_do_not_duplicate_docs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.h");
    std::fs::write(&path, "struct Owner { /** ANON */ struct { /** LEAF */ int value; }; }; /** ENUM */ enum Mode { /** FIRST */ FIRST=1, /** ALIAS */ ALIAS=1 };").unwrap();
    let source = Builder::default()
        .header(path.to_string_lossy())
        .rustified_enum("Mode")
        .formatter(Formatter::None)
        .layout_tests(false)
        .generate()
        .unwrap()
        .to_string();
    assert_eq!(source.matches("#[doc = ").count(), 4, "{source}");
    assert_eq!(source.matches(" ANON").count(), 1, "{source}");
    assert!(!source.contains("#[doc = \" ALIAS\"]"), "{source}");
}

#[test]
#[ignore = "requires rustc; TOUCAN_DOCUMENTATION_RUSTC can select the output MSRV"]
fn escaped_documentation_compiles_as_rust_attributes() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("input.h");
    std::fs::write(
        &header,
        "/** Read \"quoted\" paths C:\\tmp\\file and café.\n * Keep tabs\there and another line.\n */\ntypedef int Documented;\n",
    )
    .unwrap();
    let source = Builder::default()
        .header(header.to_string_lossy())
        .formatter(Formatter::None)
        .layout_tests(false)
        .generate()
        .unwrap()
        .to_string();
    assert!(source.contains("\\\"quoted\\\""), "{source}");
    assert!(source.contains("C:\\\\tmp\\\\file"), "{source}");
    assert!(source.contains("café.\\n Keep tabs\\there"), "{source}");
    let output = directory.path().join("bindings.rs");
    std::fs::write(&output, source).unwrap();
    let rustc = std::env::var_os("TOUCAN_DOCUMENTATION_RUSTC").unwrap_or_else(|| "rustc".into());
    let result = std::process::Command::new(rustc)
        .args(["--edition=2021", "--crate-type=lib", "--emit=metadata"])
        .arg(&output)
        .arg("--out-dir")
        .arg(directory.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

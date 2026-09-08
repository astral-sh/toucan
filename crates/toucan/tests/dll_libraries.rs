use std::collections::BTreeMap;

use toucan::{BindingOptions, Target};

const HEADER: &str = r#"
typedef int Number;
__declspec(dllimport) int a_value;
int ordinary(void);
__declspec(dllimport) int b_value;
__declspec(dllimport) int a_function(void);
__declspec(dllimport) int a_special;
__declspec(dllimport) int unmatched_function(void);
__declspec(dllexport) extern int exported;
"#;

fn generate(header: &str, rules: &[(&str, &str)]) -> Result<String, toucan::Error> {
    let parsed = toucan::parse_source(
        std::path::Path::new("api.h"),
        header,
        &toucan::Config::new(Target::X86_64PcWindowsMsvc),
    )?;
    parsed
        .bindings(&BindingOptions {
            dll_import_libraries: rules
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            rust_target: toucan::RustTarget::RUST_1_64,
            ..Default::default()
        })
        .map(|result| result.0)
}

#[test]
fn foreign_blocks_follow_library_and_abi_scope() {
    let source = generate(
        HEADER,
        &[
            ("a*", "wrong"),
            ("a_*", "alpha"),
            ("a_special", "beta"),
            ("b_*", "beta"),
        ],
    )
    .unwrap();
    assert!(!source.contains("name = \"wrong\""));
    assert_eq!(source.matches("#[link(name = \"alpha\"").count(), 2);
    assert_eq!(source.matches("#[link(name = \"beta\"").count(), 2);
    assert!(source.contains("#[link(name = \"alpha\", kind = \"dylib\")]\nextern \"C\" {\n    pub static mut a_value:"),"{source}");
    assert!(
        source.contains("}\n\nextern \"C\" {\n    pub fn ordinary("),
        "{source}"
    );
    assert!(
        source.contains("}\n\nextern \"C\" {\n    pub fn unmatched_function("),
        "{source}"
    );
    let last = source.rfind("extern \"C\"").unwrap();
    assert!(source[last..].contains("pub static mut exported:"));
    assert!(!source[last..].contains("#[link("));
    let abi = generate("__declspec(dllimport) int first(void); __declspec(dllimport) __attribute__((sysv_abi)) int second(void); __declspec(dllimport) int third(void);", &[("*","alpha")]).unwrap();
    assert_eq!(abi.matches("#[link(name = \"alpha\"").count(), 3, "{abi}");
    assert!(abi.contains("extern \"sysv64\""), "{abi}");
}

#[test]
fn selection_and_ignored_storage_do_not_receive_link_attributes() {
    let header = "__declspec(dllimport) int selected; __declspec(dllimport) int unselected; __declspec(dllexport) int exported; int ordinary; __declspec(dllimport) typedef int Alias;";
    let parsed = toucan::parse_source(
        std::path::Path::new("api.h"),
        header,
        &toucan::Config::new(Target::X86_64PcWindowsMsvc),
    )
    .unwrap();
    let options = BindingOptions {
        allowlist: vec![
            "selected".into(),
            "exported".into(),
            "ordinary".into(),
            "Alias".into(),
        ],
        dll_import_libraries: BTreeMap::from([
            ("selected".into(), "alpha".into()),
            ("unselected".into(), "unrelated".into()),
            ("ordinary".into(), "wrong".into()),
            ("exported".into(), "wrong".into()),
            ("Alias".into(), "wrong".into()),
        ]),
        ..Default::default()
    };
    let source = parsed.bindings(&options).unwrap().0;
    assert_eq!(source.matches("#[link(").count(), 1, "{source}");
    assert!(!source.contains("unselected"));
    assert!(!source.contains("unrelated"));
    assert!(!source.contains("wrong"));
    assert!(
        generate(header, &[("selected", "alpha")])
            .unwrap_err()
            .to_string()
            .contains("unselected")
    );
    assert!(
        !generate(
            "typedef int Alias; void ordinary(void);",
            &[("*", "unused")]
        )
        .unwrap()
        .contains("#[link(")
    );
}

#[test]
fn invalid_rules_are_diagnosed_and_library_literals_are_escaped() {
    for pattern in ["", "a*b", "a**", "a.*", "1name", "a b", "a|b"] {
        assert!(
            generate("int ordinary;", &[(pattern, "library")])
                .unwrap_err()
                .to_string()
                .contains("DLL import pattern"),
            "{pattern}"
        );
    }
    for name in ["", "a\0b", "a\nb", "a\rb", "a\tb", "a\u{7f}b"] {
        assert!(
            generate("int ordinary;", &[("*", name)])
                .unwrap_err()
                .to_string()
                .contains("control characters"),
            "{name:?}"
        );
    }
    let source = generate("__declspec(dllimport) int value;", &[("value", "a\"b\\c")]).unwrap();
    assert!(
        source.contains(r##"#[link(name = "a\"b\\c", kind = "dylib")]"##),
        "{source}"
    );
    let source = generate(
        "__declspec(dllimport) int value;",
        &[("value", "first"), ("value", "second")],
    )
    .unwrap();
    assert!(source.contains("name = \"second\""));
    assert!(!source.contains("name = \"first\""));
}

#[test]
fn aliases_of_one_imported_symbol_require_consistent_library_rules() {
    let source = "__declspec(dllimport) extern int first __asm__(\"shared\"); __declspec(dllimport) extern int second __asm__(\"shared\");";
    assert!(
        generate(source, &[("first", "alpha"), ("second", "beta")])
            .unwrap_err()
            .to_string()
            .contains("conflicting library rules")
    );
    assert!(
        generate(source, &[("first", "alpha"), ("second", "alpha")])
            .unwrap()
            .contains("link_name = \"shared\"")
    );
}

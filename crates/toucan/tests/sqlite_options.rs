use std::path::Path;

use toucan::{BindingOptions, Config, MacroType, Target};

fn parse(source: &str) -> toucan::Compilation {
    let mut config = Config::new(Target::X86_64UnknownLinuxGnu);
    config.preprocessor.defines.clear();
    toucan::parse_source(Path::new("api.h"), source, &config).unwrap()
}

#[test]
fn macro_policies_use_the_most_specific_name_without_changing_c_types() {
    let compilation = parse(
        "#define FLAG_OK 1\n#define FLAG_KEEP 2\n#define FLAG_MORE_ONE 3\n#define FLAG_MORE_TWO 4\n#define CODE 5\n",
    );
    let (source, report) = compilation
        .bindings(&BindingOptions {
            macro_type_overrides: [
                ("FLAG*".into(), MacroType::Unsigned),
                ("FLAG_MORE*".into(), MacroType::C),
                ("FLAG_MORE_ONE".into(), MacroType::Unsigned),
                ("FLAG_KEEP".into(), MacroType::C),
            ]
            .into(),
            ..Default::default()
        })
        .unwrap();
    for name in ["FLAG_OK", "FLAG_MORE_ONE"] {
        assert!(source.contains(&format!("pub const {name}: ::core::primitive::u32")));
    }
    for name in ["FLAG_KEEP", "FLAG_MORE_TWO", "CODE"] {
        assert!(source.contains(&format!("pub const {name}: ::core::primitive::i32")));
    }
    assert_eq!(report.macro_types.len(), 2);
    assert!(
        report
            .macro_types
            .iter()
            .all(|value| value.c_bits == 32 && value.c_signed && !value.rust_signed)
    );
}

#[test]
fn blocked_functions_and_caller_rust_remain_visible_in_the_report() {
    let compilation = parse(
        "int keep(void); long double remove_one(long double); int remove_two(void); extern int remove_variable;",
    );
    let raw = "// caller supplied\nunsafe extern \"C\" { pub fn replacement() -> i32; }";
    let (source, report) = compilation
        .bindings(&BindingOptions {
            allowlist: vec!["keep".into(), "remove_*".into()],
            blocklist_functions: vec!["remove_*".into()],
            raw_lines: vec![raw.into()],
            ..Default::default()
        })
        .unwrap();
    assert!(source.contains("pub fn keep("));
    assert!(source.contains("pub static mut remove_variable:"));
    assert!(!source.contains("pub fn remove_"));
    assert!(source.ends_with(&format!("{raw}\n")));
    assert_eq!(report.blocked_functions, ["remove_one", "remove_two"]);
    assert_eq!(report.raw_lines, [raw]);
    assert_eq!(report.declarations, 2);
    assert!(
        compilation
            .unit
            .declarations
            .iter()
            .any(|declaration| declaration.name == "remove_one")
    );
}

#[test]
fn cstr_projection_checks_interior_nuls_and_handles_empty_strings() {
    let compilation = parse("#define EMPTY \"\"\n#define TEXT \"a\\n\" \"b\"\n");
    let (source, report) = compilation
        .bindings(&BindingOptions {
            generate_cstr: true,
            ..Default::default()
        })
        .unwrap();
    assert!(source.contains("pub const EMPTY: &::core::ffi::CStr"));
    assert!(source.contains("from_bytes_with_nul_unchecked(&[0])"));
    assert!(source.contains("from_bytes_with_nul_unchecked(&[97, 10, 98, 0])"));
    assert_eq!(report.string_macros, 2);
    for literal in [r#""a\0b""#, r#""\0""#, r#""a\x00""#] {
        let compilation = parse(&format!("#define BAD {literal}\n"));
        assert!(compilation.bindings(&BindingOptions::default()).is_ok());
        assert!(
            compilation
                .bindings(&BindingOptions {
                    generate_cstr: true,
                    ..Default::default()
                })
                .unwrap_err()
                .to_string()
                .contains("interior NUL")
        );
    }
}

#[test]
fn a_macro_can_use_the_name_of_an_explicitly_blocked_function() {
    let compilation = parse("int API(void);\n#define API 5\n");
    let (source, report) = compilation
        .bindings(&BindingOptions {
            blocklist_functions: vec!["API".into()],
            ..Default::default()
        })
        .unwrap();
    assert!(!source.contains("pub fn API("));
    assert!(source.contains("pub const API: ::core::primitive::i32 = 5;"));
    assert_eq!(report.blocked_functions, ["API"]);
}

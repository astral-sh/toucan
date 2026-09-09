use toucan_bindings::{EnumConstantStyle, Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

const HEADER: &str = r#"
    struct Record { int value; };
    typedef struct Record RecordAlias;
    typedef RecordAlias RecordAlias2;
    typedef RecordAlias2 Record;
    union Value { int number; double fraction; };
    typedef union Value ValueAlias;
    typedef ValueAlias Value;
    enum Kind { KIND = 0 };
    typedef enum Kind KindAlias;
    typedef KindAlias Kind;
    struct Opaque;
    typedef struct Opaque OpaqueAlias;
    typedef OpaqueAlias Opaque;
"#;

fn bindings(enum_constant_style: EnumConstantStyle, rustified_enums: bool) -> String {
    let unit = analyze(HEADER, Target::X86_64UnknownLinuxGnu).unwrap();
    generate(
        &unit,
        &Options {
            enum_constant_style,
            rustified_enums,
            ..Default::default()
        },
    )
    .unwrap()
    .source
}

#[test]
fn indirect_tag_aliases_do_not_redefine_the_tag() {
    for style in [EnumConstantStyle::Integer, EnumConstantStyle::Bindgen] {
        for rustified in [false, true] {
            let source = bindings(style, rustified);
            for name in ["Record", "Value", "Opaque"] {
                assert!(!source.contains(&format!("pub type {name} =")), "{source}");
                assert!(source.contains(&format!("pub type {name}Alias = {name};")));
            }
            assert!(source.contains("pub type RecordAlias2 = RecordAlias;"));
            assert!(!source.contains("pub type Kind = KindAlias;"), "{source}");
            assert!(source.contains("pub type KindAlias = Kind;"));
        }
    }
}

#[test]
fn external_intermediate_aliases_keep_their_rust_identity() {
    for (declaration, kind) in [
        ("struct S { int value; };", "record"),
        ("union S { int value; };", "record"),
        ("enum S { VALUE = 0 };", "enum"),
        ("struct S;", "record"),
    ] {
        let keyword = declaration.split_whitespace().next().unwrap();
        let unit = analyze(
            &format!("{declaration} typedef {keyword} S T; typedef T U; typedef U S;"),
            Target::X86_64UnknownLinuxGnu,
        )
        .unwrap();
        for enum_constant_style in [EnumConstantStyle::Integer, EnumConstantStyle::Bindgen] {
            let output = generate(
                &unit,
                &Options {
                    blocklist_types: vec!["T".into()],
                    enum_constant_style,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(output.source.contains("pub type S = U;"));
            assert!(output.source.contains("pub type U = T;"));
            assert!(output.source.contains(&format!("__toucan_{kind}_")));
            assert!(!output.source.contains("pub type T ="));
            assert!(output.blocked_types.iter().any(|ty| ty.rust_name == "T"));
        }
    }
}

#[test]
fn external_intermediate_aliases_retain_lexical_collision_diagnostics() {
    for definition in ["struct S { int value; }", "enum S { VALUE = 0 }"] {
        let keyword = definition.split_whitespace().next().unwrap();
        let unit = analyze(
            &format!(
                "struct Owner {{ {definition} member; }}; typedef {keyword} S T; typedef T Owner_S;"
            ),
            Target::X86_64UnknownLinuxGnu,
        )
        .unwrap();
        let error = generate(
            &unit,
            &Options {
                blocklist_types: vec!["T".into()],
                enum_constant_style: EnumConstantStyle::Bindgen,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.0.contains("type name `Owner_S` conflicts"), "{error}");
    }
}

#[test]
fn unrelated_typedefs_keep_both_type_definitions() {
    for declaration in [
        "struct S { int value; };",
        "union S { int value; };",
        "enum S { VALUE = 0 };",
    ] {
        let unit = analyze(
            &format!("{declaration} typedef int T; typedef T S;"),
            Target::X86_64UnknownLinuxGnu,
        )
        .unwrap();
        let output = generate(&unit, &Options::default()).unwrap();
        assert!(output.source.contains("pub type S = T;"));
        assert!(output.source.contains("pub type T = ::core::ffi::c_int;"));
    }
}

#[test]
#[ignore = "requires native x86-64 Linux rustc; run with --include-ignored"]
fn indirect_tag_aliases_compile_with_shared_type_identity() {
    use std::process::Command;

    if std::env::consts::ARCH != "x86_64" || std::env::consts::OS != "linux" {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let consumer = r#"
        pub fn record(value: Record) -> RecordAlias2 { value }
        pub fn union(value: Value) -> ValueAlias { value }
        pub fn enumeration(value: Kind) -> KindAlias { value }
        pub fn opaque(value: *mut Opaque) -> *mut OpaqueAlias { value }
    "#;
    let mut cases = Vec::new();
    for style in [EnumConstantStyle::Integer, EnumConstantStyle::Bindgen] {
        for rustified in [false, true] {
            cases.push(format!("{}{consumer}", bindings(style, rustified)));
        }
    }
    let unit = analyze(
        "struct S { int value; }; typedef struct S T; typedef T U; typedef U S;",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    cases.push(
        generate(
            &unit,
            &Options {
                blocklist_types: vec!["T".into()],
                raw_lines: vec![
                    "#[repr(C)] pub struct T { pub value: i32 }".into(),
                    "pub fn caller_owned(value: S) -> T { value }".into(),
                ],
                ..Default::default()
            },
        )
        .unwrap()
        .source,
    );
    for source in cases {
        std::fs::write(directory.path().join("bindings.rs"), source).unwrap();
        let output = Command::new("rustc")
            .current_dir(directory.path())
            .args(["--edition=2024", "--crate-type=lib", "bindings.rs"])
            .output()
            .unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(true),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

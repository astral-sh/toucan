use std::process::Command;
use toucan_bindings::{EnumConstantStyle, Options, RustTarget, generate};
use toucan_semantic::{TagDiscovery, TypeKind, analyze};
use toucan_target::Target;
use toucan_test_support::{compiler_acceptance, link_c_object};

fn options() -> Options {
    Options {
        enum_constant_style: EnumConstantStyle::Bindgen,
        prepend_enum_name: true,
        rust_target: RustTarget::RUST_1_64,
        ..Options::default()
    }
}

#[test]
fn hidden_helpers_do_not_consume_names_or_change_core_output() {
    let unit = analyze(
        "enum Outer { COUNT=sizeof(enum { HIDDEN=1 }) }; enum { VISIBLE=2 };",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let output = generate(&unit, &options()).unwrap().source;
    assert!(!output.contains("pub const HIDDEN:"));
    assert!(output.contains("pub type _bindgen_ty_1 ="), "{output}");
    assert!(!output.contains("_bindgen_ty_2"));
    assert!(
        generate(&unit, &Options::default())
            .unwrap()
            .source
            .contains("pub const HIDDEN:")
    );
}

#[test]
fn discovery_names_drive_enum_selection_and_validate_public_owners() {
    let mut unit=analyze("struct Parent { enum Outer { COUNT=sizeof(enum Inner { VALUE=1 }) } field; }; struct Owner { enum Inner field; };",Target::X86_64UnknownLinuxGnu).unwrap();
    for (pattern, expected) in [
        ("Owner_Inner", true),
        ("Parent_Inner", false),
        ("Inner", false),
    ] {
        let output = generate(
            &unit,
            &Options {
                rustified_enum_patterns: vec![pattern.into()],
                ..options()
            },
        )
        .unwrap()
        .source;
        assert_eq!(
            output.contains("pub enum Owner_Inner"),
            expected,
            "{output}"
        );
    }
    let id = unit
        .enums
        .iter()
        .position(|item| item.name.as_deref() == Some("Inner"))
        .unwrap();
    assert!(
        matches!(unit.records.iter().find(|item|item.name.as_deref()==Some("Owner")).unwrap().fields.as_ref().unwrap()[0].ty.kind,TypeKind::Enum(found)if found==id)
    );
    unit.tag_discovery.as_mut().unwrap().enums.insert(
        id,
        TagDiscovery::Discovered {
            record: Some(usize::MAX),
            offset: 0,
            order: 0,
        },
    );
    assert!(
        generate(&unit, &options())
            .unwrap_err()
            .to_string()
            .contains("invalid or hidden record owner")
    );
}

#[test]
#[ignore = "requires native Unix C and Rust; TOUCAN_TEST_RUST_TOOLCHAIN selects Rust1.64"]
fn discovered_enums_cross_c_calls_and_record_layouts() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let header = "enum Outer { COUNT=sizeof(enum Inner { VALUE=7 }) }; struct { int ignored; } source; struct Owner { __typeof__(source); enum Inner field; }; enum Inner echo(enum Inner); int read_owner(const struct Owner *); struct Aligned {char field;} __attribute__((aligned(sizeof(enum Attribute {ATTRIBUTE=3})))); struct Aligned from_attribute(enum Attribute); enum __attribute__((aligned(4))) AlignedEnum {ALIGNED=5}; enum AlignedEnum echo_aligned(enum AlignedEnum); int read_aligned(const enum AlignedEnum *);";
    let unit = analyze(header, target).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let c = directory.path().join("probe.c");
    std::fs::write(&c,format!("{header}\n_Static_assert(sizeof(struct Aligned)==4 && _Alignof(struct Aligned)==4,\"aligned record\"); enum Inner echo(enum Inner value) {{ return value; }} int read_owner(const struct Owner *value) {{ return value->field; }} struct Aligned from_attribute(enum Attribute value) {{struct Aligned result={{(char)value}}; return result;}} enum AlignedEnum echo_aligned(enum AlignedEnum value) {{return value;}} int read_aligned(const enum AlignedEnum *value) {{return *value;}}")).unwrap();
    for rustified_enums in [false, true] {
        let bindings = generate(
            &unit,
            &Options {
                rustified_enums,
                ..options()
            },
        )
        .unwrap()
        .source;
        let value = if rustified_enums {
            "Owner_Inner::VALUE"
        } else {
            "Owner_Inner_VALUE"
        };
        let attribute = if rustified_enums {
            "Aligned_Attribute::ATTRIBUTE"
        } else {
            "Aligned_Attribute_ATTRIBUTE"
        };
        let aligned = if rustified_enums {
            "AlignedEnum::ALIGNED"
        } else {
            "AlignedEnum_ALIGNED"
        };
        let main = directory.path().join("main.rs");
        std::fs::write(&main,format!("#![allow(dead_code,non_camel_case_types,non_upper_case_globals)]\n{bindings}\nfn main() {{ let value=Owner {{field:{value}}}; assert_eq!(unsafe{{read_owner(&value)}},7); assert_eq!(unsafe{{echo({value})}},{value}); assert_eq!(unsafe{{from_attribute({attribute})}}.field,3); assert_eq!(::core::mem::size_of::<Aligned>(),4); assert_eq!(::core::mem::align_of::<Aligned>(),4); let aligned={aligned}; assert_eq!(unsafe{{read_aligned(&aligned)}},5); assert_eq!(unsafe{{echo_aligned(aligned)}},aligned); }}")).unwrap();
        for compiler in [
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            "clang".into(),
        ] {
            let object = directory.path().join("probe.o");
            let result = Command::new(compiler)
                .args(["-std=c11", "-O2", "-c"])
                .arg(&c)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert_eq!(
                compiler_acceptance(&result),
                Ok(true),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let executable = directory.path().join("probe");
            let mut rustc = Command::new("rustc");
            if let Some(toolchain) = std::env::var_os("TOUCAN_TEST_RUST_TOOLCHAIN") {
                rustc.arg(format!("+{}", toolchain.to_string_lossy()));
            }
            rustc
                .args(["--edition=2021", "-Dwarnings"])
                .arg(&main)
                .arg("-o")
                .arg(&executable);
            link_c_object(&mut rustc, &object);
            let result = rustc.output().unwrap();
            assert_eq!(
                compiler_acceptance(&result),
                Ok(true),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(Command::new(executable).status().unwrap().success());
        }
    }
}

#[test]
fn delayed_anonymous_types_follow_discovery_order() {
    let unit=analyze("enum Outer { COUNT=sizeof(struct Hidden { struct { int x; } field; }) }; struct { int y; } normal; typedef __typeof__(((struct Hidden*)0)->field) Alias;",Target::X86_64UnknownLinuxGnu).unwrap();
    let source = generate(&unit, &options()).unwrap().source;
    assert!(
        source.contains("pub type Alias = _bindgen_ty_2;"),
        "{source}"
    );
    assert!(
        source.contains("pub static mut normal: _bindgen_ty_1;"),
        "{source}"
    );
    assert!(!source.contains("pub struct Hidden"));
}

#[test]
fn trailing_record_attribute_names_match_bindgen_and_drive_enum_selection() {
    for (source, name, constant) in [
        (
            "struct Record {char field;} __attribute__((aligned(sizeof(enum Visible {VALUE=1}))));",
            "Record_Visible",
            "Record_Visible_VALUE",
        ),
        (
            "typedef struct {char field;} __attribute__((aligned(sizeof(enum Visible {VALUE=1})))) Alias;",
            "Alias_Visible",
            "Alias_Visible_VALUE",
        ),
        (
            "struct Record {char field;} __attribute__((aligned(sizeof(enum {VALUE=1}))));",
            "Record__bindgen_ty_1",
            "Record_VALUE",
        ),
        (
            "struct Record {char field;} __attribute__((aligned(sizeof(struct Inner {enum Visible {VALUE=1} field;}))));",
            "Record_Inner_Visible",
            "Record_Inner_Visible_VALUE",
        ),
    ] {
        let unit = analyze(source, Target::X86_64UnknownLinuxGnu).unwrap();
        let output = generate(&unit, &options()).unwrap().source;
        assert!(output.contains(&format!("pub type {name} =")), "{output}");
        assert!(
            output.contains(&format!("pub const {constant}:")),
            "{output}"
        );
        let output = generate(
            &unit,
            &Options {
                rustified_enum_patterns: vec![name.into()],
                ..options()
            },
        )
        .unwrap()
        .source;
        assert!(output.contains(&format!("pub enum {name}")), "{output}");
        let mut plain = unit.clone();
        plain.tag_discovery = None;
        assert_eq!(
            generate(&unit, &Options::default()).unwrap().source,
            generate(&plain, &Options::default()).unwrap().source
        );
    }
    let unit = analyze(
        "struct Record {char field;} __attribute__((aligned(sizeof(enum Visible {VALUE=1}))));",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let output = generate(
        &unit,
        &Options {
            rustified_enum_patterns: vec!["Visible".into()],
            ..options()
        },
    )
    .unwrap()
    .source;
    assert!(!output.contains("pub enum Record_Visible"));
}

#[test]
fn aligned_enum_attribute_children_follow_bindgen_cursor_visibility() {
    for (source, visible) in [
        (
            "enum E{VALUE=1} __attribute__((aligned(sizeof(enum Other{OTHER=1}))));",
            false,
        ),
        (
            "enum __attribute__((aligned(sizeof(enum Other{OTHER=1})))) E{VALUE=1};",
            true,
        ),
        (
            "__attribute__((aligned(sizeof(enum Other{OTHER=1})))) enum E{VALUE=1};",
            true,
        ),
        (
            "enum Outer{COUNT=sizeof(enum Hidden{HIDDEN=1} __attribute__((aligned(sizeof(enum Other{OTHER=1})))))};enum Hidden object;",
            false,
        ),
    ] {
        for rustified_enums in [false, true] {
            let unit = analyze(source, Target::X86_64UnknownLinuxGnu).unwrap();
            let output = generate(
                &unit,
                &Options {
                    rustified_enums,
                    ..options()
                },
            )
            .unwrap()
            .source;
            assert_eq!(
                output.contains("pub type Other =") || output.contains("pub enum Other"),
                visible,
                "{output}"
            );
            assert_eq!(
                output.contains("pub const Other_OTHER:"),
                visible && !rustified_enums,
                "{output}"
            );
            assert!(
                generate(&unit, &Options::default())
                    .unwrap()
                    .source
                    .contains("pub const OTHER:")
            );
        }
    }
}

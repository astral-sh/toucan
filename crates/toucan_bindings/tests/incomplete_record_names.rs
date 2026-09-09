use toucan_bindings::{EnumConstantStyle, Options, generate};
use toucan_semantic::{Scope, analyze};
use toucan_target::Target;

fn bindings(source: &str) -> String {
    let unit = analyze(source, Target::X86_64UnknownLinuxGnu).unwrap();
    generate(
        &unit,
        &Options {
            enum_constant_style: EnumConstantStyle::Bindgen,
            ..Options::default()
        },
    )
    .unwrap()
    .source
}

#[test]
fn incomplete_member_tags_use_the_file_scope_name() {
    for source in [
        "typedef struct { struct T *data; } Owner;",
        "struct Owner { struct T *data; }; struct T *get(void);",
        "struct Owner { struct T *data; }; void set(void (*cb)(struct T *));",
        "struct Owner { struct T; struct T *data; };",
        "struct First { struct T *data; }; struct Second { struct T *data; };",
        "struct Owner { union T *data; }; union T *get(void);",
        "struct Owner { struct T *data; }; typedef struct T T;",
    ] {
        let generated = bindings(source);
        assert!(
            generated.contains("pub struct T { _private:"),
            "{generated}"
        );
        assert!(generated.contains("pub data: *mut T,"), "{generated}");
        assert!(!generated.contains("Owner_T"), "{generated}");
        assert!(!generated.contains("_address"), "{generated}");
    }
}

#[test]
fn completed_tags_keep_the_definition_owner() {
    for (source, name) in [
        ("struct Owner { struct T { int value; } data; };", "Owner_T"),
        (
            "struct Owner { struct T *data; }; struct Other { struct T { int value; } data; };",
            "Other_T",
        ),
        (
            "struct Owner { struct T *data; }; struct T { int value; };",
            "T",
        ),
    ] {
        let generated = bindings(source);
        assert!(
            generated.contains(&format!("pub struct {name} {{\n    pub value:")),
            "{generated}"
        );
    }
}

#[test]
fn conflicting_typedef_keeps_the_qualified_opaque_name() {
    for source in [
        "struct Owner { struct T *data; }; typedef int T;",
        "typedef int T; struct Owner { struct T *data; };",
    ] {
        let generated = bindings(source);
        assert!(generated.contains("pub struct Owner_T { _private:"));
        assert!(generated.contains("pub data: *mut Owner_T,"));
        assert!(generated.contains("pub type T ="));
    }
}

#[test]
fn prototype_tag_identity_stays_distinct() {
    let source = "void set(void (*cb)(struct T *)); struct Owner { struct T *data; };";
    let unit = analyze(source, Target::X86_64UnknownLinuxGnu).unwrap();
    let tags: Vec<_> = unit
        .records
        .iter()
        .enumerate()
        .filter(|(_, record)| record.name.as_deref() == Some("T"))
        .collect();
    assert_eq!(tags.len(), 2);
    let prototype = tags
        .iter()
        .find(|(_, record)| record.scope != Scope::File)
        .unwrap()
        .0;
    let generated = bindings(source);
    assert!(generated.contains("pub struct T { _private:"));
    assert!(generated.contains(&format!("fn(arg0: *mut __toucan_record_{prototype})")));
    assert!(generated.contains("pub data: *mut T,"));
}

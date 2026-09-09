use toucan_semantic::{TagDiscovery, Type, TypeKind, analyze};
use toucan_target::Target;

fn enumeration(unit: &toucan_semantic::TranslationUnit, name: &str) -> usize {
    unit.enums
        .iter()
        .position(|item| item.name.as_deref() == Some(name))
        .unwrap()
}
fn record(unit: &toucan_semantic::TranslationUnit, name: &str) -> usize {
    unit.records
        .iter()
        .position(|item| item.name.as_deref() == Some(name))
        .unwrap()
}

#[test]
fn enum_values_keep_types_and_constants_without_exposing_cursors() {
    let unit=analyze("struct Parent { enum Outer { COUNT=sizeof(enum Inner { VALUE=7 }) } field; }; _Static_assert(sizeof(enum Inner)==4,\"size\");",Target::X86_64UnknownLinuxGnu).unwrap();
    let inner = enumeration(&unit, "Inner");
    assert!(matches!(
        unit.tag_discovery.as_ref().unwrap().enums[&inner],
        TagDiscovery::Hidden
    ));
    assert_eq!(unit.constants["VALUE"].as_u64().unwrap(), 7);
    assert_eq!(
        unit.layout(&Type::new(TypeKind::Enum(inner)))
            .unwrap()
            .size_bits,
        32
    );
    assert_eq!(
        unit.lexical_tags.enums[&inner].record,
        Some(record(&unit, "Parent"))
    );
}

#[test]
fn deferred_records_choose_the_first_visited_type_use() {
    for (tail, owner) in [
        (
            "struct Public { enum Inner field; }; struct Deferred object;",
            "Public",
        ),
        (
            "struct Deferred object; struct Public { enum Inner field; };",
            "Deferred",
        ),
    ] {
        let source = format!(
            "enum Outer {{ COUNT=sizeof(enum Inner {{ VALUE=1 }}) }}; enum Other {{ SIZE=sizeof(struct Deferred {{ enum Inner field; }}) }}; {tail}"
        );
        let unit = analyze(&source, Target::X86_64UnknownLinuxGnu).unwrap();
        let inner = enumeration(&unit, "Inner");
        let TagDiscovery::Discovered {
            record: Some(id),
            offset,
            ..
        } = unit.tag_discovery.as_ref().unwrap().enums[&inner]
        else {
            panic!()
        };
        assert_eq!(id, record(&unit, owner));
        assert!(offset < source.len());
        assert!(unit.enums[inner].complete);
    }
}

#[test]
fn forwards_and_real_uses_are_distinct_from_queries() {
    for source in [
        "enum Inner; enum Outer { COUNT=sizeof(enum Inner { VALUE=1 }) };",
        "enum Outer { COUNT=sizeof(enum Inner { VALUE=1 }) }; enum Inner;",
        "enum Outer { COUNT=sizeof(enum Inner { VALUE=1 }) }; typedef __typeof__((enum Inner)0) Alias;",
    ] {
        let unit = analyze(source, Target::X86_64UnknownLinuxGnu).unwrap();
        assert!(matches!(
            unit.tag_discovery.as_ref().unwrap().enums[&enumeration(&unit, "Inner")],
            TagDiscovery::Discovered { record: None, .. }
        ));
    }
    let unit = analyze(
        "enum Mode { VALUE=1 }; struct Record { int field[VALUE]; };",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    assert!(unit.tag_discovery.is_none());
}

#[test]
fn indirect_type_shapes_keep_discovery_separate_from_naming_context() {
    for (field, nested) in [
        ("enum Inner field;", true),
        ("enum Inner *field;", false),
        ("enum Inner field[2];", false),
        ("int (*field)(enum Inner);", false),
    ] {
        let unit=analyze(&format!("enum Outer {{ COUNT=sizeof(enum Inner {{ VALUE=1 }}) }}; struct Owner {{ {field} }};"),Target::X86_64UnknownLinuxGnu).unwrap();
        let facts = &unit.tag_discovery.as_ref().unwrap().enums[&enumeration(&unit, "Inner")];
        let TagDiscovery::Discovered { record, .. } = facts else {
            panic!()
        };
        assert_eq!(record.is_some(), nested, "{field}");
    }
}

#[test]
fn record_attributes_follow_record_discovery() {
    let unit = analyze("enum Outer { COUNT=sizeof(struct Record { int field; } __attribute__((aligned(sizeof(enum Inner { VALUE=1 }))))) }; struct Record object;", Target::X86_64UnknownLinuxGnu).unwrap();
    assert!(
        matches!(unit.tag_discovery.as_ref().unwrap().enums[&enumeration(&unit, "Inner")], TagDiscovery::Discovered { record: Some(id), .. } if id == record(&unit, "Record"))
    );
}

#[test]
fn anonymous_member_admission_keeps_following_enum_fields_in_order() {
    use toucan_semantic::{AnalysisOptions, analyze_with_profile};
    use toucan_target::{Compiler, CompilerProfile};

    for target in [Target::X86_64UnknownLinuxGnu, Target::X86_64PcWindowsMsvc] {
        let profile = CompilerProfile::new(target, Compiler::Clang).unwrap();
        for (member, direct) in [
            ("RecordAlias;", false),
            ("Later;", false),
            ("const _Atomic RecordAlias;", false),
            ("struct Named;", false),
            ("struct Nested {int nested;};", false),
            ("const _Atomic struct {int nested;};", true),
        ] {
            let source = format!(
                "enum Outer {{COUNT=sizeof(enum Inner {{VALUE=1}})}};\n\
                 typedef int Scalar;\n\
                 typedef struct Named {{int nested;}} RecordAlias;\n\
                 typedef RecordAlias Later;\n\
                 typedef RecordAlias *PointerAlias;\n\
                 typedef RecordAlias ArrayAlias[2];\n\
                 typedef _Atomic(RecordAlias) AtomicAlias;\n\
                 RecordAlias source;\n\
                 struct Owner {{Scalar; PointerAlias; ArrayAlias; AtomicAlias;\n\
                     const _Atomic AtomicAlias; __typeof__(source);\n\
                     {member} enum Inner field;}};"
            );
            for retain_code in [false, true] {
                let analysis = analyze_with_profile(
                    &source,
                    profile,
                    &AnalysisOptions {
                        retain_code,
                        ..Default::default()
                    },
                )
                .unwrap();
                let unit = analysis.unit();
                let owner = record(unit, "Owner");
                assert_eq!(
                    unit.records[owner].fields.as_ref().unwrap().len(),
                    if direct || target == Target::X86_64PcWindowsMsvc {
                        2
                    } else {
                        1
                    }
                );
                let TagDiscovery::Discovered {
                    record: Some(naming_record),
                    offset,
                    ..
                } = unit.tag_discovery.as_ref().unwrap().enums[&enumeration(unit, "Inner")]
                else {
                    panic!("{target:?}: {member}");
                };
                assert_eq!(naming_record, owner, "{target:?}: {member}");
                assert_eq!(
                    offset,
                    source.rfind("field").unwrap(),
                    "{target:?}: {member}"
                );
            }
        }
    }
}

#[test]
fn trailing_attribute_enums_have_cursor_owners_without_a_c_record_scope() {
    use toucan_semantic::{AnalysisOptions, Scope, analyze_with_profile};
    use toucan_target::{CompilerProfile, LanguageMode};

    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            for prefix in ["", "enum Outer {COUNT=sizeof(enum Hidden {HIDDEN=1})};"] {
                let source = format!(
                    "{prefix} struct Record {{char field;}} __attribute__((aligned(sizeof(enum Visible {{VALUE=7}}))));\n_Static_assert(sizeof(struct Record)==4,\"size\");\n_Static_assert(__alignof__(struct Record)==4,\"alignment\");\n_Static_assert(sizeof(enum Visible)==4 && VALUE==7,\"file scope\");"
                );
                let mut units = Vec::new();
                for retain_code in [false, true] {
                    let analysis = analyze_with_profile(
                        &source,
                        profile.with_language_mode(mode),
                        &AnalysisOptions {
                            retain_code,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    let unit = analysis.unit();
                    let visible = enumeration(unit, "Visible");
                    assert_eq!(unit.enums[visible].scope, Scope::File);
                    assert!(!unit.lexical_tags.enums.contains_key(&visible));
                    assert!(
                        matches!(unit.tag_discovery.as_ref().unwrap().enums[&visible],
                        TagDiscovery::Discovered {record: Some(id), ..} if id == record(unit, "Record"))
                    );
                    units.push(serde_json::to_value(unit).unwrap());
                }
                assert_eq!(units[0], units[1]);
            }
        }
    }
}

#[test]
fn record_attribute_placement_and_prior_declarations_control_discovery() {
    for source in [
        "struct __attribute__((aligned(sizeof(enum Visible {VALUE=1})))) Record {char field;};",
        "__attribute__((aligned(sizeof(enum Visible {VALUE=1})))) struct Record {char field;};",
        "enum Visible; struct Record {char field;} __attribute__((aligned(sizeof(enum Visible {VALUE=1}))));",
        "struct Record {char field;}; struct Record __attribute__((aligned(sizeof(enum Visible {VALUE=1}))));",
    ] {
        for prefix in ["", "enum Outer {COUNT=sizeof(enum Hidden {HIDDEN=1})};"] {
            let unit =
                analyze(&format!("{prefix}{source}"), Target::X86_64UnknownLinuxGnu).unwrap();
            let visible = enumeration(&unit, "Visible");
            assert!(!matches!(
                unit.tag_discovery
                    .as_ref()
                    .and_then(|facts| facts.enums.get(&visible)),
                Some(TagDiscovery::Discovered {
                    record: Some(_),
                    ..
                })
            ));
        }
    }
    let unit = analyze("enum Outer {COUNT=sizeof(struct __attribute__((aligned(sizeof(enum Visible {VALUE=1})))) Record {char field;})}; struct Record object;", Target::X86_64UnknownLinuxGnu).unwrap();
    assert!(matches!(
        unit.tag_discovery.as_ref().unwrap().enums[&enumeration(&unit, "Visible")],
        TagDiscovery::Hidden
    ));
    let unit = analyze(
        "struct Record {char field;} __attribute__((aligned(4)));",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    assert!(unit.tag_discovery.is_none());
}

#[test]
fn record_attribute_children_do_not_need_an_unrelated_enum_to_activate_discovery() {
    let unit = analyze(
        "struct Record {char field;} __attribute__((aligned(sizeof(struct Inner {int field;}))));",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    assert!(unit.enums.is_empty());
    assert!(
        matches!(unit.tag_discovery.as_ref().unwrap().records[&record(&unit,"Inner")],
        TagDiscovery::Discovered {record: Some(id), ..} if id == record(&unit,"Record"))
    );
}

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

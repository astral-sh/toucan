use toucan_semantic::{DeclarationKind, Scope, Type, TypeKind, analyze};
use toucan_target::Target;

fn record(unit: &toucan_semantic::TranslationUnit, name: &str) -> usize {
    unit.records
        .iter()
        .position(|record| record.name.as_deref() == Some(name))
        .unwrap()
}

#[test]
fn record_containment_does_not_introduce_a_c_tag_scope() {
    let unit = analyze("struct Outer { struct Middle { enum Inner { VALUE=1 } field; } middle; }; enum Inner outside; struct Middle other; enum Global { GLOBAL=2 };", Target::X86_64UnknownLinuxGnu).unwrap();
    let outer = record(&unit, "Outer");
    let middle = record(&unit, "Middle");
    let inner = unit
        .enums
        .iter()
        .position(|item| item.name.as_deref() == Some("Inner"))
        .unwrap();
    assert_eq!(unit.records[middle].scope, Scope::File);
    assert_eq!(unit.enums[inner].scope, Scope::File);
    assert_eq!(unit.lexical_tags.records[&middle].record, Some(outer));
    assert_eq!(unit.lexical_tags.enums[&inner].record, Some(middle));
    assert!(unit.lexical_tags.records[&middle].order < unit.lexical_tags.enums[&inner].order);
    assert!(!unit.lexical_tags.enums[&inner].prior_file_declaration);
    assert_eq!(
        unit.declarations
            .iter()
            .find(|item| item.name == "outside")
            .unwrap()
            .ty
            .kind,
        TypeKind::Enum(inner)
    );
    assert_eq!(
        unit.declarations
            .iter()
            .find(|item| item.name == "other")
            .unwrap()
            .ty
            .kind,
        TypeKind::Record(middle)
    );
    let mut without = unit.clone();
    without.lexical_tags = Default::default();
    for id in 0..unit.records.len() {
        assert_eq!(
            unit.layout(&Type::new(TypeKind::Record(id))).unwrap(),
            without.layout(&Type::new(TypeKind::Record(id))).unwrap()
        );
    }
    assert_eq!(unit.lexical_tags.enums.len(), 1);
}

#[test]
fn only_standalone_redeclarations_change_prior_file_visibility() {
    for (middle, prior) in [
        ("", false),
        ("enum Inner *pointer;", false),
        ("void function(enum Inner *);", false),
        ("typedef enum Inner Alias;", false),
        ("enum Inner;", true),
    ] {
        let source = format!(
            "struct First {{ enum Inner; }}; {middle} struct Second {{ enum Inner {{ VALUE=1 }} field; }};"
        );
        let unit = analyze(&source, Target::X86_64UnknownLinuxGnu).unwrap();
        assert_eq!(unit.enums.len(), 1);
        let origin = &unit.lexical_tags.enums[&0];
        assert_eq!(origin.record, Some(record(&unit, "Second")));
        assert_eq!(origin.prior_file_declaration, prior, "{middle}");
    }
    let unit = analyze(
        "enum Inner; struct Outer { enum Inner { VALUE=1 } field; };",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    assert!(unit.lexical_tags.enums[&0].prior_file_declaration);
    assert_eq!(
        unit.lexical_tags.enums[&0].record,
        Some(record(&unit, "Outer"))
    );
}

#[test]
fn direct_typedefs_are_distinct_from_later_typeof_aliases() {
    let unit = analyze("typedef struct { enum { VALUE=1 } field; } First, Second; typedef __typeof__(((First*)0)->field) Later; typedef struct { int field; } *Pointer;", Target::X86_64UnknownLinuxGnu).unwrap();
    let first = unit
        .declarations
        .iter()
        .position(|item| item.name == "First")
        .unwrap();
    assert_eq!(unit.declarations[first].kind, DeclarationKind::Typedef);
    let TypeKind::Record(id) = unit.resolve(&unit.declarations[first].ty).unwrap().kind else {
        panic!()
    };
    assert_eq!(
        unit.lexical_tags.records[&id].typedef_declaration,
        Some(first)
    );
    assert!(unit.lexical_tags.enums[&0].typedef_declaration.is_none());
    assert_eq!(
        unit.lexical_tags
            .records
            .values()
            .filter(|origin| origin.typedef_declaration.is_some())
            .count(),
        1
    );
    assert!(
        unit.lexical_tags
            .records
            .values()
            .any(|origin| origin.record.is_none() && origin.typedef_declaration.is_none())
    );
}

#[test]
fn ordinary_named_tags_need_no_sparse_origins() {
    let unit = analyze(
        "struct Named { int field; }; enum Mode { VALUE=1 }; typedef struct Named Alias;",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    assert!(unit.lexical_tags.is_empty());
}

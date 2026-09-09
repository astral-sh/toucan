use toucan_semantic::{AnalysisOptions, ArithmeticConstant, TypeKind, analyze_with_options};
use toucan_target::Target;

#[test]
fn capture_keeps_written_order_and_destination_values_without_code() {
    let source = "extern const int x; const int x=7; static signed char narrow=255; static const _Bool yes=4; static const float rounded=16777217; static const int empty={}; enum E{V=9}; static const enum E e=V;";
    let plain = analyze_with_options(
        source,
        Target::X86_64UnknownLinuxGnu,
        &AnalysisOptions::default(),
    )
    .unwrap();
    let captured = analyze_with_options(
        source,
        Target::X86_64UnknownLinuxGnu,
        &AnalysisOptions {
            retain_object_values: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(plain.object_values().is_none());
    assert!(captured.checked().is_none());
    assert!(captured.declaration_origins().is_none());
    assert_eq!(
        format!("{:?}", plain.unit()),
        format!("{:?}", captured.unit())
    );
    let values = captured.object_values().unwrap().entries();
    assert_eq!(values.len(), 7);
    assert_eq!(values[0].declaration(), values[1].declaration());
    assert_eq!(values[0].value(), None);
    let integer = |index: usize| match values[index].value().unwrap() {
        ArithmeticConstant::Integer(value) => value,
        _ => panic!(),
    };
    assert_eq!(integer(1).signed_value(), 7);
    assert_eq!(integer(2).signed_value(), -1);
    assert_eq!(integer(3).value, 1);
    assert_eq!(integer(3).rank, 0);
    assert!(
        matches!(values[4].value(), Some(ArithmeticConstant::Floating(value)) if value.to_bits() == u128::from(16777216f32.to_bits()))
    );
    assert_eq!(integer(5).value, 0);
    assert_eq!(values[6].value(), None);
    assert!(values[2].is_internal());
    assert!(!values[0].is_internal());
    for value in values {
        assert!(source[value.offset()..].starts_with(value.name()));
    }
}

#[test]
fn declarations_keep_bounds_scalar_types_and_literal_fallback_facts() {
    let source = "extern const char bytes[]; const char bytes[4]=\"abc\"; typedef unsigned long U; static U large=9223372036854775808UL; static U expr=9223372036854775808UL+1; _Thread_local const _Atomic(int) t=3;";
    let analysis = analyze_with_options(
        source,
        Target::X86_64UnknownLinuxGnu,
        &AnalysisOptions {
            retain_object_values: true,
            retain_code: true,
            retain_declaration_origins: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(analysis.checked().is_some());
    let values = analysis.object_values().unwrap().entries();
    assert!(matches!(
        values[0].ty().kind,
        TypeKind::Array { length: None, .. }
    ));
    assert!(matches!(
        values[1].ty().kind,
        TypeKind::Array {
            length: Some(4),
            ..
        }
    ));
    assert!(matches!(values[2].ty().kind, TypeKind::Typedef(_)));
    assert!(values[2].integer_literal_fallback());
    assert!(!values[3].integer_literal_fallback());
    assert!(values[4].is_thread_local());
    assert!(
        matches!(values[4].value(), Some(ArithmeticConstant::Integer(value)) if value.signed_value()==3)
    );
}

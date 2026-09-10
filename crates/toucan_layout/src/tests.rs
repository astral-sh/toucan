use crate::builder::compute_layout;
use crate::layout::{
    Annotation, Array, BuiltinType, Record, RecordField, RecordKind, Type, TypeLayout, TypeVariant,
};
use crate::result::ErrorType;
use crate::target::Target;

#[test]
fn type_nesting_is_bounded_before_layout() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            for target in [Target::X86_64UnknownLinuxGnu, Target::X86_64PcWindowsMsvc] {
                for kind in 0..3 {
                    for depth in [128, 129, 2048] {
                        let mut ty = Type {
                            layout: (),
                            annotations: vec![],
                            variant: TypeVariant::Builtin(BuiltinType::Int),
                        };
                        for _ in 1..depth {
                            let variant = match kind {
                                0 => TypeVariant::Typedef(Box::new(ty)),
                                1 => TypeVariant::Array(Array {
                                    element_type: Box::new(ty),
                                    num_elements: Some(1),
                                }),
                                _ => TypeVariant::Record(Record {
                                    kind: RecordKind::Struct,
                                    fields: vec![RecordField {
                                        layout: None,
                                        annotations: vec![],
                                        named: true,
                                        bit_width: None,
                                        ty,
                                    }],
                                }),
                            };
                            ty = Type {
                                layout: (),
                                annotations: vec![],
                                variant,
                            };
                        }
                        let result = compute_layout(target, &ty);
                        if depth == 128 {
                            assert_eq!(result.unwrap().layout.size_bits, 32);
                        } else {
                            assert!(matches!(result.unwrap_err().kind(), ErrorType::TypeNesting));
                        }
                        // The caller owns the input. Destroy its deliberately
                        // excessive nesting iteratively, outside layout computation.
                        loop {
                            ty = match ty.variant {
                                TypeVariant::Typedef(inner) => *inner,
                                TypeVariant::Array(array) => *array.element_type,
                                TypeVariant::Record(mut record) => record.fields.pop().unwrap().ty,
                                _ => break,
                            };
                        }
                    }
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn annotated_builtin() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![Annotation::AttrPacked],
        variant: TypeVariant::Builtin(BuiltinType::Int),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::AnnotatedBuiltinType));
}

#[test]
fn annotated_opaque() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![Annotation::AttrPacked],
        variant: TypeVariant::Opaque(TypeLayout {
            size_bits: 0,
            field_alignment_bits: 8,
            pointer_alignment_bits: 8,
            required_alignment_bits: 8,
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::AnnotatedOpaqueType));
}

#[test]
fn annotated_array() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![Annotation::AttrPacked],
        variant: TypeVariant::Array(Array {
            element_type: Box::new(Type {
                layout: (),
                annotations: vec![],
                variant: TypeVariant::Builtin(BuiltinType::Int),
            }),
            num_elements: None,
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::AnnotatedArray));
}

#[test]
fn size_overflow() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Array(Array {
            element_type: Box::new(Type {
                layout: (),
                annotations: vec![],
                variant: TypeVariant::Builtin(BuiltinType::Int),
            }),
            num_elements: Some(u64::MAX / 8 / 4 + 1),
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::SizeOverflow));
}

#[test]
fn power_of_two_alignment_1() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![Annotation::Align(Some(24))],
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Struct,
            fields: vec![],
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::PowerOfTwoAlignment));
}

#[test]
fn power_of_two_alignment_2() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Opaque(TypeLayout {
            size_bits: 0,
            field_alignment_bits: 8,
            pointer_alignment_bits: 24,
            required_alignment_bits: 8,
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::PowerOfTwoAlignment));
}

#[test]
fn power_of_two_alignment_3() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Opaque(TypeLayout {
            size_bits: 0,
            field_alignment_bits: 24,
            pointer_alignment_bits: 8,
            required_alignment_bits: 8,
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::PowerOfTwoAlignment));
}

#[test]
fn power_of_two_alignment_4() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Opaque(TypeLayout {
            size_bits: 0,
            field_alignment_bits: 8,
            pointer_alignment_bits: 8,
            required_alignment_bits: 24,
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::PowerOfTwoAlignment));
}

#[test]
fn sub_byte_alignment_1() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![Annotation::Align(Some(4))],
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Struct,
            fields: vec![],
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::SubByteAlignment));
}

#[test]
fn sub_byte_alignment_2() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Opaque(TypeLayout {
            size_bits: 0,
            field_alignment_bits: 8,
            pointer_alignment_bits: 4,
            required_alignment_bits: 8,
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::SubByteAlignment));
}

#[test]
fn sub_byte_alignment_3() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Opaque(TypeLayout {
            size_bits: 0,
            field_alignment_bits: 4,
            pointer_alignment_bits: 8,
            required_alignment_bits: 8,
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::SubByteAlignment));
}

#[test]
fn sub_byte_alignment_4() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Opaque(TypeLayout {
            size_bits: 0,
            field_alignment_bits: 8,
            pointer_alignment_bits: 8,
            required_alignment_bits: 4,
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::SubByteAlignment));
}

#[test]
fn sub_byte_size() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Opaque(TypeLayout {
            size_bits: 4,
            field_alignment_bits: 8,
            pointer_alignment_bits: 8,
            required_alignment_bits: 8,
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::SubByteSize));
}

#[test]
fn multiple_pragma_pack() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![Annotation::PragmaPack(8), Annotation::PragmaPack(8)],
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Struct,
            fields: vec![],
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(
        err.kind(),
        ErrorType::MultiplePragmaPackedAnnotations
    ));
}

#[test]
fn named_zero_sized_bit_field() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Struct,
            fields: vec![RecordField {
                layout: None,
                annotations: vec![],
                named: true,
                bit_width: Some(0),
                ty: Type {
                    layout: (),
                    annotations: vec![],
                    variant: TypeVariant::Builtin(BuiltinType::Int),
                },
            }],
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::NamedZeroSizeBitField));
}

#[test]
fn unnamed_regular_field() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Struct,
            fields: vec![RecordField {
                layout: None,
                annotations: vec![],
                named: false,
                bit_width: None,
                ty: Type {
                    layout: (),
                    annotations: vec![],
                    variant: TypeVariant::Builtin(BuiltinType::Int),
                },
            }],
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::UnnamedRegularField));
}

#[test]
fn oversized_bitfield() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Struct,
            fields: vec![RecordField {
                layout: None,
                annotations: vec![],
                named: true,
                bit_width: Some(64),
                ty: Type {
                    layout: (),
                    annotations: vec![],
                    variant: TypeVariant::Builtin(BuiltinType::Int),
                },
            }],
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::OversizedBitfield));
}

#[test]
fn bitfield_storage_boundary_overflow_returns_an_error() {
    let mut ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Struct,
            fields: vec![
                RecordField {
                    layout: None,
                    annotations: vec![],
                    named: true,
                    bit_width: None,
                    ty: Type {
                        layout: (),
                        annotations: vec![],
                        variant: TypeVariant::Builtin(BuiltinType::Char),
                    },
                },
                RecordField {
                    layout: None,
                    annotations: vec![],
                    named: false,
                    bit_width: Some(u64::MAX - 7),
                    ty: Type {
                        layout: (),
                        annotations: vec![],
                        variant: TypeVariant::Opaque(TypeLayout {
                            size_bits: u64::MAX - 7,
                            field_alignment_bits: 16,
                            pointer_alignment_bits: 8,
                            required_alignment_bits: 8,
                        }),
                    },
                },
            ],
        }),
    };
    for width in [u64::MAX - 7, u64::MAX - 15] {
        let TypeVariant::Record(record) = &mut ty.variant else {
            unreachable!();
        };
        record.fields[1].bit_width = Some(width);
        for compiler in [crate::Compiler::Gcc, crate::Compiler::Clang] {
            let result =
                crate::compute_layout_with_compiler(Target::X86_64UnknownLinuxGnu, compiler, &ty);
            if width == u64::MAX - 7 {
                assert!(matches!(
                    result.unwrap_err().kind(),
                    ErrorType::SizeOverflow
                ));
            } else {
                assert_eq!(result.unwrap().layout.size_bits, u64::MAX - 7);
            }
        }
    }
}

#[test]
fn pragma_packed_field() {
    let ty = Type::<()> {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Struct,
            fields: vec![RecordField {
                layout: None,
                annotations: vec![Annotation::PragmaPack(8)],
                named: true,
                bit_width: None,
                ty: Type {
                    layout: (),
                    annotations: vec![],
                    variant: TypeVariant::Builtin(BuiltinType::Int),
                },
            }],
        }),
    };
    let err = compute_layout(Target::X86_64UnknownLinuxGnu, &ty).unwrap_err();
    assert!(matches!(err.kind(), ErrorType::PragmaPackedField));
}

use toucan_target::{
    Annotation, BuiltinType as B, Field, LayoutError, Record, RecordKind, Target, Type, TypeVariant,
};

fn field(builtin: B) -> Field {
    Field {
        ty: Type::builtin(builtin),
        annotations: vec![],
        named: true,
        bit_width: None,
    }
}

fn record(fields: Vec<Field>) -> Type {
    Type {
        annotations: vec![],
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Struct,
            fields,
        }),
    }
}

#[test]
fn data_models_are_explicit() {
    for target in Target::ALL {
        assert_eq!(Target::parse(target.triple()).unwrap(), target);
        assert_eq!(
            target.builtin_layout(B::Pointer).unwrap().size_bits,
            target.pointer_width()
        );
        assert_eq!(
            target.builtin_layout(B::Long).unwrap().size_bits,
            target.long_width()
        );
        assert_eq!(target.builtin_layout(B::Int).unwrap().size_bytes(), 4);
        assert_eq!(target.builtin_layout(B::LongLong).unwrap().size_bytes(), 8);
        assert_eq!(
            target.char_is_signed(),
            target != Target::Aarch64UnknownLinuxGnu
        );
        let macros = target.predefined_macros();
        assert_eq!(macros["__STDC_VERSION__"], "201112L");
        assert_eq!(
            macros["__SIZEOF_LONG__"].parse::<u64>().unwrap() * 8,
            target.long_width()
        );
        assert_eq!(
            macros["__WCHAR_WIDTH__"].parse::<u64>().unwrap(),
            target.wchar_width()
        );
        assert_eq!(
            macros.contains_key("__WCHAR_UNSIGNED__"),
            !target.wchar_is_signed()
        );
        assert_eq!(
            macros["__SIZEOF_LONG_DOUBLE__"].parse::<u64>().unwrap(),
            target.builtin_layout(B::LongDouble).unwrap().size_bytes()
        );
    }
    assert!(matches!(
        Target::parse("x86_64-unknown-linux-musl"),
        Err(LayoutError::UnsupportedTarget(_))
    ));
}

#[test]
fn packing_changes_offsets_and_alignment() {
    let mut ty = record(vec![field(B::Char), field(B::Int), field(B::Double)]);
    for target in Target::ALL {
        let natural = target.layout(&ty).unwrap();
        assert_eq!((natural.size_bytes(), natural.alignment_bytes()), (16, 8));
        assert_eq!(natural.fields[1].unwrap().offset_bits, 32);
        ty.annotations = vec![Annotation::Packed];
        let packed = target.layout(&ty).unwrap();
        assert_eq!((packed.size_bytes(), packed.alignment_bytes()), (13, 1));
        assert_eq!(packed.fields[1].unwrap().offset_bits, 8);
        ty.annotations = vec![Annotation::PragmaPack(16)];
        let pragma = target.layout(&ty).unwrap();
        assert_eq!((pragma.size_bytes(), pragma.alignment_bytes()), (14, 2));
        assert_eq!(pragma.fields[1].unwrap().offset_bits, 16);
        ty.annotations.clear();
    }
}

#[test]
fn bitfields_preserve_offsets_and_zero_width_barriers() {
    let mut first = field(B::UnsignedInt);
    first.bit_width = Some(3);
    let mut second = field(B::UnsignedInt);
    second.bit_width = Some(5);
    let mut barrier = field(B::UnsignedInt);
    barrier.named = false;
    barrier.bit_width = Some(0);
    let ty = record(vec![first, second, barrier, field(B::Char)]);
    for target in Target::ALL {
        let layout = target.layout(&ty).unwrap();
        assert_eq!(layout.fields[0].unwrap().offset_bits, 0);
        assert_eq!(layout.fields[0].unwrap().size_bits, 3);
        assert_eq!(layout.fields[1].unwrap().offset_bits, 3);
        assert_eq!(layout.fields[1].unwrap().size_bits, 5);
        assert!(layout.fields[2].is_none());
        assert_eq!(layout.fields[3].unwrap().offset_bits, 32);
    }
}

#[test]
fn union_and_array_layouts() {
    let ty = Type {
        annotations: vec![],
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Union,
            fields: vec![field(B::Char), field(B::Double)],
        }),
    };
    for target in Target::ALL {
        let union = target.layout(&ty).unwrap();
        assert_eq!((union.size_bytes(), union.alignment_bytes()), (8, 8));
        assert!(
            union
                .fields
                .iter()
                .all(|field| field.unwrap().offset_bits == 0)
        );
        let array = Type {
            annotations: vec![],
            variant: TypeVariant::Array {
                element: Box::new(ty.clone()),
                length: Some(7),
            },
        };
        let layout = target.layout(&array).unwrap();
        assert_eq!((layout.size_bytes(), layout.alignment_bytes()), (56, 8));
    }
}

#[test]
fn invalid_input_returns_errors() {
    let target = Target::X86_64UnknownLinuxGnu;
    assert!(matches!(
        target.builtin_layout(B::Void),
        Err(LayoutError::VoidObject)
    ));
    let mut ty = record(vec![field(B::Int)]);
    ty.annotations = vec![Annotation::PragmaPack(3)];
    assert!(matches!(
        target.layout(&ty),
        Err(LayoutError::InvalidPragmaPack(3))
    ));
    ty.annotations = vec![Annotation::Align(Some(24))];
    assert!(matches!(target.layout(&ty), Err(LayoutError::Abi(_))));
    let mut invalid = field(B::Float);
    invalid.bit_width = Some(2);
    assert!(matches!(
        target.layout(&record(vec![invalid])),
        Err(LayoutError::NonIntegerBitfield)
    ));
    let mut invalid = field(B::Bool);
    invalid.bit_width = Some(2);
    assert!(matches!(
        target.layout(&record(vec![invalid])),
        Err(LayoutError::BooleanBitfieldWidth)
    ));
    let mut too_wide = field(B::UnsignedInt);
    too_wide.bit_width = Some(33);
    assert!(matches!(
        target.layout(&record(vec![too_wide])),
        Err(LayoutError::Abi(_))
    ));
    let array = Type {
        annotations: vec![],
        variant: TypeVariant::Array {
            element: Box::new(Type::builtin(B::LongLong)),
            length: Some(u64::MAX),
        },
    };
    assert!(matches!(target.layout(&array), Err(LayoutError::Abi(_))));
    assert!(matches!(
        Target::X86_64PcWindowsMsvc.builtin_layout(B::Int128),
        Err(LayoutError::UnsupportedBuiltin { .. })
    ));
}

#[test]
fn nesting_is_bounded() {
    let mut ty = Type::builtin(B::Int);
    for _ in 0..256 {
        ty = Type {
            annotations: vec![],
            variant: TypeVariant::Typedef(Box::new(ty)),
        };
    }
    assert!(matches!(
        Target::X86_64UnknownLinuxGnu.layout(&ty),
        Err(LayoutError::NestingLimit)
    ));
}

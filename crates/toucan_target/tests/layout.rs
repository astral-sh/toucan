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
            !matches!(
                target,
                Target::Aarch64UnknownLinuxGnu
                    | Target::Aarch64UnknownLinuxMusl
                    | Target::Armv7UnknownLinuxGnueabihf
            )
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
        Target::parse("riscv64gc-unknown-linux-musl"),
        Err(LayoutError::UnsupportedTarget(_))
    ));
    let i686 = Target::I686UnknownLinuxGnu;
    assert_eq!((i686.pointer_width(), i686.long_width()), (32, 32));
    assert_eq!(i686.builtin_layout(B::Double).unwrap().alignment_bytes(), 4);
    assert_eq!(
        (
            i686.builtin_layout(B::LongDouble).unwrap().size_bytes(),
            i686.builtin_layout(B::LongDouble)
                .unwrap()
                .alignment_bytes()
        ),
        (12, 4)
    );
    assert!(matches!(
        i686.layout(&record(vec![field(B::Int128)])),
        Err(LayoutError::UnsupportedBuiltin { .. })
    ));
    assert!(matches!(
        i686.layout(&Type {
            annotations: vec![],
            variant: TypeVariant::Enum(vec![-1, i128::from(u64::MAX)]),
        }),
        Err(LayoutError::UnsupportedEnumRange(_))
    ));
}

#[test]
fn armv7_hard_float_uses_clang_and_the_32_bit_aapcs_vfp_model() {
    use toucan_target::{Compiler, CompilerProfile};

    let target = Target::parse("armv7-unknown-linux-gnueabihf").unwrap();
    assert!(target.is_armv7() && target.is_linux());
    assert!(!target.is_aarch64() && !target.is_x86_64() && !target.is_windows());
    assert_eq!(
        CompilerProfile::default_for(target).compiler(),
        Compiler::Clang
    );
    assert!(CompilerProfile::new(target, Compiler::Gnu).is_err());
    assert_eq!((target.pointer_width(), target.long_width()), (32, 32));
    assert!(!target.char_is_signed() && !target.wchar_is_signed());
    assert_eq!(target.default_maximum_alignment(), 8);
    assert_eq!(
        (
            target.builtin_layout(B::LongDouble).unwrap().size_bytes(),
            target
                .builtin_layout(B::LongDouble)
                .unwrap()
                .alignment_bytes(),
        ),
        (8, 8),
    );
    assert!(matches!(
        target.builtin_layout(B::Int128),
        Err(LayoutError::UnsupportedBuiltin { .. })
    ));
    let macros = target.predefined_macros();
    for (name, value) in [
        ("__ARM_PCS_VFP", "1"),
        ("__ARM_ARCH", "7"),
        ("__ILP32__", "1"),
        ("__SIZEOF_POINTER__", "4"),
        ("__SIZEOF_LONG_DOUBLE__", "8"),
        ("__BIGGEST_ALIGNMENT__", "8"),
        ("__WCHAR_TYPE__", "unsigned int"),
        ("__INT64_TYPE__", "long long int"),
    ] {
        assert_eq!(&macros[name], value, "{name}");
    }
    assert!(!macros.contains_key("__SIZEOF_INT128__"));
}

#[test]
fn arm64_windows_model_has_its_own_predefines_and_alignment_rules() {
    let target = Target::parse("aarch64-pc-windows-msvc").unwrap();
    let macros = target.predefined_macros();
    assert_eq!(macros["_M_ARM64"], "1");
    assert_eq!(macros["__SIZEOF_INT128__"], "16");
    assert_eq!(macros["_WIN64"], "1");
    assert!(!macros.contains_key("_M_X64"));
    assert!(!macros.contains_key("_M_AMD64"));
    assert!(!macros.contains_key("__LP64__"));
    assert_eq!((target.long_width(), target.wchar_width()), (32, 16));
    assert!(target.is_aarch64() && target.is_windows() && !target.is_x86_64());
    let mut ty = record(vec![field(B::Char), field(B::Int128)]);
    let natural = target.layout(&ty).unwrap();
    assert_eq!((natural.size_bytes(), natural.alignment_bytes()), (32, 16));
    assert_eq!(natural.fields[1].unwrap().offset_bits, 128);
    ty.annotations.push(Annotation::PragmaPack(64));
    let packed = target.layout(&ty).unwrap();
    assert_eq!((packed.size_bytes(), packed.alignment_bytes()), (24, 8));
    assert_eq!(packed.fields[1].unwrap().offset_bits, 64);
}

#[test]
fn packing_changes_offsets_and_alignment() {
    let mut ty = record(vec![field(B::Char), field(B::Int), field(B::Double)]);
    for target in Target::ALL {
        let natural = target.layout(&ty).unwrap();
        assert_eq!(
            (natural.size_bytes(), natural.alignment_bytes()),
            (
                16,
                if target == Target::I686UnknownLinuxGnu {
                    4
                } else {
                    8
                }
            )
        );
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
        let expected_alignment = if target == Target::I686UnknownLinuxGnu {
            4
        } else {
            8
        };
        assert_eq!(
            (union.size_bytes(), union.alignment_bytes()),
            (8, expected_alignment)
        );
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
        assert_eq!(
            (layout.size_bytes(), layout.alignment_bytes()),
            (56, expected_alignment)
        );
    }
}

#[test]
fn opaque_children_preserve_packing_and_required_alignment() {
    for target in Target::ALL {
        for inner_annotations in [
            vec![],
            vec![Annotation::Packed],
            vec![Annotation::Align(Some(128))],
        ] {
            let mut inner = record(vec![field(B::Char), field(B::Int)]);
            inner.annotations = inner_annotations;
            let opaque = Type::opaque_layout(&target.layout(&inner).unwrap());
            for outer_annotations in [
                vec![],
                vec![Annotation::Packed],
                vec![Annotation::PragmaPack(8)],
            ] {
                let parent = |child| {
                    let mut ty = record(vec![
                        field(B::Char),
                        Field {
                            ty: Type {
                                annotations: vec![],
                                variant: TypeVariant::Array {
                                    element: Box::new(child),
                                    length: Some(2),
                                },
                            },
                            annotations: vec![],
                            named: true,
                            bit_width: None,
                        },
                        field(B::Char),
                    ]);
                    ty.annotations = outer_annotations.clone();
                    ty
                };
                assert_eq!(
                    target.layout(&parent(inner.clone())).unwrap(),
                    target.layout(&parent(opaque.clone())).unwrap()
                );
            }
        }
    }
}

#[test]
fn enum_layouts_cover_signed_and_unsigned_boundaries() {
    for target in Target::ALL
        .into_iter()
        .filter(|target| !target.is_windows())
    {
        for (minimum, maximum, packed, bits) in [
            (0, i128::from(u32::MAX), false, 32),
            (-1, i128::from(i32::MAX), false, 32),
            (-1, i128::from(i32::MAX) + 1, false, 64),
            (-1, i128::from(u32::MAX), false, 64),
            (i128::from(i32::MIN) - 1, 0, false, 64),
            (-1, i128::from(i64::MAX), false, 64),
            (0, i128::from(u64::MAX), false, 64),
            (-128, 127, true, 8),
            (-1, 128, true, 16),
            (-1, 255, true, 16),
            (-1, 32767, true, 16),
            (-1, 32768, true, 32),
            (-1, 65535, true, 32),
            (0, 255, true, 8),
        ] {
            let ty = Type {
                annotations: if packed {
                    vec![Annotation::Packed]
                } else {
                    vec![]
                },
                variant: TypeVariant::Enum(vec![minimum, maximum]),
            };
            let layout = target.layout(&ty).unwrap();
            assert_eq!(
                (layout.size_bits, layout.alignment_bits),
                (
                    bits,
                    if bits == 64 && target == Target::I686UnknownLinuxGnu {
                        32
                    } else {
                        bits
                    }
                ),
                "{target}: {minimum}..={maximum}, packed={packed}"
            );
        }
    }
}

#[test]
fn wide_enum_layouts_follow_compiler_profiles() {
    for values in [
        vec![-1, i128::from(u64::MAX)],
        vec![0, 1_i128 << 100],
        vec![i128::MIN, 0],
        vec![i128::MIN, i128::MAX],
    ] {
        let ty = Type {
            annotations: vec![],
            variant: TypeVariant::Enum(values),
        };
        for target in [
            Target::X86_64UnknownLinuxGnu,
            Target::Aarch64UnknownLinuxGnu,
        ] {
            let layout = target.layout(&ty).unwrap();
            assert_eq!((layout.size_bits, layout.alignment_bits), (128, 128));
        }
        for target in [Target::X86_64AppleDarwin, Target::Aarch64AppleDarwin] {
            assert!(matches!(
                target.layout(&ty),
                Err(LayoutError::UnsupportedEnumRange(_))
            ));
        }
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

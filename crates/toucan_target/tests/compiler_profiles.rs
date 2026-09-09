use toucan_target::{
    Annotation, BuiltinType, Compiler, CompilerProfile, Field, Record, RecordKind, Target, Type,
    TypeVariant,
};

#[test]
fn attributed_enums_use_the_compiler_through_nested_layouts() {
    let ty = Type {
        annotations: vec![],
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Struct,
            fields: vec![
                Field {
                    named: true,
                    bit_width: None,
                    annotations: vec![],
                    ty: Type::builtin(BuiltinType::Char),
                },
                Field {
                    named: true,
                    bit_width: None,
                    annotations: vec![],
                    ty: Type {
                        annotations: vec![Annotation::Align(Some(128))],
                        variant: TypeVariant::Enum(vec![0]),
                    },
                },
                Field {
                    named: true,
                    bit_width: None,
                    annotations: vec![],
                    ty: Type::builtin(BuiltinType::Char),
                },
            ],
        }),
    };
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        for compiler in [Compiler::Gnu, Compiler::Clang] {
            let profile = CompilerProfile::new(target, compiler).unwrap();
            let layout = profile.layout(&ty).unwrap();
            assert_eq!(
                (layout.size_bytes(), layout.alignment_bytes()),
                if compiler == Compiler::Clang {
                    (32, 16)
                } else {
                    (12, 4)
                }
            );
            let wide = Type {
                annotations: vec![],
                variant: TypeVariant::Enum(vec![-1, i128::from(u64::MAX)]),
            };
            assert_eq!(profile.layout(&wide).is_ok(), compiler == Compiler::Gnu);
            let nested = Type {
                annotations: vec![],
                variant: TypeVariant::Array {
                    element: Box::new(wide),
                    length: Some(2),
                },
            };
            assert_eq!(profile.layout(&nested).is_ok(), compiler == Compiler::Gnu);
        }
    }
}

use toucan_layout::layout::{
    Annotation, Array, BuiltinType, Record, RecordField, RecordKind, Type, TypeLayout, TypeVariant,
};
use toucan_layout::{compute_layout, compute_layout_with_compiler, Compiler, ErrorType, Target};

fn builtin(kind: BuiltinType) -> Type<()> {
    Type {
        layout: (),
        annotations: vec![],
        variant: TypeVariant::Builtin(kind),
    }
}
fn field(ty: Type<()>, width: Option<u64>, annotations: Vec<Annotation>) -> RecordField<()> {
    RecordField {
        layout: None,
        annotations,
        named: true,
        bit_width: width,
        ty,
    }
}
fn record(fields: Vec<RecordField<()>>, annotations: Vec<Annotation>) -> Type<()> {
    Type {
        layout: (),
        annotations,
        variant: TypeVariant::Record(Record {
            kind: RecordKind::Struct,
            fields,
        }),
    }
}
fn cases() -> Vec<(&'static str, Type<()>)> {
    let e = Type {
        layout: (),
        annotations: vec![Annotation::Align(Some(128))],
        variant: TypeVariant::Enum(vec![0]),
    };
    let enumeration = record(
        vec![
            field(builtin(BuiltinType::Char), None, vec![]),
            field(e, None, vec![]),
            field(builtin(BuiltinType::Char), None, vec![]),
        ],
        vec![],
    );
    let aligned = Type {
        layout: (),
        annotations: vec![Annotation::Align(Some(64))],
        variant: TypeVariant::Typedef(Box::new(builtin(BuiltinType::UnsignedChar))),
    };
    let bitfield = record(
        vec![
            field(builtin(BuiltinType::UnsignedChar), Some(1), vec![]),
            field(aligned, Some(1), vec![]),
            field(builtin(BuiltinType::Char), None, vec![]),
        ],
        vec![],
    );
    let packed = record(
        vec![
            field(builtin(BuiltinType::UnsignedChar), Some(1), vec![]),
            field(
                builtin(BuiltinType::UnsignedInt),
                Some(1),
                vec![Annotation::Align(Some(32))],
            ),
            field(builtin(BuiltinType::Char), None, vec![]),
        ],
        vec![Annotation::PragmaPack(16)],
    );
    let nested = record(
        vec![
            field(builtin(BuiltinType::Char), None, vec![]),
            field(
                Type {
                    layout: (),
                    annotations: vec![],
                    variant: TypeVariant::Array(Array {
                        element_type: Box::new(Type {
                            layout: (),
                            annotations: vec![],
                            variant: TypeVariant::Typedef(Box::new(enumeration.clone())),
                        }),
                        num_elements: Some(2),
                    }),
                },
                None,
                vec![],
            ),
            field(builtin(BuiltinType::Char), None, vec![]),
        ],
        vec![],
    );
    vec![("enum __attribute__((aligned(16))) E{X}; struct S{char a;enum E b;char c;};",enumeration),("typedef unsigned char C __attribute__((aligned(8))); struct S{unsigned char a:1;C b:1;char c;};",bitfield),("#pragma pack(push,2)\nstruct S{unsigned char a:1;unsigned int b:1 __attribute__((aligned(4)));char c;};\n#pragma pack(pop)",packed),("enum __attribute__((aligned(16))) E{X}; typedef struct{char a;enum E b;char c;} ER; struct S{char a;ER b[2];char c;};",nested)]
}
fn fields(ty: &Type<TypeLayout>) -> &[RecordField<TypeLayout>] {
    match &ty.variant {
        TypeVariant::Record(r) => &r.fields,
        _ => panic!("record"),
    }
}

#[test]
fn compiler_selection_propagates_through_fields_typedefs_and_arrays() {
    let expected = [
        [(12, 4, 8), (16, 8, 9), (4, 2, 3), (32, 4, 28)],
        [(32, 16, 20), (8, 8, 1), (2, 2, 1), (96, 16, 80)],
    ];
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        for (compiler, expected) in [Compiler::Gcc, Compiler::Clang].iter().zip(expected) {
            for ((_, ty), (size, align, tail)) in cases().iter().zip(expected) {
                let result = compute_layout_with_compiler(target, *compiler, ty).unwrap();
                assert_eq!(
                    (
                        result.layout.size_bits / 8,
                        result.layout.pointer_alignment_bits / 8,
                        fields(&result)[2].layout.unwrap().offset_bits / 8
                    ),
                    (size, align, tail)
                );
                if *compiler == Compiler::Gcc {
                    assert_eq!(result, compute_layout(target, ty).unwrap());
                }
            }
        }
        for (index, gcc_bit, clang_bit) in [(1, 64, 1), (2, 16, 1)] {
            let (_, ty) = &cases()[index];
            for (compiler, bit) in [(Compiler::Gcc, gcc_bit), (Compiler::Clang, clang_bit)] {
                let result = compute_layout_with_compiler(target, compiler, ty).unwrap();
                assert_eq!(fields(&result)[1].layout.unwrap().offset_bits, bit);
            }
        }
    }
}

#[test]
fn unvalidated_compiler_overrides_are_errors() {
    let ty = builtin(BuiltinType::Int);
    for &target in toucan_layout::TARGETS {
        for compiler in [Compiler::Gcc, Compiler::Clang, Compiler::Msvc] {
            let supported = compiler == toucan_layout::system_compiler(target)
                || compiler == Compiler::Clang
                    && matches!(
                        target,
                        Target::X86_64UnknownLinuxGnu
                            | Target::X86_64UnknownLinuxMusl
                            | Target::Aarch64UnknownLinuxGnu
                            | Target::Aarch64UnknownLinuxMusl
                    );
            let result = compute_layout_with_compiler(target, compiler, &ty);
            assert_eq!(result.is_ok(), supported, "{} {compiler:?}", target.name());
            if let Err(error) = result {
                assert!(matches!(error.kind(), ErrorType::UnsupportedCompiler));
            }
        }
    }
}

fn oracle(command: &mut std::process::Command, source: &str) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("compiler must be installed");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn assertions(source: &str, ty: &Type<TypeLayout>) -> String {
    format!("{source}\n_Static_assert(sizeof(struct S)=={},\"size\");\n_Static_assert(_Alignof(struct S)=={},\"alignment\");\n_Static_assert(__builtin_offsetof(struct S,c)=={},\"tail\");\n",ty.layout.size_bits/8,ty.layout.pointer_alignment_bits/8,fields(ty)[2].layout.unwrap().offset_bits/8)
}

#[test]
#[ignore = "requires native GNU GCC and Clang Linux backends; run with --include-ignored"]
fn selected_layout_matches_compilers_on_the_same_linux_target() {
    use std::process::Command;
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        identity.status.success()
            && !String::from_utf8_lossy(&identity.stdout)
                .to_lowercase()
                .contains("clang"),
        "TOUCAN_GCC must identify GNU GCC"
    );
    // Cross-Clang checks retain the Linux physical ABI on both architectures.
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        for (source, ty) in cases() {
            let expected = compute_layout_with_compiler(target, Compiler::Clang, &ty).unwrap();
            let output = oracle(
                Command::new("clang").args([
                    "-target",
                    target.name(),
                    "-std=gnu11",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
                ]),
                &assertions(source, &expected),
            );
            assert!(
                output.status.success(),
                "{}: {}",
                target.name(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    // Same-target execution validates bit positions as well as size/alignment.
    // Other hosts still run the cross-Linux Clang checks above.
    let native = if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        Some(Target::X86_64UnknownLinuxGnu)
    } else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
        Some(Target::Aarch64UnknownLinuxGnu)
    } else {
        None
    };
    if let Some(target) = native {
        let dir = tempfile::tempdir().unwrap();
        for (compiler, cc) in [(Compiler::Gcc, gcc.as_str()), (Compiler::Clang, "clang")] {
            for (index, (source, ty)) in cases().into_iter().enumerate() {
                let expected = compute_layout_with_compiler(target, compiler, &ty).unwrap();
                let mut source = assertions(source, &expected);
                if [1, 2].contains(&index) {
                    let bit = fields(&expected)[1].layout.unwrap().offset_bits;
                    source.push_str(&format!("int main(void){{struct S s;unsigned char*p=(unsigned char*)&s;for(unsigned i=0;i<sizeof s;i++)p[i]=0;s.b=1;for(unsigned i=0;i<sizeof s;i++)if(p[i]!=(i=={}?{}:0))return 1;return 0;}}",bit/8,1u64<<(bit%8)));
                } else {
                    source.push_str("int main(void){return 0;}");
                }
                let exe = dir.path().join(format!("{compiler:?}-{index}"));
                let output = oracle(
                    Command::new(cc)
                        .args(["-std=gnu11", "-x", "c", "-"])
                        .arg("-o")
                        .arg(&exe),
                    &source,
                );
                assert!(
                    output.status.success(),
                    "{cc}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    Command::new(&exe).status().unwrap().success(),
                    "{} case {}",
                    cc,
                    index
                );
            }
        }
    }
}

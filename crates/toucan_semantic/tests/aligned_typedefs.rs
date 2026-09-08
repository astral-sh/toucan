use toucan_semantic::{AnalysisOptions, Type, TypeKind, analyze, analyze_with_options};
use toucan_target::Target;

const SOURCE: &str = r#"
    typedef int Maximum __attribute__((aligned));
    typedef int I1 __attribute__((aligned(1)));
    typedef int I16 __attribute__((aligned(16)));
    typedef I16 I2 __attribute__((aligned(2)));
    typedef I1 Chain;
    typedef int Arr[3] __attribute__((aligned(16)));
    struct S { char c; int n; };
    typedef struct S S1 __attribute__((aligned(1)));
    struct F1 { char c; I1 x; char z; };
    struct F16 { char c; I16 x; char z; };
    struct FS1 { char c; S1 x; char z; };
    typedef I1 *P16 __attribute__((aligned(16)));
    struct Mixed { S1 lowered; char gap; struct S natural; };
    #pragma pack(push, 1)
    struct Packed16 { char c; I16 x; char z; };
    struct Packed1 { char c; I1 x; char z; };
    #pragma pack(pop)
"#;

#[test]
fn typedef_layouts_and_c_alignment_queries_preserve_record_identity() {
    for target in Target::ALL {
        let unit = analyze(SOURCE, target).unwrap();
        for (name, size, alignment) in [
            ("Maximum", 4, 16),
            ("I1", 4, 1),
            ("I16", 4, 16),
            ("I2", 4, 2),
            ("Chain", 4, 1),
            ("Arr", 12, 16),
            ("S1", 8, 1),
            ("P16", 8, 16),
        ] {
            let ty = Type::new(TypeKind::Typedef(name.into()));
            let layout = unit.layout(&ty).unwrap();
            assert_eq!(layout.size_bytes(), size, "{target:?} {name}");
            assert_eq!(unit.alignment(&ty).unwrap(), alignment, "{target:?} {name}");
        }
        for (name, size, align, x, z) in if target == Target::X86_64PcWindowsMsvc {
            [
                ("S", 8, 4, 4, 4),
                ("F1", 12, 4, 4, 8),
                ("F16", 32, 16, 16, 20),
                ("FS1", 16, 4, 4, 12),
                ("Mixed", 20, 4, 8, 12),
                ("Packed16", 32, 16, 16, 20),
                ("Packed1", 6, 1, 1, 5),
            ]
        } else {
            [
                ("S", 8, 4, 4, 4),
                ("F1", 6, 1, 1, 5),
                ("F16", 32, 16, 16, 20),
                ("FS1", 10, 1, 1, 9),
                ("Mixed", 20, 4, 8, 12),
                ("Packed16", 6, 1, 1, 5),
                ("Packed1", 6, 1, 1, 5),
            ]
        } {
            let id = unit
                .records
                .iter()
                .position(|r| r.name.as_deref() == Some(name))
                .unwrap();
            let layout = unit.layout(&Type::new(TypeKind::Record(id))).unwrap();
            assert_eq!(
                (layout.size_bytes(), layout.alignment_bytes()),
                (size, align),
                "{target:?} {name}"
            );
            assert_eq!(layout.fields[1].as_ref().unwrap().offset_bits / 8, x);
            assert_eq!(
                layout.fields.last().unwrap().as_ref().unwrap().offset_bits / 8,
                z
            );
        }
        let alias_layout = unit
            .layout(&Type::new(TypeKind::Typedef("S1".into())))
            .unwrap();
        assert_eq!(alias_layout.fields.len(), 2);
        assert_eq!(alias_layout.fields[1].as_ref().unwrap().offset_bits, 32);
    }
}

#[test]
fn typedef_arrays_redeclarations_and_block_scopes_keep_their_alignment() {
    let source = r#"
      typedef int A __attribute__((aligned(16)));
      typedef int A __attribute__((aligned(1)));
      typedef A Small __attribute__((aligned(1)));
      typedef int B __attribute__((aligned(1)));
      typedef int B __attribute__((aligned(16)));
      enum { A_ALIGN=_Alignof(A), B_ALIGN=_Alignof(B), SMALL_ARRAY=sizeof(Small[3]), SMALL_ALIGN=_Alignof(Small[3]) };
      int f(int n) {
        typedef int Local[n] __attribute__((aligned(32)));
        typedef int A __attribute__((aligned(2)));
        _Static_assert(_Alignof(Local)==32,"VLA alias alignment");
        _Static_assert(_Alignof(A)==2,"local alias alignment");
        A a; typeof(a) b; _Static_assert(_Alignof(typeof(b))==2,"typeof alignment");
        return 0;
      }
    "#;
    for target in Target::ALL {
        let plain = analyze(source, target).unwrap();
        assert_eq!(plain.constants["A_ALIGN"].value, 16);
        assert_eq!(plain.constants["B_ALIGN"].value, 16);
        assert_eq!(plain.constants["SMALL_ARRAY"].value, 12);
        assert_eq!(plain.constants["SMALL_ALIGN"].value, 1);
        let retained = analyze_with_options(
            source,
            target,
            &AnalysisOptions {
                retain_code: true,
                ..AnalysisOptions::default()
            },
        )
        .unwrap();
        assert_eq!(format!("{plain:?}"), format!("{:?}", retained.unit()));
    }
}

#[test]
fn invalid_array_stride_and_c11_typedef_alignment_fail_explicitly() {
    for source in [
        "typedef int A __attribute__((aligned(16))); A array[2];",
        "typedef int A[3] __attribute__((aligned(16))); A array[2];",
        "_Alignas(8) typedef int A;",
        "int f(void){ _Alignas(8) typedef int A; return 0; }",
    ] {
        for target in Target::ALL {
            let error = analyze(source, target).unwrap_err();
            assert!(error.message.contains("alignment"), "{source}: {error}");
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang plus Clang cross-target layout support"]
fn typedef_layouts_match_native_and_cross_target_compilers() {
    let expressions = [
        "sizeof(struct Packed16)",
        "_Alignof(struct Packed16)",
        "__builtin_offsetof(struct Packed16,x)",
        "__builtin_offsetof(struct Packed16,z)",
        "sizeof(struct Packed1)",
        "_Alignof(struct Packed1)",
        "__builtin_offsetof(struct Packed1,x)",
        "__builtin_offsetof(struct Packed1,z)",
        "_Alignof(Maximum)",
        "sizeof(I1)",
        "_Alignof(I1)",
        "sizeof(I16)",
        "_Alignof(I16)",
        "sizeof(I2)",
        "_Alignof(I2)",
        "sizeof(Chain)",
        "_Alignof(Chain)",
        "sizeof(Arr)",
        "_Alignof(Arr)",
        "sizeof(struct S)",
        "_Alignof(struct S)",
        "sizeof(S1)",
        "_Alignof(S1)",
        "sizeof(struct F1)",
        "_Alignof(struct F1)",
        "__builtin_offsetof(struct F1,x)",
        "__builtin_offsetof(struct F1,z)",
        "sizeof(struct F16)",
        "_Alignof(struct F16)",
        "__builtin_offsetof(struct F16,x)",
        "__builtin_offsetof(struct F16,z)",
        "sizeof(struct FS1)",
        "_Alignof(struct FS1)",
        "__builtin_offsetof(struct FS1,x)",
        "__builtin_offsetof(struct FS1,z)",
        "sizeof(P16)",
        "_Alignof(P16)",
        "sizeof(I1[3])",
        "_Alignof(I1[3])",
    ];
    let directory = tempfile::tempdir().unwrap();
    for target in Target::ALL {
        let mut source = String::from(SOURCE);
        source.push_str("enum {");
        for (i, expression) in expressions.iter().enumerate() {
            source.push_str(&format!("M{i}={expression},"));
        }
        source.push_str("};");
        let unit = analyze(&source, target).unwrap();
        for (i, expression) in expressions.iter().enumerate() {
            source.push_str(&format!(
                "_Static_assert(({expression})=={},\"M{i}\");",
                unit.constants[&format!("M{i}")].value
            ));
        }
        let input = directory.path().join("alignment.c");
        std::fs::write(&input, &source).unwrap();
        let output = std::process::Command::new("clang")
            .args([
                "-target",
                target.triple(),
                "-std=gnu11",
                "-Werror",
                "-fsyntax-only",
            ])
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{target:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if target == Target::X86_64UnknownLinuxGnu
            && cfg!(all(target_arch = "x86_64", target_os = "linux"))
        {
            let compiler = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
            let output = std::process::Command::new(&compiler)
                .args(["-std=gnu11", "-Werror", "-fsyntax-only"])
                .arg(&input)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn unsupported_arithmetic_alignment_is_not_silently_erased() {
    for expression in ["a+a", "a+1", "1+a", "+a", "~a", "a<<1", "a&1", "1?a:a"] {
        let source = format!(
            "typedef int A __attribute__((aligned(1))); A a; enum {{ ALIGN = _Alignof(typeof({expression})) }};"
        );
        assert!(
            analyze(&source, Target::X86_64UnknownLinuxGnu)
                .unwrap_err()
                .message
                .contains("arithmetic result alignment")
        );
    }
    analyze("typedef unsigned short A __attribute__((aligned(1))); A a; enum { ALIGN = _Alignof(typeof(+a)) }; _Static_assert(ALIGN==4,\"promotion\");",Target::X86_64UnknownLinuxGnu).unwrap();
}

#[test]
fn composite_pointer_alias_alignment_is_not_silently_selected() {
    let source = "typedef int A __attribute__((aligned(1))); A *a; int *b; enum { ALIGN=_Alignof(typeof(*(1?a:b))) };";
    assert!(
        analyze(source, Target::X86_64UnknownLinuxGnu)
            .unwrap_err()
            .message
            .contains("composite pointer alignment")
    );
}

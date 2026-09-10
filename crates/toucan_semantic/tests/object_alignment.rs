use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::{Analysis, AnalysisOptions, Error, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile};

fn parity(source: &str, profile: CompilerProfile) -> Result<Analysis, Error> {
    let plain = analyze_with_profile(source, profile, &AnalysisOptions::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&plain, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => {
            assert_eq!(a.offset, b.offset);
            assert_eq!(a.message, b.message);
        }
        _ => panic!("{profile:?}: plain={plain:?}, retained={retained:?}"),
    }
    retained
}

const CASES: &[(&str, bool, bool)] = &[
    ("struct S{_Alignas(16) int x;};", true, true),
    ("struct S{_Alignas(1) int x;};", false, false),
    (
        "struct S{_Alignas(1) int x __attribute__((aligned(16)));};",
        false,
        true,
    ),
    (
        "struct S{_Alignas(0) int x __attribute__((aligned(1)));};",
        true,
        false,
    ),
    ("struct S{_Alignas(16) unsigned x:3;};", false, false),
    ("struct S{_Alignas(0) unsigned x:3;};", false, false),
    (
        "struct S{unsigned named:3 __attribute__((mode(QI)));};",
        true,
        true,
    ),
    (
        "struct S{unsigned named:33 __attribute__((mode(DI)));};",
        false,
        false,
    ),
    (
        "struct S{unsigned (__attribute__((vector_size(16))) named):3;};",
        false,
        false,
    ),
    (
        "struct S{unsigned named:sizeof(enum{B=8}) __attribute__((aligned(B)));};",
        true,
        true,
    ),
    (
        "struct S{unsigned (__attribute__((aligned(sizeof(enum{A=4})))) named):sizeof(enum{B=A*2}) __attribute__((aligned(B)));};",
        true,
        true,
    ),
    (
        "struct S{unsigned :3 __attribute__((mode(QI)));};",
        true,
        true,
    ),
    (
        "struct S{unsigned :9 __attribute__((mode(BOGUS)));};",
        false,
        false,
    ),
    (
        "struct S{unsigned :33 __attribute__((mode(DI)));};",
        false,
        false,
    ),
    (
        "struct S{unsigned :9 __attribute__((vector_size(3)));};",
        false,
        false,
    ),
    (
        "struct S{unsigned :9 __attribute__((aligned(3)));};",
        false,
        false,
    ),
    (
        "struct S{unsigned :9 __attribute__((packed(1)));};",
        false,
        false,
    ),
    ("struct S{_Alignas(16) struct{int x;};};", true, true),
    ("int x=sizeof(_Alignas(16) int);", false, false),
    ("_Alignas(1) extern int x[];", false, true),
    ("struct S{int n;_Alignas(1) int x[];};", false, true),
    ("int x __attribute__((aligned(1)));", true, true),
    ("_Alignas(16) int x;", true, true),
    ("_Alignas(0) int x;", true, true),
    (
        "_Alignas(0) int x __attribute__((aligned(1)));",
        true,
        false,
    ),
    ("_Alignas(0) extern int x; int x;", true, false),
    ("_Alignas(1) int x;", false, false),
    ("_Alignas(3) int x;", false, false),
    ("_Alignas(16) void f(void);", false, false),
    ("void f(void){_Alignas(16) register int x;}", false, false),
    (
        "void f(void){register int x __attribute__((aligned(16)));}",
        true,
        true,
    ),
    ("int f(int x __attribute__((aligned(32))));", false, true),
    ("int f(_Alignas(32) int x);", false, false),
    ("_Alignas(16) typedef int T;", false, false),
    (
        "_Alignas(1) int x __attribute__((aligned(16)));",
        false,
        true,
    ),
    (
        "_Alignas(16) int x __attribute__((aligned(1)));",
        true,
        true,
    ),
    (
        "_Alignas(8) extern int x; _Alignas(16) extern int x;",
        true,
        false,
    ),
    ("_Alignas(8) extern int x; extern int x;", true, true),
    ("_Alignas(8) extern int x; int x;", true, false),
    ("int x; _Alignas(16) extern int x;", true, true),
    ("int x=0; _Alignas(16) extern int x;", true, false),
    (
        "_Alignas(8) extern int x; _Alignas(0) extern int x;",
        true,
        false,
    ),
    (
        "extern int x __attribute__((aligned(8))); _Alignas(16) extern int x;",
        true,
        true,
    ),
    (
        "_Alignas(8) extern int x; extern int x __attribute__((aligned(16)));",
        true,
        true,
    ),
    (
        "_Alignas(8) extern int x __attribute__((aligned(32))); _Alignas(16) extern int x __attribute__((aligned(32)));",
        true,
        true,
    ),
    (
        "_Alignas(8) extern int x; int x __attribute__((aligned(16)));",
        true,
        false,
    ),
    ("struct S; _Alignas(8) extern struct S x;", true, true),
    ("_Alignas(16) struct S{int x;};", true, true),
    ("void f(void) __attribute__((aligned(1)));", true, true),
    (
        "void f(void){} void f(void) __attribute__((aligned(32)));",
        true,
        true,
    ),
    (
        "typedef int T __attribute__((aligned(32))); T x __attribute__((aligned(1)));",
        true,
        true,
    ),
    (
        "int x __attribute__((aligned(32))); void f(void){extern int x __attribute__((aligned(1)));}",
        true,
        true,
    ),
];

#[test]
fn declaration_constraints_follow_the_compiler_profile() {
    for profile in CompilerProfile::ALL {
        for &(source, gnu, clang) in CASES {
            let result = parity(source, profile);
            assert_eq!(
                result.is_ok(),
                if profile.compiler() == Compiler::Gnu {
                    gnu
                } else {
                    clang
                },
                "{profile:?}: {source}: {result:?}"
            );
        }
    }
}

#[test]
fn object_attributes_preserve_type_layout_and_each_written_declaration() {
    for profile in CompilerProfile::ALL {
        let analysis = parity("extern int x __attribute__((aligned(8))); extern int x __attribute__((aligned(32))); _Alignas(16) int y __attribute__((aligned(1))); _Alignas(0) int z; void f(void) __attribute__((aligned(32)));", profile).unwrap();
        let unit = analysis.unit();
        let x = unit.declarations.iter().find(|d| d.name == "x").unwrap();
        assert_eq!(unit.alignment(&x.ty).unwrap(), 4);
        assert_eq!(unit.declaration_alignment(x).unwrap(), 32);
        let y = unit.declarations.iter().find(|d| d.name == "y").unwrap();
        assert_eq!(y.alignment.gnu().unwrap().get(), 1);
        assert_eq!(y.alignment.c11(), Some(16));
        assert_eq!(unit.declaration_alignment(y).unwrap(), 16);
        let z = unit.declarations.iter().find(|d| d.name == "z").unwrap();
        assert_eq!(z.alignment.c11(), Some(0));
        assert_eq!(unit.declaration_alignment(z).unwrap(), 4);
        let f = unit.declarations.iter().find(|d| d.name == "f").unwrap();
        assert_eq!(f.alignment.gnu().unwrap().get(), 32);
        assert!(unit.declaration_alignment(f).is_err());
        let code = analysis.checked().unwrap();
        let sites: Vec<_> = code
            .declarations()
            .filter(|(_, site)| code.entity(site.entity()).unwrap().name() == Some("x"))
            .map(|(_, site)| site)
            .collect();
        assert_eq!(sites.len(), 2);
        assert_eq!(sites[0].alignment().gnu().unwrap().get(), 8);
        assert_eq!(sites[1].alignment().gnu().unwrap().get(), 32);
        assert_eq!(
            code.entity(sites[0].entity()).unwrap().alignment(),
            x.alignment
        );
    }
}

#[test]
fn lexical_alignment_and_linked_alignment_have_distinct_owners() {
    for profile in CompilerProfile::ALL {
        let analysis = parity("extern int x; void f(void){extern int x __attribute__((aligned(1))); {int x __attribute__((aligned(32)));} _Alignas(16) int local;}", profile).unwrap();
        let unit = analysis.unit();
        let x = unit.declarations.iter().find(|d| d.name == "x").unwrap();
        assert_eq!(unit.declaration_alignment(x).unwrap(), 4);
        let code = analysis.checked().unwrap();
        let entities: Vec<_> = code
            .entities()
            .filter(|(_, entity)| entity.name() == Some("x"))
            .map(|(_, entity)| entity)
            .collect();
        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0].alignment(), x.alignment);
        assert_eq!(entities[1].alignment().effective().unwrap().get(), 32);
        let local_extern = code
            .declarations()
            .map(|(_, site)| site)
            .find(|site| site.alignment().gnu().is_some_and(|value| value.get() == 1))
            .unwrap();
        assert_eq!(
            local_extern
                .effective_alignment()
                .effective()
                .unwrap()
                .get(),
            if profile.compiler() == Compiler::Gnu {
                4
            } else {
                1
            }
        );
        assert!(
            code.entities()
                .any(|(_, entity)| entity.name() == Some("local")
                    && entity.alignment().c11() == Some(16))
        );
    }
}

#[test]
fn public_alignment_queries_reject_malformed_metadata() {
    for value in [3, (1 << 28) + 1, 1 << 29, u32::MAX] {
        assert!(toucan_semantic::DeclarationAlignment::new(Some(value), None).is_err());
        assert!(toucan_semantic::DeclarationAlignment::new(None, Some(value)).is_err());
    }
    let alignment = toucan_semantic::DeclarationAlignment::new(Some(1 << 28), Some(0)).unwrap();
    assert_eq!(alignment.gnu().unwrap().get(), 1 << 28);
    assert_eq!(alignment.c11(), Some(0));
}

#[test]
#[ignore = "requires GNU GCC and Clang; run with --include-ignored"]
fn declaration_constraints_match_native_compilers_and_clang_targets() {
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(identity.status.success());
    assert!(
        !String::from_utf8_lossy(&identity.stdout)
            .to_ascii_lowercase()
            .contains("clang")
    );
    for (compiler, gnu) in [(gcc.as_str(), true), ("clang", false)] {
        let targets: Vec<_> = if gnu {
            vec![None]
        } else {
            toucan_target::Target::ALL.into_iter().map(Some).collect()
        };
        for target in targets {
            for &(source, gcc_accepts, clang_accepts) in CASES {
                let mut command = Command::new(compiler);
                command.args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"]);
                if let Some(target) = target {
                    command.args(["-target", target.triple()]);
                }
                let mut child = command
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(source.as_bytes())
                    .unwrap();
                let output = child.wait_with_output().unwrap();
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&output),
                    Ok(if gnu { gcc_accepts } else { clang_accepts }),
                    "{compiler} {target:?}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

#[test]
fn redeclaration_alignment_includes_the_compilers_scope_rules() {
    let cases = [
        (
            "extern int x __attribute__((aligned(1))); extern int x;",
            4,
            1,
        ),
        (
            "extern int x; extern int x __attribute__((aligned(1)));",
            4,
            1,
        ),
        (
            "extern int x; void f(void){extern int x __attribute__((aligned(32)));}",
            32,
            4,
        ),
        (
            "void f(void){extern int x __attribute__((aligned(1)));} extern int x;",
            4,
            1,
        ),
        (
            "void f(void){extern int x __attribute__((aligned(32)));} extern int x;",
            32,
            32,
        ),
        (
            "void a(void){extern int x __attribute__((aligned(8)));} void b(void){extern int x __attribute__((aligned(32)));} extern int x;",
            32,
            8,
        ),
        (
            "void a(void){extern int x __attribute__((aligned(8))); extern int x __attribute__((aligned(32)));} extern int x;",
            32,
            8,
        ),
        (
            "void a(void){extern int x __attribute__((aligned(1)));} void b(void){extern int x;} extern int x;",
            4,
            1,
        ),
    ];
    for profile in CompilerProfile::ALL {
        for (source, gnu, clang) in cases {
            let analysis = parity(source, profile).unwrap();
            let x = analysis
                .unit()
                .declarations
                .iter()
                .find(|d| d.name == "x")
                .unwrap();
            assert_eq!(
                analysis.unit().declaration_alignment(x).unwrap(),
                if profile.compiler() == Compiler::Gnu {
                    gnu
                } else {
                    clang
                },
                "{profile:?}: {source}"
            );
        }
    }
}

#[test]
fn alignment_operands_do_not_evaluate_new_vla_bounds() {
    for profile in CompilerProfile::ALL {
        let analysis = parity("void f(int n){ _Alignas(int[n++]) int x; _Alignas(sizeof(int(*)[n++])) int y; int z __attribute__((aligned(sizeof(int(*)[n++]))));}", profile).unwrap();
        let bounds: Vec<_> = analysis
            .checked()
            .unwrap()
            .bounds()
            .map(|(_, bound)| bound.evaluation())
            .collect();
        assert_eq!(
            bounds,
            [toucan_semantic::checked::BoundEvaluation::Unevaluated; 3]
        );
    }
}

#[test]
fn field_alignment_preserves_c11_annotations_without_changing_the_field_type() {
    for profile in CompilerProfile::ALL {
        let analysis = parity("struct S{char lead; _Alignas(16) int x;};", profile).unwrap();
        let unit = analysis.unit();
        let record = unit
            .records
            .iter()
            .find(|record| record.name.as_deref() == Some("S"))
            .unwrap();
        assert_eq!(record.fields.as_ref().unwrap()[1].alignment, Some(16));
        assert_eq!(
            unit.alignment(&record.fields.as_ref().unwrap()[1].ty)
                .unwrap(),
            4
        );
        let field = analysis
            .checked()
            .unwrap()
            .declarations()
            .map(|(_, site)| site)
            .find(|site| site.alignment().c11() == Some(16))
            .unwrap();
        assert_eq!(field.effective_alignment().effective(), None);
    }
}

#[test]
fn rejects_unsupported_bitfield_attribute_types() {
    for profile in CompilerProfile::ALL {
        for (source, message) in [
            (
                "struct S{unsigned :9 __attribute__((mode(QI)));};",
                "bitfields wider than their attribute-modified type are not supported",
            ),
            (
                "struct S{unsigned :3 __attribute__((vector_size(16)));};",
                "vector bitfield types are not supported",
            ),
            (
                "struct S{unsigned named:9 __attribute__((mode(QI)));};",
                "bitfields wider than their attribute-modified type are not supported",
            ),
            (
                "struct S{unsigned named:3 __attribute__((vector_size(16)));};",
                "vector bitfield types are not supported",
            ),
        ] {
            assert_eq!(parity(source, profile).unwrap_err().message, message);
        }
    }
}

#[test]
fn unnamed_bitfield_attributes_belong_to_their_own_field() {
    let source = "struct S{char lead; unsigned :0 __attribute__((aligned(8))), named:3 __attribute__((packed)), :0 __attribute__((aligned(16))); char x;};";
    for profile in CompilerProfile::ALL {
        let analysis = parity(source, profile).unwrap();
        let fields = analysis
            .unit()
            .records
            .iter()
            .find(|record| record.name.as_deref() == Some("S"))
            .unwrap()
            .fields
            .as_ref()
            .unwrap();
        assert_eq!(fields.len(), 5);
        assert_eq!(fields[1].alignment, Some(8));
        assert!(!fields[1].packed);
        assert!(fields[2].packed);
        assert_eq!(fields[2].alignment, None);
        assert_eq!(fields[3].alignment, Some(16));
        assert!(!fields[3].packed);
        let code = analysis.checked().unwrap();
        let alignments: Vec<_> = code
            .declarations()
            .filter_map(|(_, site)| site.alignment().gnu().map(|value| value.get()))
            .collect();
        assert_eq!(alignments, [8, 16]);
    }
}

#[test]
#[ignore = "requires GNU GCC and Clang; run with --include-ignored"]
fn aligned_members_match_native_and_cross_target_record_layouts() {
    let sources = [
        ("struct S{char lead;_Alignas(16) int x;};", 1),
        (
            "struct __attribute__((packed)) S{char lead;_Alignas(16) int x;};",
            1,
        ),
        (
            "#pragma pack(push,1)\nstruct S{char lead;_Alignas(16) int x;};\n#pragma pack(pop)\n",
            1,
        ),
        ("struct S{int n;_Alignas(16) int x[];};", 1),
        (
            "struct S{char lead;unsigned :3 __attribute__((mode(QI)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned :3 __attribute__((mode(HI)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned :3 __attribute__((mode(DI)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned :0 __attribute__((mode(QI)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned :0 __attribute__((mode(HI)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned :0 __attribute__((mode(DI)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned :0 __attribute__((aligned(8)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned :0 __attribute__((packed));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned :9 __attribute__((packed));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned :9 __attribute__((aligned(8)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned :9 __attribute__((aligned(8),packed));char x;};",
            2,
        ),
        (
            "struct __attribute__((packed)) S{char lead;unsigned :0 __attribute__((aligned(8)));char x;};",
            2,
        ),
        (
            "#pragma pack(push,1)\nstruct S{char lead;unsigned :0 __attribute__((aligned(8)));char x;};\n#pragma pack(pop)\n",
            2,
        ),
        (
            "struct S{char lead;unsigned :0 __attribute__((aligned(8))), named:3 __attribute__((packed)), :0 __attribute__((aligned(16)));char x;};",
            4,
        ),
        (
            "struct S{char lead;unsigned :sizeof(enum{B=8}) __attribute__((aligned(B)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned named:sizeof(enum{B=8}) __attribute__((aligned(B)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned (__attribute__((aligned(sizeof(enum{A=4})))) named):sizeof(enum{B=A*2}) __attribute__((aligned(B)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned named:3 __attribute__((mode(QI)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned named:3 __attribute__((mode(HI)));char x;};",
            2,
        ),
        (
            "struct S{char lead;unsigned named:3 __attribute__((mode(DI)));char x;};",
            2,
        ),
    ];
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let host = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => toucan_target::Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => toucan_target::Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => toucan_target::Target::X86_64AppleDarwin,
        ("aarch64", "macos") => toucan_target::Target::Aarch64AppleDarwin,
        _ => return,
    };
    for profile in CompilerProfile::ALL {
        if profile.compiler() == Compiler::Gnu && profile.target() != host {
            continue;
        }
        let compiler = if profile.compiler() == Compiler::Gnu {
            gcc.as_str()
        } else {
            "clang"
        };
        for (source, field) in sources {
            let analysis = parity(source, profile).unwrap();
            let id = analysis
                .unit()
                .records
                .iter()
                .position(|record| record.name.as_deref() == Some("S"))
                .unwrap();
            let layout = analysis
                .unit()
                .layout(&toucan_semantic::Type::new(
                    toucan_semantic::TypeKind::Record(id),
                ))
                .unwrap();
            let probe = format!(
                "{source}\n_Static_assert(sizeof(struct S)=={},\"size\");\n_Static_assert(_Alignof(struct S)=={},\"alignment\");\n_Static_assert(__builtin_offsetof(struct S,x)=={},\"offset\");",
                layout.size_bytes(),
                layout.alignment_bytes(),
                layout.fields[field].as_ref().unwrap().offset_bits / 8
            );
            let mut command = Command::new(compiler);
            command.args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"]);
            if profile.compiler() == Compiler::Clang {
                command.args(["-target", profile.target().triple()]);
            }
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(probe.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(true),
                "{profile:?}: {probe}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

use toucan_semantic::{
    Analysis, AnalysisOptions, Error, IntegerKind, Type, TypeKind, analyze_with_profile,
    evaluate_integer,
};
use toucan_target::{Compiler, CompilerProfile, Target};

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

const RANGES: &[(&str, IntegerKind, &str, u64)] = &[
    ("A=0,B=0", IntegerKind::UnsignedChar, "unsigned char", 1),
    (
        "A=(_Bool)0,B=(_Bool)1",
        IntegerKind::UnsignedChar,
        "unsigned char",
        1,
    ),
    ("A=0,B=255", IntegerKind::UnsignedChar, "unsigned char", 1),
    ("A=-128,B=127", IntegerKind::SignedChar, "signed char", 1),
    ("A=0,B=256", IntegerKind::UnsignedShort, "unsigned short", 2),
    ("A=-129,B=127", IntegerKind::Short, "short", 2),
    ("A=-1,B=255", IntegerKind::Short, "short", 2),
    (
        "A=0,B=65535",
        IntegerKind::UnsignedShort,
        "unsigned short",
        2,
    ),
    ("A=-32768,B=32767", IntegerKind::Short, "short", 2),
    ("A=0,B=65536", IntegerKind::UnsignedInt, "unsigned int", 4),
    ("A=-32769,B=32767", IntegerKind::Int, "int", 4),
    (
        "A=0,B=2147483647",
        IntegerKind::UnsignedInt,
        "unsigned int",
        4,
    ),
];

fn range_probe(profile: CompilerProfile, values: &str, c_type: &str, bytes: u64) -> String {
    let microsoft = profile.target().is_windows();
    let (c_type, bytes) = if microsoft {
        ("int", 4)
    } else {
        (c_type, bytes)
    };
    format!(
        "enum __attribute__((packed)) E{{{values}}};\n_Static_assert(sizeof(enum E)=={bytes},\"size\");\n_Static_assert(_Alignof(enum E)=={bytes},\"alignment\");\n_Static_assert(_Generic((enum E)0,{c_type}:1,default:0),\"compatible integer\");\n_Static_assert(_Generic(A,int:1,default:0),\"enumerator A\");\n_Static_assert(_Generic(B,int:1,default:0),\"enumerator B\");"
    )
}

#[test]
fn packed_ranges_select_the_compatible_integer_without_changing_identifiers() {
    for profile in CompilerProfile::ALL {
        for &(values, expected, c_type, bytes) in RANGES {
            let source = range_probe(profile, values, c_type, bytes);
            let analysis =
                parity(&source, profile).unwrap_or_else(|e| panic!("{profile:?}: {source}: {e}"));
            let unit = analysis.unit();
            assert!(unit.enums[0].packed);
            assert_eq!(
                unit.enum_integer_kind(0).unwrap(),
                if profile.target().is_windows() {
                    IntegerKind::Int
                } else {
                    expected
                }
            );
            for name in ["A", "B"] {
                let value = unit.constants[name];
                assert_eq!((value.bits, value.signed, value.rank), (32, true, 3));
            }
        }
    }
}

#[test]
fn wide_packed_enums_keep_the_existing_compiler_range_rules() {
    for profile in CompilerProfile::ALL {
        for (values, kind) in [
            ("A=0,B=0xffffffffU", IntegerKind::UnsignedInt),
            ("A=0,B=1ULL<<40", IntegerKind::UnsignedLong),
            ("A=-1,B=1ULL<<40", IntegerKind::Long),
            ("A=0,B=~0ULL", IntegerKind::UnsignedLong),
            ("A=-1,B=~0ULL", IntegerKind::Int128),
        ] {
            let source = format!("enum __attribute__((packed)) E{{{values}}};");
            let result = parity(&source, profile);
            if (profile.target().is_windows() && kind != IntegerKind::UnsignedInt)
                || (kind == IntegerKind::Int128
                    && (profile.compiler() == Compiler::Clang
                        || profile.target() == Target::I686UnknownLinuxGnu))
            {
                assert!(result.is_err(), "{profile:?}: {source}");
            } else {
                let kind = if profile.target().is_windows() {
                    IntegerKind::Int
                } else if profile.target() == Target::I686UnknownLinuxGnu {
                    match kind {
                        IntegerKind::UnsignedLong => IntegerKind::UnsignedLongLong,
                        IntegerKind::Long => IntegerKind::LongLong,
                        other => other,
                    }
                } else {
                    kind
                };
                assert_eq!(result.unwrap().unit().enum_integer_kind(0).unwrap(), kind);
            }
        }
    }
}

// Prefix/postfix packing applies to definitions. GNU ignores a forward tag
// attribute; Clang retains it. Attributes after completion/declarator do not
// repack an already defined enum or mutate a typedef's underlying tag.
const PLACEMENTS: &[(&str, bool, bool)] = &[
    ("enum __attribute__((packed)) E{A=0,B=255};", true, true),
    ("enum E{A=0,B=255} __attribute__((packed));", true, true),
    (
        "typedef enum __attribute__((packed)) E{A=0,B=255} T;",
        true,
        true,
    ),
    (
        "typedef enum E{A=0,B=255} T __attribute__((packed));",
        false,
        false,
    ),
    (
        "enum __attribute__((packed)) E;enum E{A=0,B=255};",
        false,
        true,
    ),
    (
        "enum E;enum __attribute__((packed)) E{A=0,B=255};",
        true,
        true,
    ),
    (
        "enum E{A=0,B=255};enum __attribute__((packed)) E;",
        false,
        false,
    ),
    (
        "enum E{A=0,B=255};typedef enum E T __attribute__((packed));",
        false,
        false,
    ),
    (
        "enum E{A=0,B=255};enum E object __attribute__((packed));",
        false,
        false,
    ),
    ("enum E{A __attribute__((packed))=0,B=255};", false, false),
];

#[test]
fn tag_attribute_placement_does_not_repack_unrelated_objects() {
    for profile in CompilerProfile::ALL {
        for &(source, gnu, clang) in PLACEMENTS {
            let analysis = parity(source, profile).unwrap();
            let unit = analysis.unit();
            let packed = if profile.compiler() == Compiler::Gnu {
                gnu
            } else {
                clang
            };
            assert_eq!(unit.enums[0].packed, packed, "{profile:?}: {source}");
            let expected_size = if packed && !profile.target().is_windows() {
                1
            } else {
                4
            };
            assert_eq!(
                unit.layout(&Type::new(TypeKind::Enum(0)))
                    .unwrap()
                    .size_bytes(),
                expected_size
            );
        }
        assert_eq!(
            parity("enum __attribute__((aligned(16))) E{A=0};", profile).is_ok(),
            profile.compiler() == Compiler::Gnu,
        );
        assert!(
            parity("enum __attribute__((packed)) E;", profile)
                .unwrap()
                .unit()
                .enum_integer_kind(0)
                .unwrap_err()
                .message
                .contains("incomplete enum")
        );
    }
}

#[test]
fn tag_alignment_is_admitted_only_when_storage_rules_are_unchanged() {
    for profile in CompilerProfile::ALL {
        for (attributes, bytes) in [
            ("aligned(4)", 4),
            ("aligned(1),aligned(4)", 4),
            ("aligned(4),aligned(1)", 4),
            ("packed,aligned(1)", 1),
        ] {
            let source = format!(
                "enum __attribute__(({attributes})) E{{A=1,B=2}}; struct __attribute__((packed)) Packed{{char head;enum E field;char tail;}};\n#pragma pack(push,1)\nstruct Pragma{{char head;enum E field;char tail;}};\n#pragma pack(pop)\n_Static_assert(sizeof(enum E)=={bytes} && _Alignof(enum E)=={bytes},\"enum\");\n_Static_assert(sizeof(struct Packed)=={bytes}+2 && _Alignof(struct Packed)==1 && __builtin_offsetof(struct Packed,field)==1,\"packed\");\n_Static_assert(sizeof(struct Pragma)=={bytes}+2 && _Alignof(struct Pragma)==1 && __builtin_offsetof(struct Pragma,field)==1,\"pragma\"); enum E array[2]; int use(enum E *p){{return *p;}}"
            );
            let result = parity(&source, profile);
            if profile.target().is_windows() {
                assert!(result.unwrap_err().message.contains("Microsoft enum"));
            } else {
                result.unwrap_or_else(|error| panic!("{profile:?}: {source}: {error}"));
            }
        }
        for source in [
            "enum __attribute__((aligned(1))) E{A=1};",
            "enum __attribute__((aligned(16))) E{A=1};",
            "enum __attribute__((packed,aligned(4))) E{A=1};",
            "enum __attribute__((aligned)) E{A=1};",
        ] {
            let result = parity(source, profile);
            assert_eq!(
                result.is_ok(),
                profile.compiler() == Compiler::Gnu,
                "{profile:?}: {source}"
            );
            if let Err(error) = result {
                assert!(error.offset < source.find(" E").unwrap());
                assert!(error.message.contains("unsupported"));
            }
        }
        for source in [
            "enum __attribute__((aligned(3))) E{A=1};",
            "enum __attribute__((aligned(-1))) E{A=1};",
            "int value; enum __attribute__((aligned(value))) E{A=1};",
            "enum __attribute__((aligned(1,2))) E{A=1};",
        ] {
            assert!(parity(source, profile).is_err(), "{profile:?}: {source}");
        }
        for source in [
            "enum __attribute__((aligned(0))) E{A=1};",
            "int object __attribute__((aligned(0)));",
            "typedef int Alias __attribute__((aligned(0))); ",
        ] {
            assert_eq!(
                parity(source, profile).is_ok(),
                profile.compiler() == Compiler::Gnu
            );
        }
        parity("_Alignas(0) int object;", profile).unwrap();
    }
}

#[test]
fn alignment_forwards_and_late_declarations_keep_their_distinct_constraints() {
    for profile in CompilerProfile::ALL {
        for source in [
            "int before; enum __attribute__((aligned(4))) E; enum E{A=1};",
            "int before; enum __attribute__((aligned(4))) E; enum __attribute__((packed)) E{A=1};",
        ] {
            let result = parity(source, profile);
            if profile.compiler() == Compiler::Gnu {
                result.unwrap();
            } else {
                let error = result.unwrap_err();
                assert!(
                    (source.find("enum").unwrap()..source.find(" E").unwrap())
                        .contains(&error.offset)
                );
                assert!(error.message.contains(if profile.target().is_windows() {
                    "Microsoft enum"
                } else {
                    "incomplete enum"
                }));
            }
        }
        parity("enum E{A=1};enum __attribute__((aligned(16))) E;_Static_assert(_Alignof(enum E)==4,\"late ignored\");",profile).unwrap();
        if !profile.target().is_windows() {
            let alignment = if profile.target() == Target::I686UnknownLinuxGnu
                && profile.compiler() == Compiler::Gnu
            {
                4
            } else {
                8
            };
            let source = format!(
                "enum E{{A=0x100000000ULL}} __attribute__((aligned(8)));_Static_assert(sizeof(enum E)==8 && _Alignof(enum E)=={alignment},\"wide\");"
            );
            let result = parity(&source, profile);
            if profile.target() == Target::I686UnknownLinuxGnu
                && profile.compiler() == Compiler::Clang
            {
                assert!(
                    result
                        .unwrap_err()
                        .message
                        .contains("enum alignment that changes storage layout"),
                    "{source}"
                );
            } else {
                result.unwrap();
            }
        }
    }
}

#[test]
fn packed_values_promote_but_enum_tags_keep_nominal_identity() {
    for profile in CompilerProfile::ALL {
        let header = "enum __attribute__((packed)) E{A=0,B=255};";
        let integer = if profile.target().is_windows() {
            "int"
        } else {
            "unsigned char"
        };
        parity(&format!("{header}_Static_assert(_Generic(+(enum E)B,int:1,default:0),\"promote\");_Static_assert((enum E)B+1==256,\"arithmetic\");int f(enum E*);int f({integer}*);"),profile).unwrap();
        for tail in [
            "typedef enum E T;typedef unsigned char T;",
            "enum __attribute__((packed)) F{C=0,D=255};int f(enum E*);int f(enum F*);",
        ] {
            assert!(parity(&format!("{header}{tail}"), profile).is_err());
        }
        let old_style = parity(&format!("{header}int f();int f(enum E);"), profile);
        assert_eq!(old_style.is_ok(), profile.target().is_windows());
        let too_wide = parity(&format!("{header}struct Bits{{enum E field:9;}};"), profile);
        assert_eq!(too_wide.is_ok(), profile.target().is_windows());
        let analysis = parity(&format!("{header}int variadic(int,...);int run(enum E value){{return variadic(0,value)+(+value);}}"),profile).unwrap();
        let code = analysis.checked().unwrap();
        let mut promoted = 0;
        for (_, expression) in code.expressions() {
            if let toucan_semantic::checked::ExprKind::Call { arguments, .. } = expression.kind() {
                for argument in arguments {
                    if argument.conversions().iter().any(|step| {
                        step.kind() == toucan_semantic::checked::Conversion::DefaultArgument
                    }) {
                        assert!(matches!(
                            code.ty(argument.effective_type()).unwrap().kind,
                            TypeKind::Integer(IntegerKind::Int)
                        ));
                        promoted += 1;
                    }
                }
            }
        }
        assert_eq!(promoted, 1);
        assert_eq!(
            evaluate_integer(analysis.unit(), "(enum E)255+1")
                .unwrap()
                .value,
            256
        );
    }
}

#[test]
fn public_enum_query_checks_incomplete_and_invalid_ids() {
    let profile = CompilerProfile::default_for(Target::X86_64UnknownLinuxGnu);
    let analysis = parity("enum E;", profile).unwrap();
    assert!(analysis.unit().enum_integer_kind(0).is_err());
    assert!(analysis.unit().enum_integer_kind(usize::MAX).is_err());
}

#[test]
#[ignore = "requires genuine GNU GCC and Clang target backends"]
fn packed_enum_layout_types_and_placement_match_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        identity.status.success() && !String::from_utf8_lossy(&identity.stdout).contains("clang")
    );
    for profile in CompilerProfile::ALL {
        if profile.compiler() == Compiler::Gnu
            && (profile.target().triple()
                != format!("{}-unknown-linux-gnu", std::env::consts::ARCH)
                || !cfg!(target_os = "linux"))
        {
            continue;
        }
        let compiler = if profile.compiler() == Compiler::Gnu {
            gcc.as_str()
        } else {
            "clang"
        };
        let mut sources = RANGES
            .iter()
            .map(|&(values, _, name, bytes)| range_probe(profile, values, name, bytes))
            .collect::<Vec<_>>();
        for &(source, gnu, clang) in PLACEMENTS {
            let packed = if profile.compiler() == Compiler::Gnu {
                gnu
            } else {
                clang
            };
            let bytes = if packed && !profile.target().is_windows() {
                1
            } else {
                4
            };
            sources.push(format!(
                "{source}_Static_assert(sizeof(enum E)=={bytes},\"placement\");"
            ));
        }
        for source in sources {
            parity(&source, profile).unwrap();
            let mut command = Command::new(compiler);
            if profile.compiler() == Compiler::Clang {
                command.args(["-target", profile.target().triple()]);
            }
            let mut child = command
                .args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"])
                .stdin(Stdio::piped())
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
            assert!(
                output.status.success(),
                "{profile:?}: {source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

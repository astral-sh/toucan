use toucan_semantic::checked::{BoundEvaluation, BoundSite, Conversion, ParameterEvaluationOrder};
use toucan_semantic::{
    Analysis, AnalysisOptions, Error, FloatKind, IntegerKind, TypeKind, analyze_with_profile,
};
use toucan_target::{Compiler, CompilerProfile};

const VALID: &[&str] = &[
    "int f(a,b) int a;char b;{return a+b;}",
    "int f(a,b) char b;int a;{return a+b;}",
    "int f(a,b) int a,b;{return a+b;}",
    "int f(a) register int a;{return a;}",
    "int f(int,double);int f(a,b) char a;float b;{return a+(int)b;}",
    "int f(a,b) char a;float b;{return a+(int)b;}int f(int,double);",
    "int f(char,float);int f(a,b) char a;float b;{return a+(int)b;}",
    "int f(int,...);int f(a) int a;{__builtin_va_list ap;__builtin_va_start(ap,a);__builtin_va_end(ap);return a;}",
    "int f(a,cb) int a[3];int cb(int);{return cb(a[0]);}",
    "int f(n,a) int n;int a[static n];{return a[0];}",
    "int n;int f(n,a) int a[n];int n;{return a[0];}",
    "int a;int f(a) int (*a)[sizeof a];{return sizeof*a;}",
    "typedef int T;int (*f(a))(int T) int a;{T x=a;return 0;}",
    "int f(a) int a;{{int a=3;}return a;}",
    "int f(a) enum E{A=1} a;{enum E b=a;return b+A;}",
    "int f(a) struct S{int x;} a;{struct S b=a;return b.x;}",
    "int f(a) _Atomic(int) a;{return a;}",
    "int f(_Atomic(int));int f(a) _Atomic(int) a;{return a;}",
    "int f(a) int a;{return a;}int g(void){return f();}",
    "int f(a) float a;{return a;}_Static_assert(__builtin_types_compatible_p(typeof(f),int(int)),\"type\");",
    "int g(void){extern int f(double);return 0;}int f(a) float a;{return a;}",
    "int f(a) float a;{return a;}int g(void){extern int f(double);return 0;}",
    "enum __attribute__((packed)) E{A=1};int f(int);int f(a) enum E a;{return a;}",
    "enum E{A=1};int f(enum E);int f(a) enum E a;{return a;}",
];
const INVALID: &[&str] = &[
    "typedef int F(void);F f{return 0;}",
    "int f(a)[3] int a;{return 0;}",
    "int f(a)() int a;{return 0;}",
    "int f(a){return a;}",
    "int f(a) int a,b;{return a;}",
    "int f(a) int a;int a;{return a;}",
    "int f(a,a) int a;{return a;}",
    "int f(a) int;int a;{return a;}",
    "int f(int a) int a;{return a;}",
    "int f() int a;{return a;}",
    "int f(a) int a=1;{return a;}",
    "int f(a) static int a;{return a;}",
    "int f(a) register int a;{return *&a;}",
    "int f(a) void a;{return 0;}",
    "int f(a,b) char a;float b;{return a+(int)b;}int f(char,float);",
    "int f(int);int f(a,b) int a,b;{return a+b;}",
    "int f(a,b) int a,b;{return a+b;}int f(int);",
    "int f(n,a) int a[n];int n;{return a[0];}",
    "typedef int T;int f(a,T) int a;int T;{return a+T;}",
    "int f(a) int a;{int a;return a;}",
    "int f(a,b);",
    "int f(int);int f(a) _Atomic(int) a;{return a;}",
    "int f(a) float a;{return a;}int g(void){extern int f(int);return 0;}",
    "int f(a) float a;{return a;}int f();int f(int);",
    "int f(a) int a[*];{return 0;}",
    "int f(a) int a[const 3];{a=0;return 0;}",
];

fn parity(source: &str, profile: CompilerProfile) -> Result<Analysis, Error> {
    let normal = analyze_with_profile(source, profile, &AnalysisOptions::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&normal, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(
            format!("{:?}", a.unit()),
            format!("{:?}", b.unit()),
            "{source}"
        ),
        (Err(a), Err(b)) => assert_eq!((&a.message, a.offset), (&b.message, b.offset), "{source}"),
        _ => panic!("{profile:?}: normal {normal:?}, retained {retained:?}: {source}"),
    }
    retained
}

#[test]
fn declarations_scopes_and_constraints_have_seven_profile_parity() {
    for profile in CompilerProfile::ALL {
        for source in VALID {
            parity(source, profile).unwrap_or_else(|e| panic!("{profile:?}: {source}: {e}"));
        }
        for source in INVALID {
            assert!(parity(source, profile).is_err(), "{profile:?}: {source}");
        }
    }
}

#[test]
fn entry_types_and_conversions_preserve_the_adjusted_parameter_objects() {
    let source = "int f(a,b) float b;unsigned char a;{return a+(int)b;}";
    for profile in CompilerProfile::ALL {
        let analysis = parity(source, profile).unwrap();
        let code = analysis.checked().unwrap();
        let (_, body) = code.bodies().next().unwrap();
        let entry = body.old_style().unwrap();
        assert_eq!(
            entry.evaluation_order(),
            ParameterEvaluationOrder::UnspecifiedBetweenParameters
        );
        for (index, parameter) in entry.parameters().iter().enumerate() {
            let incoming = code
                .ty(code.type_use(parameter.incoming()).unwrap().shape())
                .unwrap();
            assert!(matches!(
                (&incoming.kind, index),
                (TypeKind::Integer(IntegerKind::Int), 0) | (TypeKind::Float(FloatKind::Double), 1)
            ));
            let site = code.declaration(parameter.declaration()).unwrap();
            let local = code.ty(site.ty()).unwrap();
            assert!(matches!(
                (&local.kind, index),
                (TypeKind::Integer(IntegerKind::UnsignedChar), 0)
                    | (TypeKind::Float(FloatKind::Float), 1)
            ));
            assert_eq!(parameter.conversions().len(), 1);
            assert_eq!(parameter.conversions()[0].kind(), Conversion::Assignment);
            assert_eq!(body.parameters()[index], parameter.declaration());
            assert_eq!(
                &source[code
                    .occurrence(parameter.identifier())
                    .unwrap()
                    .source()
                    .range()],
                if index == 0 { "a" } else { "b" }
            );
        }
        let names: Vec<_> = entry
            .declarations()
            .iter()
            .flat_map(|id| code.declaration_group(*id).unwrap().declarations())
            .map(|id| {
                code.entity(code.declaration(*id).unwrap().entity())
                    .unwrap()
                    .name()
                    .unwrap()
            })
            .collect();
        assert_eq!(names, ["b", "a"]);
        let adopted = parity("int g(float);int g(a) float a;{return a;}", profile).unwrap();
        let code = adopted.checked().unwrap();
        let (_, body) = code.bodies().next().unwrap();
        let parameter = &body.old_style().unwrap().parameters()[0];
        let incoming = code
            .ty(code.type_use(parameter.incoming()).unwrap().shape())
            .unwrap();
        assert!(matches!(incoming.kind, TypeKind::Float(FloatKind::Float)));
        assert!(parameter.conversions().is_empty());
    }
}

#[test]
fn parameter_bounds_and_tags_keep_source_ownership_and_required_effects() {
    let source = "int bound(int);int f(a,b) int b[bound(2)];int (*a)[bound(1)];{return sizeof*a;}";
    for profile in CompilerProfile::ALL {
        let analysis = parity(source, profile).unwrap();
        let code = analysis.checked().unwrap();
        assert_eq!(
            code.bounds()
                .filter(|(_, b)| b.site() == BoundSite::FunctionEntry
                    && b.evaluation() == BoundEvaluation::Required)
                .count(),
            2
        );
        let (_, body) = code.bodies().next().unwrap();
        let entry = body.old_style().unwrap();
        assert_eq!(entry.parameters().len(), 2);
        assert_eq!(entry.declarations().len(), 2);
        assert_eq!(
            code.type_use(entry.parameters()[0].incoming())
                .unwrap()
                .extents()
                .len(),
            1
        );
        assert!(
            code.type_use(entry.parameters()[1].incoming())
                .unwrap()
                .extents()
                .is_empty()
        );
    }
}

#[test]
fn atomic_promotions_belong_to_definition_entry_not_atomic_loads() {
    for profile in CompilerProfile::ALL {
        let analysis = parity("int f(a) _Atomic(float) a;{return sizeof a;}", profile).unwrap();
        let code = analysis.checked().unwrap();
        let (_, body) = code.bodies().next().unwrap();
        let entry = &body.old_style().unwrap().parameters()[0];
        let incoming = code
            .ty(code.type_use(entry.incoming()).unwrap().shape())
            .unwrap();
        let inner = analysis.unit().atomic_value(incoming).unwrap().unwrap();
        assert!(
            matches!(inner.kind,TypeKind::Float(kind) if kind==if profile.compiler()==Compiler::Gnu {FloatKind::Double}else{FloatKind::Float})
        );
        assert!(
            entry
                .conversions()
                .iter()
                .all(|conversion| conversion.kind() != Conversion::AtomicLoad)
        );
    }
}

#[test]
fn parameter_sites_survive_query_replay_and_c11_minimum_parameter_count() {
    for profile in CompilerProfile::ALL {
        parity(
            "int f(n,a) int n;int (*a)[n];{return __builtin_constant_p(sizeof *a)+sizeof*a;}",
            profile,
        )
        .unwrap();
        let names: Vec<_> = (0..127).map(|index| format!("p{index}")).collect();
        let source = format!(
            "int f({}) int {};{{return p126;}}",
            names.join(","),
            names.join(",")
        );
        let analysis = parity(&source, profile).unwrap();
        let code = analysis.checked().unwrap();
        assert_eq!(code.bodies().next().unwrap().1.parameters().len(), 127);
        let limited = analyze_with_profile(
            &source,
            profile,
            &AnalysisOptions {
                retain_code: true,
                retain_declaration_origins: false,
                limits: toucan_semantic::checked::Limits {
                    payload_bytes: 4096,
                    ..Default::default()
                },
            },
        )
        .unwrap_err();
        assert!(limited.message.contains("limit"));
        assert!(limited.offset <= source.len());
    }
}

const PARAMETER_ENTRY_RUNTIME: &str = include_str!("fixtures/parameter_entry.c");

#[test]
fn unused_vla_parameters_still_require_entry_bound_effects() {
    for profile in CompilerProfile::ALL {
        let analysis = parity(PARAMETER_ENTRY_RUNTIME, profile).unwrap();
        let code = analysis.checked().unwrap();
        assert_eq!(
            code.bounds()
                .filter(|(_, bound)| bound.site() == BoundSite::FunctionEntry
                    && bound.evaluation() == BoundEvaluation::Required)
                .count(),
            4
        );
    }
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn constraints_and_parameter_entry_values_match_native_compilers() {
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    let mut runtime_failures = Vec::new();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc.as_str(), "clang"] {
        let version = Command::new(compiler).arg("--version").output().unwrap();
        assert!(version.status.success());
        let clang = String::from_utf8_lossy(&version.stdout)
            .to_ascii_lowercase()
            .contains("clang");
        for (index, (source, accept)) in VALID
            .iter()
            .map(|source| (*source, true))
            .chain(INVALID.iter().map(|source| (*source, false)))
            .enumerate()
        {
            // Separately documented oracle discrepancies, with source-semantics
            // regressions above: Clang drops declaration-list tag scope; GCC
            // drops definition constraints after an intervening empty declaration.
            if source.contains("enum E{A=1} a;")
                || source.contains("struct S{int x;} a;")
                || source == "int f(a) float a;{return a;}int f();int f(int);"
            {
                continue;
            }
            if source == "int f(a){return a;}"
                || source == "int f(a) int;int a;{return a;}"
                || source == "int f(a,b);"
            {
                continue;
            }
            let path = temp.path().join(format!("case-{index}.c"));
            std::fs::write(&path, format!("{source}\n")).unwrap();
            let mut command = Command::new(compiler);
            command.args([
                "-std=gnu11",
                "-fsyntax-only",
                "-Werror=incompatible-pointer-types",
            ]);
            if clang {
                command.args(["-Wno-deprecated-non-prototype", "-Wno-strict-prototypes"]);
            }
            let output = command.arg(&path).output().unwrap();
            assert_regular_compiler_result(&output, compiler, source);
            assert_eq!(
                output.status.success(),
                accept,
                "{compiler}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let source = PARAMETER_ENTRY_RUNTIME;
        let path = temp.path().join("runtime.c");
        std::fs::write(&path, source).unwrap();
        for opt in ["-O0", "-O2"] {
            let exe = temp.path().join("runtime");
            let mut command = Command::new(compiler);
            command.args(["-std=gnu11", opt]);
            if clang {
                command.arg("-Wno-deprecated-non-prototype");
            }
            let output = command.arg(&path).arg("-o").arg(&exe).output().unwrap();
            assert_regular_compiler_result(&output, compiler, source);
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let executed = Command::new(&exe).output().unwrap();
            let version_text = String::from_utf8_lossy(&version.stdout);
            let known_missing_bound = !clang
                && ((cfg!(target_os = "macos")
                    && version_text.contains("(Homebrew GCC 14.4.0) 14.4.0"))
                    || (cfg!(all(target_os = "linux", target_arch = "x86_64"))
                        && version_text.contains("(Ubuntu 14.2.0-4ubuntu2~24.04.1) 14.2.0")))
                && executed.status.code() == Some(1)
                && matches!(
                    executed.stdout.as_slice(),
                    b"order=0 narrow=16777217 callback=4 prototype=12\n"
                        | b"order=0 narrow=16777217 callback=4 prototype=21\n"
                );
            // These exact GNU builds omit the old-style bound effects. All
            // prototype bounds, narrowing and callback controls must still pass.
            // A compiler that fixes the defect passes normally above.
            if known_missing_bound {
                eprintln!(
                    "recorded GCC parameter-bound defect: {compiler} {opt}: {}",
                    String::from_utf8_lossy(&executed.stdout).trim()
                );
            }
            if !executed.status.success() && !known_missing_bound {
                runtime_failures.push(format!(
                    "{compiler} {opt} ({version_text}): {}\nstdout: {}\nstderr: {}",
                    executed.status,
                    String::from_utf8_lossy(&executed.stdout),
                    String::from_utf8_lossy(&executed.stderr),
                ));
            }
        }
    }
    assert!(
        runtime_failures.is_empty(),
        "{}",
        runtime_failures.join("\n")
    );
}

fn assert_regular_compiler_result(output: &std::process::Output, compiler: &str, source: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        matches!(output.status.code(), Some(0 | 1))
            && ![
                "internal compiler error",
                "PLEASE submit a bug report",
                "Segmentation fault",
                "frontend command failed",
                "Assertion"
            ]
            .iter()
            .any(|marker| stderr.contains(marker)),
        "compiler oracle crashed or failed to run: {compiler}: {source}: {stderr}"
    );
}

#[test]
fn alignment_and_leading_attributes_are_not_silently_reclassified() {
    for profile in CompilerProfile::ALL {
        for source in [
            "int f(a) int a __attribute__((aligned(16)));{return a;}",
            "int f(a) int __attribute__((aligned(16))) a;{return a;}",
        ] {
            let result = parity(source, profile);
            if profile.compiler() == Compiler::Gnu {
                assert!(result.unwrap_err().message.contains("alignment"));
            } else {
                let analysis = result.unwrap();
                let code = analysis.checked().unwrap();
                let (_, body) = code.bodies().next().unwrap();
                let parameter = &body.old_style().unwrap().parameters()[0];
                let site = code.declaration(parameter.declaration()).unwrap();
                assert_eq!(site.alignment().gnu().unwrap().get(), 16);
                assert_eq!(site.effective_alignment().effective().unwrap().get(), 16);
                assert!(matches!(
                    code.ty(site.ty()).unwrap().kind,
                    TypeKind::Integer(IntegerKind::Int)
                ));
                let incoming = code
                    .ty(code.type_use(parameter.incoming()).unwrap().shape())
                    .unwrap();
                assert!(matches!(incoming.kind, TypeKind::Integer(IntegerKind::Int)));
                assert!(parameter.conversions().is_empty());
            }
        }
        for source in [
            "int f(a) __attribute__((unused)) int a;{return a;}",
            "int f(a,b) int a;__attribute__((unused)) int b;{return a+b;}",
        ] {
            assert_eq!(
                parity(source, profile).is_ok(),
                profile.compiler() == Compiler::Clang
            );
        }
        parity(
            "int f(a) int __attribute__((unused)) a;{return a;}",
            profile,
        )
        .unwrap();
        parity(
            "int f(a) int a __attribute__((unused));{return a;}",
            profile,
        )
        .unwrap();
    }
}

#[test]
fn function_attributes_cover_identifier_list_bounds_and_keep_parameter_sites() {
    let cases = [
        (
            r#"int f(a) __attribute__((unused)) int a; {return a;}"#,
            false,
            true,
        ),
        (
            r#"int f(a) __attribute__((returns_twice)) int a; {return a;}"#,
            false,
            true,
        ),
        (
            r#"int f(a) __attribute__((target("mmx"))) int a[(__builtin_ia32_emms(),3)]; {return a[0];}"#,
            false,
            true,
        ),
        (
            r#"int f(a) __attribute__((target("no-mmx"))) int a[(__builtin_ia32_emms(),3)]; {return a[0];}"#,
            false,
            false,
        ),
        (
            r#"__attribute__((target("mmx"))) int f(a) int a[(__builtin_ia32_emms(),3)]; {return a[0];}"#,
            true,
            true,
        ),
        (
            r#"__attribute__((target("no-mmx"))) int f(a) int a[(__builtin_ia32_emms(),3)]; {return a[0];}"#,
            true,
            false,
        ),
        (
            r#"int f(a) __attribute__((target("no-mmx"))) int a[sizeof((__builtin_ia32_emms(),3))]; {return a[0];}"#,
            false,
            true,
        ),
        (
            r#"int f(a) __attribute__((returns_twice,target("mmx"))) int a; {return a;}"#,
            false,
            true,
        ),
    ];
    for profile in CompilerProfile::ALL.into_iter().filter(|profile| {
        matches!(
            profile.target(),
            toucan_target::Target::X86_64UnknownLinuxGnu
                | toucan_target::Target::X86_64UnknownLinuxMusl
                | toucan_target::Target::X86_64AppleDarwin
                | toucan_target::Target::X86_64PcWindowsMsvc
        )
    }) {
        for (source, gnu, clang) in cases {
            let analysis = parity(source, profile);
            let expected = if profile.compiler() == Compiler::Gnu {
                gnu
            } else {
                clang
            };
            assert_eq!(
                analysis.is_ok(),
                expected,
                "{profile:?}: {source}: {analysis:?}"
            );
            if let Ok(analysis) = analysis {
                let code = analysis.checked().unwrap();
                let (_, body) = code.bodies().next().unwrap();
                assert!(body.old_style().is_some());
                assert_eq!(
                    analysis.unit().declarations[0].returns_twice,
                    source.contains("returns_twice")
                );
                for entry in body.old_style().unwrap().parameters() {
                    assert!(
                        !code
                            .declaration(entry.declaration())
                            .unwrap()
                            .returns_twice()
                    );
                }
                if source.contains("target(") {
                    assert_eq!(analysis.unit().function_options.len(), 1);
                }
            }
        }
    }
}

use toucan_semantic::{
    Analysis, AnalysisOptions, Error, FloatKind, IntegerKind, TypeKind, analyze_with_profile,
    evaluate_integer,
};
use toucan_target::{CompilerProfile, LanguageMode, Target};

fn parity(source: &str, profile: CompilerProfile) -> Result<Analysis, Error> {
    let ordinary = analyze_with_profile(source, profile, &AnalysisOptions::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&ordinary, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(
            format!("{:?}", a.unit()),
            format!("{:?}", b.unit()),
            "{source}"
        ),
        (Err(a), Err(b)) => assert_eq!((&a.message, a.offset), (&b.message, b.offset), "{source}"),
        _ => panic!("{profile:?}: {source}: ordinary {ordinary:?}, retained {retained:?}"),
    }
    retained
}

#[test]
fn implicit_int_declarations_preserve_block_expressions_and_parameter_order() {
    for profile in CompilerProfile::ALL {
        for mode in [LanguageMode::C90, LanguageMode::Gnu90] {
            let profile = profile.with_language_mode(mode);
            for source in [
                "x; extern y; static z; typedef T; T w;",
                "f(){return 1;} static g(){return f();}",
                "int f(int x){x; x=2; return x;}",
                "int f(const x){return x;}",
                "int f(void){return sizeof(const);}",
                "int f(a,b) char b; {return a+b;} int f(int,int);",
                "enum {a=42}; int f(a){return a;} _Static_assert(a==42,\"scope\");",
            ] {
                parity(source, profile)
                    .unwrap_or_else(|error| panic!("{profile:?}: {source}: {error}"));
            }
            let source = "f(a,b,c) float b; {return a+(int)b+c;}";
            let analysis = parity(source, profile).unwrap();
            let code = analysis.checked().unwrap();
            let (_, body) = code.bodies().next().unwrap();
            let entry = body.old_style().unwrap();
            assert_eq!(entry.parameters().len(), 3);
            assert_eq!(entry.declarations().len(), 1);
            for (index, parameter) in entry.parameters().iter().enumerate() {
                let site = code.declaration(parameter.declaration()).unwrap();
                let ty = code.ty(site.ty()).unwrap();
                assert!(matches!(
                    (&ty.kind, index),
                    (TypeKind::Integer(IntegerKind::Int), 0 | 2)
                        | (TypeKind::Float(FloatKind::Float), 1)
                ));
                assert_eq!(
                    &source[code
                        .occurrence(parameter.identifier())
                        .unwrap()
                        .source()
                        .range()],
                    ["a", "b", "c"][index]
                );
                assert_eq!(body.parameters()[index], parameter.declaration());
            }
        }
    }
}

#[test]
fn decimal_types_follow_c90_candidate_order_and_preserve_large_values() {
    for profile in CompilerProfile::ALL {
        for mode in [LanguageMode::C90, LanguageMode::Gnu90] {
            let profile = profile.with_language_mode(mode);
            let analysis = parity("", profile).unwrap();
            let unit = analysis.unit();
            let large = evaluate_integer(unit, "9223372036854775808").unwrap();
            assert_eq!(large.as_u64().unwrap(), 1u64 << 63);
            assert!(!large.signed);
            let expected = if profile.target() == Target::X86_64PcWindowsMsvc {
                "unsigned long long"
            } else {
                "unsigned long"
            };
            let source = format!(
                "_Static_assert(__builtin_types_compatible_p(__typeof__(9223372036854775808),{expected}),\"C90\");"
            );
            parity(&source, profile).unwrap();
            assert!(evaluate_integer(unit, "18446744073709551616").is_err());
        }
    }
}

#[test]
fn direct_source_and_expression_queries_share_comment_boundaries() {
    for profile in CompilerProfile::ALL {
        let profile = profile.with_language_mode(LanguageMode::C90);
        let analysis = parity("_Static_assert(6 //**/ 2 == 3,\"division\");", profile).unwrap();
        assert_eq!(
            evaluate_integer(analysis.unit(), "6 //**/ 2")
                .unwrap()
                .as_u64()
                .unwrap(),
            3
        );
        for source in ["6 //**/ 2; int injected", "6 //**/ 2) ; int injected("] {
            assert!(evaluate_integer(analysis.unit(), source).is_err());
        }
    }
}

#[test]
fn implicit_functions_keep_scope_linkage_and_default_promotions() {
    for profile in CompilerProfile::ALL {
        for mode in [LanguageMode::C90, LanguageMode::Gnu90] {
            let profile = profile.with_language_mode(mode);
            for source in [
                "int f(void){return missing(1);}",
                "int f(void){return missing(1.0f);} int missing(double);",
                "int f(void){missing(1);return missing(2);}",
                "int f(void){{missing(1);}return missing(2);}",
                "int f(void){{extern int missing(double);}return missing(1.0f);}",
                "int n[sizeof(missing())]; int f(void){return missing(2);}",
                "int f(void){return missing(1);} int missing(x){return x;}",
            ] {
                parity(source, profile)
                    .unwrap_or_else(|error| panic!("{profile:?}: {source}: {error}"));
            }
            for source in [
                "int f(void){return (missing)(1);}",
                "int f(void){return (*missing)(1);}",
                "int missing; int f(void){return missing(1);}",
                "int f(void){{missing(1);}return sizeof(&missing);}",
                "int f(void){return missing(1.0f);} int missing(float);",
                "int f(void){return malloc(1)!=0;}",
                "int f(void){return __builtin_toucan_unknown(1);}",
            ] {
                assert!(parity(source, profile).is_err(), "{profile:?}: {source}");
            }
            let source = "int f(void){{missing(1.0f);}return missing(2);}";
            let analysis = parity(source, profile).unwrap();
            assert_eq!(analysis.unit().declarations.len(), 1);
            let code = analysis.checked().unwrap();
            let sites: Vec<_> = code
                .declarations()
                .filter(|(_, site)| {
                    code.occurrence(site.occurrence()).unwrap().kind()
                        == toucan_semantic::checked::OccurrenceKind::ImplicitFunction
                })
                .collect();
            assert_eq!(sites.len(), 2);
            assert_eq!(sites[0].1.entity(), sites[1].1.entity());
            for (_, site) in sites {
                assert_eq!(&source[site.name_source().unwrap().range()], "missing");
            }
            let source = "int f(void){return missing(1);} static int missing(int x){return x;}";
            let result = parity(source, profile);
            assert_eq!(
                result.is_ok(),
                profile.target() == Target::X86_64PcWindowsMsvc
            );
            if let Ok(analysis) = result {
                assert!(
                    !analysis
                        .unit()
                        .declarations
                        .iter()
                        .find(|d| d.name == "missing")
                        .unwrap()
                        .is_static
                );
                let code = analysis.checked().unwrap();
                let (_, entity) = code
                    .entities()
                    .find(|(_, entity)| entity.name() == Some("missing"))
                    .unwrap();
                assert_eq!(
                    entity.linkage(),
                    toucan_semantic::checked::Linkage::External
                );
                let (_, site) = code
                    .declarations()
                    .find(|(_, site)| {
                        code.occurrence(site.occurrence()).unwrap().kind()
                            == toucan_semantic::checked::OccurrenceKind::ImplicitFunction
                    })
                    .unwrap();
                assert_eq!(site.linkage(), toucan_semantic::checked::Linkage::External);
            }
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang; checks all Clang targets and native GNU profiles"]
fn c90_scope_and_syntax_decisions_match_compilers() {
    use std::process::Command;
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let clang = std::env::var("TOUCAN_CLANG").unwrap_or_else(|_| "clang".into());
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("c90.c");
    for profile in CompilerProfile::ALL {
        let gnu = profile.compiler() == toucan_target::Compiler::Gnu;
        if gnu
            && !(cfg!(target_os = "linux")
                && match profile.target() {
                    Target::X86_64UnknownLinuxGnu => cfg!(target_arch = "x86_64"),
                    Target::Aarch64UnknownLinuxGnu => cfg!(target_arch = "aarch64"),
                    _ => false,
                })
        {
            continue;
        }
        for mode in [LanguageMode::C90, LanguageMode::Gnu90] {
            let profile = profile.with_language_mode(mode);
            for source in [
                "int inline=1; int f(void){return inline;}",
                "int restrict=1; int f(void){return restrict;}",
                "inline int f(void){return 1;}",
                "int f(int *restrict p){return *p;}",
                "int f(void){for(int i=0;i<2;i++){}return 0;}",
                "x; extern y; typedef T; T z;",
                "f(a,b) float b; {return a+(int)b;}",
                "int f(void){return missing(1.0f);} int missing(double);",
                "int f(void){return missing(1.0f);} int missing(float);",
                "int f(void){return (missing)(1);}",
                "int f(void){{missing(1);}return missing(2);}",
                "int f(void){{missing(1);}return sizeof(&missing);}",
                "int n[sizeof(missing())]; int f(void){return missing(2);}",
                "int f(void){return missing(1);} static int missing(int x){return x;}",
                "extern int missing(int); static int missing(int x){return x;}",
                "int x=6 //**/ 2;",
                "// extension\nint x=6 //**/ 2;",
                "void f(void){(void)u\"x\";}",
                "int f(const x){return sizeof(const)+x;}",
                "typedef unsigned __int64 U; int __stdcall invoke(__int16 x){return x;} __int8 small; U large;",
            ] {
                std::fs::write(&input, source).unwrap();
                let mut command = Command::new(if gnu { &gcc } else { &clang });
                command.arg(format!("-std={mode}")).arg("-fsyntax-only");
                if !gnu {
                    command.arg(format!("--target={}", profile.target()));
                }
                let output = command.arg(&input).output().unwrap();
                let expected = toucan_test_support::compiler_acceptance(&output).unwrap();
                let analysis = parity(source, profile);
                assert_eq!(
                    analysis.is_ok(),
                    expected,
                    "{profile:?}: {source}\nToucan: {analysis:?}\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
    let source = "int f(void){return missing(1);} static int missing(int x){return x;}";
    std::fs::write(&input, source).unwrap();
    let output = Command::new(&clang)
        .args([
            "--target=x86_64-pc-windows-msvc",
            "-std=c90",
            "-O0",
            "-S",
            "-emit-llvm",
        ])
        .arg(&input)
        .args(["-o", "-"])
        .output()
        .unwrap();
    assert_eq!(toucan_test_support::compiler_acceptance(&output), Ok(true));
    let ir = String::from_utf8(output.stdout).unwrap();
    let definition = ir
        .lines()
        .find(|line| line.starts_with("define ") && line.contains("@missing("))
        .unwrap();
    assert!(!definition.contains(" internal "), "{definition}");
}

#[test]
fn implicit_calls_keep_later_noreturn_declarations_separate() {
    use toucan_semantic::checked::ExprKind;
    let source = "int before(void){return missing(1);} _Noreturn int missing(int); int after(void){return missing(2);}";
    for profile in CompilerProfile::ALL {
        for mode in [LanguageMode::C90, LanguageMode::Gnu90] {
            let analysis = parity(source, profile.with_language_mode(mode)).unwrap();
            let code = analysis.checked().unwrap();
            let mut calls = code
                .expressions()
                .filter_map(|(_, expression)| {
                    let ExprKind::Call { noreturn, .. } = expression.kind() else {
                        return None;
                    };
                    Some((
                        code.occurrence(expression.occurrence())
                            .unwrap()
                            .source()
                            .range()
                            .start,
                        *noreturn,
                    ))
                })
                .collect::<Vec<_>>();
            calls.sort_by_key(|(offset, _)| *offset);
            assert_eq!(
                calls
                    .into_iter()
                    .map(|(_, promise)| promise)
                    .collect::<Vec<_>>(),
                [false, true]
            );
        }
    }
}

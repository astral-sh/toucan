use toucan_semantic::{
    Analysis, AnalysisOptions, analyze_with_profile,
    checked::{Conversion, ExprKind, QueryEvaluation, UseContext},
};
use toucan_target::{Compiler, CompilerProfile, Target};
fn check(source: &str, profile: CompilerProfile) -> Result<Analysis, toucan_semantic::Error> {
    let plain = analyze_with_profile(source, profile, &Default::default());
    let kept = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&plain, &kept) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => assert_eq!((a.offset, &a.message), (b.offset, &b.message)),
        _ => panic!("{source}: {plain:?} {kept:?}"),
    };
    kept
}
const CASES: &[(&str, &str, bool, bool)] = &[
    (
        "atomic_source",
        "typedef int V __attribute__((vector_size(16)));V f(_Atomic(V)*p){return __builtin_convertvector(*p,V);}",
        true,
        false,
    ),
    (
        "int_float",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
F4 f(I4 x){return __builtin_convertvector(x,F4);}
"#,
        true,
        true,
    ),
    (
        "float_int",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
I4 f(F4 x){return __builtin_convertvector(x,I4);}
"#,
        true,
        true,
    ),
    (
        "widen",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
I4 f(S4 x){return __builtin_convertvector(x,I4);}
"#,
        true,
        true,
    ),
    (
        "narrow",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
S4 f(I4 x){return __builtin_convertvector(x,S4);}
"#,
        true,
        true,
    ),
    (
        "unsigned",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
U4 f(I4 x){return __builtin_convertvector(x,U4);}
"#,
        true,
        true,
    ),
    (
        "half",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
H4 f(F4 x){return __builtin_convertvector(x,H4);}
"#,
        true,
        true,
    ),
    (
        "const_result",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
F4 f(I4 x){return __builtin_convertvector(x,const F4);}
"#,
        true,
        true,
    ),
    (
        "volatile_result",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
F4 f(I4 x){return __builtin_convertvector(x,volatile F4);}
"#,
        true,
        true,
    ),
    (
        "restrict_result",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
F4 f(I4 x){return __builtin_convertvector(x,F4 restrict);}
"#,
        false,
        false,
    ),
    (
        "aligned_result",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
AI4 f(I4 x){return __builtin_convertvector(x,AI4);}
"#,
        true,
        true,
    ),
    (
        "volatile_input",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
I4 f(volatile I4*p){return __builtin_convertvector(*p,I4);}
"#,
        true,
        true,
    ),
    (
        "lvalue",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
void f(I4 x){__builtin_convertvector(x,I4)=x;}
"#,
        false,
        false,
    ),
    (
        "address",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
void f(I4 x){(void)&__builtin_convertvector(x,I4);}
"#,
        false,
        false,
    ),
    (
        "count",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
D2 f(I4 x){return __builtin_convertvector(x,D2);}
"#,
        false,
        false,
    ),
    (
        "nonvector_source",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
I4 f(int x){return __builtin_convertvector(x,I4);}
"#,
        false,
        false,
    ),
    (
        "nonvector_dest",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
int f(I4 x){return __builtin_convertvector(x,int);}
"#,
        false,
        false,
    ),
    (
        "atomic_dest",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
I4 f(I4 x){return __builtin_convertvector(x,_Atomic(I4));}
"#,
        true,
        false,
    ),
    (
        "general_type",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
F4 f(I4 x){return __builtin_convertvector(x,float __attribute__((vector_size(16))));}
"#,
        true,
        true,
    ),
    (
        "sizeof",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
_Static_assert(sizeof(__builtin_convertvector((I4){1,2,3,4},F4))==16,"size");
"#,
        true,
        true,
    ),
    (
        "constant_p",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
_Static_assert(!__builtin_constant_p(__builtin_convertvector((I4){1,2,3,4},F4)),"const");
"#,
        true,
        true,
    ),
    (
        "constant_lane",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
_Static_assert(__builtin_convertvector((I4){1,2,3,4},F4)[0]==1,"lane");
"#,
        false,
        false,
    ),
    (
        "query_sideeffects",
        r#"typedef int I4 __attribute__((vector_size(16))); typedef unsigned U4 __attribute__((vector_size(16))); typedef float F4 __attribute__((vector_size(16))); typedef short S4 __attribute__((vector_size(8))); typedef double D2 __attribute__((vector_size(16))); typedef int I2 __attribute__((vector_size(8))); typedef _Float16 H4 __attribute__((vector_size(8))); typedef I4 AI4 __attribute__((aligned(32)));
int n; enum{X=__builtin_constant_p(__builtin_convertvector((n++,(I4){1}),F4))};
"#,
        true,
        true,
    ),
];
#[test]
fn vector_conversion_constraints_match_compiler_profiles() {
    for profile in CompilerProfile::ALL {
        for &(name, source, gnu, clang) in CASES {
            assert_eq!(
                check(source, profile).is_ok(),
                if profile.target() == Target::I686UnknownLinuxGnu && source.contains("_Float16") {
                    false
                } else if profile.compiler() == Compiler::Gnu {
                    gnu
                } else {
                    clang
                },
                "{name}: {profile:?}"
            );
        }
    }
}
#[test]
fn static_conversion_has_an_explicit_frontend_limitation() {
    // Apple Clang accepts this extension, while upstream Clang 18 rejects it.
    // This is a missing evaluator feature, not a universal C constraint.
    let source = include_str!("fixtures/static_convert_vector.c");
    for profile in CompilerProfile::ALL {
        let error = check(source, profile).unwrap_err();
        if profile.target() == Target::I686UnknownLinuxGnu {
            assert!(error.message.contains("_Float16 is unavailable"));
            continue;
        }
        assert!(
            error.message.contains(
                "numeric vector conversion is not a static initializer in this compiler profile"
            ),
            "{profile:?}: {error}"
        );
    }
}
#[test]
fn retained_conversion_has_one_value_edge_and_a_written_destination() {
    let source = "typedef int I __attribute__((vector_size(16)));typedef float F __attribute__((vector_size(16)));I input(void); F f(volatile I*p){return __builtin_convertvector((input(),*p), const F);}";
    for profile in CompilerProfile::ALL {
        let a = check(source, profile).unwrap();
        let c = a.checked().unwrap();
        let conversions = c
            .expressions()
            .filter_map(|(_, e)| {
                if let ExprKind::ConvertVector { value, destination } = e.kind() {
                    Some((e, value, destination))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(conversions.len(), 1);
        let (e, value, destination) = conversions[0];
        assert_eq!(value.context(), UseContext::Value);
        assert!(
            value
                .conversions()
                .iter()
                .all(|x| x.kind() != Conversion::VectorReinterpret)
        );
        let ty = c
            .ty(c.type_use(destination.type_use).unwrap().shape())
            .unwrap();
        assert!(a.unit().qualifiers(ty).unwrap().is_const);
        assert_eq!(
            a.unit().resolve(c.ty(e.ty()).unwrap()).unwrap(),
            a.unit().resolve(ty).unwrap()
        );
        assert!(c.occurrence(destination.occurrence).is_some());
    }
}
#[test]
fn queries_and_discarded_branches_keep_evaluation_and_types_separate() {
    let source = "typedef int I __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); I input(void); void f(volatile I*p){_Static_assert(!__builtin_constant_p(__builtin_convertvector(*p,F)),\"volatile\");_Static_assert(sizeof(__builtin_convertvector(input(),F))==16,\"size\"); (void)__builtin_choose_expr(1,0,__builtin_convertvector(input(),F)); (void)_Generic(__builtin_convertvector(input(),F),F:0);}";
    for profile in CompilerProfile::ALL {
        let a = check(source, profile).unwrap();
        let c = a.checked().unwrap();
        let query = c
            .expressions()
            .find_map(|(_, e)| {
                if let ExprKind::BuiltinCall {
                    query_evaluation: Some(q),
                    ..
                } = e.kind()
                {
                    Some(q)
                } else {
                    None
                }
            })
            .unwrap();
        if profile.compiler() == Compiler::Clang {
            assert!(matches!(query, QueryEvaluation::Unevaluated(_)));
        }
    }
}
#[test]
fn checked_type_operands_do_not_redeclare_scopes() {
    let source = "typedef int I __attribute__((vector_size(16)));int f(I v){return __builtin_constant_p(__builtin_convertvector(v,__typeof__((struct S{int x;}*)0, v)));}";
    for profile in CompilerProfile::ALL {
        let a = check(source, profile).unwrap();
        assert_eq!(
            a.unit()
                .records
                .iter()
                .filter(|record| record.name.as_deref() == Some("S"))
                .count(),
            1
        );
    }
}
#[test]
#[ignore = "requires GCC and Clang; cross-target checks use Clang"]
fn compiler_type_constraints_and_native_numeric_conversions() {
    let d = tempfile::tempdir().unwrap();
    let file = d.path().join("case.c");
    for profile in CompilerProfile::ALL {
        if profile.compiler() == Compiler::Gnu
            && !((cfg!(all(target_os = "linux", target_arch = "x86_64"))
                && matches!(
                    profile.target(),
                    Target::X86_64UnknownLinuxGnu | Target::X86_64UnknownLinuxMusl
                ))
                || (cfg!(all(target_os = "linux", target_arch = "aarch64"))
                    && matches!(
                        profile.target(),
                        Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
                    )))
        {
            continue;
        }
        for &(name, source, gnu, clang) in CASES {
            std::fs::write(&file, source).unwrap();
            let mut command = if profile.compiler() == Compiler::Gnu {
                std::process::Command::new(
                    std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
                )
            } else {
                let mut c = std::process::Command::new("clang");
                c.args(["-target", profile.target().triple()]);
                c
            };
            let out = command
                .args(["-std=gnu11", "-fsyntax-only"])
                .arg(&file)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&out).unwrap(),
                if profile.target() == Target::I686UnknownLinuxGnu && source.contains("_Float16") {
                    false
                } else if profile.compiler() == Compiler::Gnu {
                    gnu
                } else {
                    clang
                },
                "{name} {profile:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
    let source = include_str!("fixtures/convert_vector.c");
    for profile in CompilerProfile::ALL {
        check(source, profile).unwrap();
    }
    std::fs::write(&file, source).unwrap();
    for cc in ["gcc", "clang"] {
        if cc == "gcc" && !cfg!(target_os = "linux") {
            continue;
        }
        for optimization in ["-O0", "-O2"] {
            let binary = d.path().join("run.exe");
            let out = std::process::Command::new(if cc == "gcc" {
                std::env::var("TOUCAN_GCC").unwrap_or_else(|_| cc.into())
            } else {
                cc.into()
            })
            .args(["-std=gnu11", optimization])
            .arg(&file)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
            assert!(
                toucan_test_support::compiler_acceptance(&out).unwrap(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let out = std::process::Command::new(binary).output().unwrap();
            assert!(
                out.status.success(),
                "{cc} {optimization}: {:?}",
                out.status.code()
            );
        }
    }
}

use toucan_semantic::{
    Analysis, AnalysisOptions, TypeKind, analyze_with_profile,
    checked::{ExprKind, QueryEvaluation, QuerySideEffects, ShuffleLane, ShuffleMask, UseContext},
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
        "identity",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,0,1,2,3));}",
        true,
        true,
    ),
    (
        "second",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,4,5,6,7));}",
        true,
        true,
    ),
    (
        "undefined",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,-1,1,-1,7));}",
        true,
        true,
    ),
    (
        "single",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,7));}",
        true,
        true,
    ),
    (
        "pair",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,0,7));}",
        true,
        true,
    ),
    (
        "odd",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,0,1,2));}",
        false,
        true,
    ),
    (
        "empty",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b));}",
        false,
        true,
    ),
    (
        "big",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,0,1,2,3,4,5,6,7));}",
        true,
        true,
    ),
    (
        "negative",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,-2));}",
        false,
        false,
    ),
    (
        "out_of_bounds",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,8));}",
        false,
        false,
    ),
    (
        "uint_max",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,4294967295u));}",
        false,
        false,
    ),
    (
        "wrap32",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,4294967296ull));}",
        false,
        false,
    ),
    (
        "ulong_max",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,18446744073709551615ull));}",
        false,
        false,
    ),
    (
        "negative_long",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,-1L));}",
        true,
        true,
    ),
    (
        "float",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,1.0));}",
        false,
        false,
    ),
    (
        "cast_float",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,(int)1.0));}",
        true,
        true,
    ),
    (
        "float_computed",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,(int)(1.0+1.0)));}",
        false,
        false,
    ),
    (
        "comma",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,(0,1)));}",
        false,
        false,
    ),
    (
        "constant_local",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,N));}",
        false,
        false,
    ),
    (
        "dynamic",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,n));}",
        false,
        false,
    ),
    (
        "index_shortcircuit",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,0&&n));}",
        false,
        false,
    ),
    (
        "index_dead_call",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,0&&g()));}",
        false,
        false,
    ),
    (
        "index_voidptr",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,b,(void*)0));}",
        true,
        false,
    ),
    (
        "mixed_unsigned",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,u,0,7));}",
        false,
        false,
    ),
    (
        "mixed_float",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,f,0,7));}",
        false,
        false,
    ),
    (
        "mixed_width",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,s,0,7));}",
        false,
        false,
    ),
    (
        "mixed_length",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,w,0,5));}",
        true,
        false,
    ),
    (
        "scalar_first",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(0,b,0));}",
        false,
        false,
    ),
    (
        "scalar_second",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\nint g(void); int run(V a,V b,W w,U u,F f,S s,int n){const int N=1; return sizeof(__builtin_shufflevector(a,0,0));}",
        false,
        false,
    ),
    (
        "result_element_type",
        "typedef int V __attribute__((vector_size(16))); typedef int W __attribute__((vector_size(8))); typedef unsigned U __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); typedef short S __attribute__((vector_size(16)));\n_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_shufflevector((V){0},(V){0},0,1)),W),\"type\");",
        true,
        true,
    ),
];
#[test]
fn inputs_indices_and_result_types_match_supported_compiler_forms() {
    for p in CompilerProfile::ALL {
        for &(name, source, gnu, clang) in CASES {
            let result = check(source, p);
            let unsupported = name == "big"
                || (p.compiler() == Compiler::Clang && name == "odd")
                || (p.compiler() == Compiler::Gnu && name == "index_voidptr");
            if unsupported {
                assert!(
                    result.unwrap_err().message.contains("unsupported"),
                    "{p:?} {name}"
                );
            } else {
                assert_eq!(
                    result.is_ok(),
                    if p.compiler() == Compiler::Gnu {
                        gnu
                    } else {
                        clang
                    },
                    "{p:?} {name}: {result:?}"
                );
            }
        }
    }
}
#[test]
fn retained_lane_plan_separates_value_operands_from_constant_indices() {
    let source = "typedef int V __attribute__((vector_size(16))); V left(void); V right(void); int f(void){return __builtin_shufflevector(left(),right(),-1,7)[1];}";
    for p in CompilerProfile::ALL {
        let a = check(source, p).unwrap();
        let code = a.checked().unwrap();
        let expr = code
            .expressions()
            .map(|(_, e)| e)
            .find(|e| matches!(e.kind(), ExprKind::ShuffleVector { .. }))
            .unwrap();
        let ExprKind::ShuffleVector { operands, mask, .. } = expr.kind() else {
            unreachable!()
        };
        assert!(operands.iter().all(|o| o.context() == UseContext::Value));
        assert_ne!(operands[0].expression(), operands[1].expression());
        let ShuffleMask::Constant(indices) = mask else {
            panic!()
        };
        assert_eq!(indices.len(), 2);
        assert_eq!(indices[0].lane(), ShuffleLane::Undefined);
        assert_eq!(indices[1].lane(), ShuffleLane::Index(7));
        assert!(
            indices
                .iter()
                .all(|i| i.operand().context() == UseContext::UnevaluatedValue)
        );
        assert!(matches!(
            code.ty(expr.ty()).unwrap().kind,
            TypeKind::Vector { lanes: 2, .. }
        ));
        assert_eq!(
            code.expressions()
                .filter(|(_, e)| matches!(e.kind(), ExprKind::Call { .. }))
                .count(),
            2
        );
    }
    let p = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    let a=check("typedef int V __attribute__((vector_size(16)));typedef signed char M __attribute__((vector_size(4))); V f(V a,M m){return __builtin_shufflevector(a,m);}",p).unwrap();
    assert!(a.checked().unwrap().expressions().any(|(_, e)| matches!(
        e.kind(),
        ExprKind::ShuffleVector {
            mask: ShuffleMask::Dynamic,
            ..
        }
    )));
}
#[test]
fn vector_identity_alignment_and_dynamic_mask_rules_follow_the_profile() {
    for p in CompilerProfile::ALL {
        let a=check("typedef int V __attribute__((vector_size(16)));typedef V A __attribute__((aligned(1))); A a; V b; typedef __typeof__(__builtin_shufflevector(a,b,0,1,2,3)) R; typedef __typeof__(__builtin_shufflevector(a,b,0,1)) S;",p).unwrap();
        assert_eq!(
            a.unit().alignment(&a.unit().typedefs["R"]).unwrap(),
            if p.compiler() == Compiler::Clang {
                1
            } else {
                16
            }
        );
        assert_eq!(a.unit().alignment(&a.unit().typedefs["S"]).unwrap(), 8);
        for (mask, ok) in [
            ("typedef short M __attribute__((vector_size(8)));", true),
            ("typedef float M __attribute__((vector_size(16)));", false),
            ("typedef short M __attribute__((vector_size(4)));", false),
        ] {
            let source = format!(
                "typedef int V __attribute__((vector_size(16))); {mask} V f(V a,M m){{return __builtin_shufflevector(a,m);}}"
            );
            assert_eq!(
                check(&source, p).is_ok(),
                p.compiler() == Compiler::Clang && ok
            );
        }
    }
    let p = CompilerProfile::new(Target::Aarch64UnknownLinuxGnu, Compiler::Gnu).unwrap();
    let a=check("typedef __Float32x4_t N;typedef float V __attribute__((vector_size(16))); N n; V v; typedef __typeof__(__builtin_shufflevector(n,v,0,1,2,3)) R;",p).unwrap();
    assert!(matches!(
        a.unit().resolve(&a.unit().typedefs["R"]).unwrap().kind,
        TypeKind::Vector {
            kind: toucan_semantic::VectorKind::Gnu,
            ..
        }
    ));
}
#[test]
fn queries_keep_the_clang_syntactic_gate_and_unevaluated_lane_bounds() {
    for p in CompilerProfile::ALL {
        for (index, effects) in [
            ("0", QuerySideEffects::Absent),
            ("__builtin_constant_p(n++)", QuerySideEffects::Present),
            ("sizeof(int(*)[bump()])", QuerySideEffects::Absent),
        ] {
            let source = format!(
                "typedef signed char V __attribute__((vector_size(16))); V a; int bump(void); unsigned long f(int n){{return __builtin_object_size((int(*)[bump()])(unsigned long)__builtin_shufflevector(a,a,{index})[0],0);}}"
            );
            let a = check(&source, p).unwrap();
            let code = a.checked().unwrap();
            let query = code
                .expressions()
                .find_map(|(_, e)| match e.kind() {
                    ExprKind::BuiltinCall {
                        builtin: toucan_semantic::checked::Builtin::ObjectSize,
                        query_evaluation,
                        ..
                    } => *query_evaluation,
                    _ => None,
                })
                .unwrap();
            if p.compiler() == Compiler::Clang && effects == QuerySideEffects::Absent {
                assert_eq!(
                    query,
                    QueryEvaluation::ClangFallback {
                        side_effects: effects
                    }
                );
            } else {
                assert!(matches!(query, QueryEvaluation::Unevaluated(_)));
            }
        }
    }
}
#[test]
fn checking_cached_calls_does_not_redeclare_written_scopes() {
    for p in CompilerProfile::ALL {
        check("typedef int V __attribute__((vector_size(16))); int f(void){return sizeof(__builtin_shufflevector(({struct S{int x;}; (V){0};}), (V){0},sizeof(struct T{char x;})));}",p).unwrap();
    }
}

const EXECUTION_SOURCE: &str = r#"typedef int V __attribute__((vector_size(16))); typedef signed char M __attribute__((vector_size(4))); typedef int W __attribute__((vector_size(8))); typedef long long L __attribute__((vector_size(16)));
int printf(const char*,...); int left_count, right_count;
V left(void){left_count++;return (V){10,20,30,40};} V right(void){right_count++;return (V){50,60,70,80};}
int main(void){V a=__builtin_shufflevector(left(),right(),0,0,0,0); if(left_count!=1||right_count!=1||a[0]!=10)return 1;
V b=__builtin_shufflevector(left(),right(),-1,7,-1,2);if(left_count!=2||right_count!=2||b[1]!=80||b[3]!=30)return 2;
#ifdef __clang__
V c=__builtin_shufflevector((V){10,20,30,40},(V){-1,-2,4,7}); if(c[0]!=40||c[1]!=30||c[2]!=10||c[3]!=40)return 3;
V d=__builtin_shufflevector((V){10,20,30,40},(M){-1,-2,4,7});if(d[0]!=40||d[1]!=30||d[2]!=10||d[3]!=40)return 4;
W e=__builtin_shufflevector((W){10,20},(L){0x100000001ll,-2});if(e[0]!=20||e[1]!=10)return 5;
#endif
printf("%d %d %d %d\n",left_count,right_count,b[1],b[3]);return 0;}
"#;

#[test]
#[ignore = "requires native C compilers; Clang also checks all five target profiles"]
fn compiler_constraints_and_native_operand_evaluation() {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("shuffle.c");
    let output = d.path().join("shuffle.s");
    for p in CompilerProfile::ALL {
        let gnu = p.compiler() == Compiler::Gnu;
        let host_gnu = (cfg!(all(target_os = "linux", target_arch = "x86_64"))
            && p.target() == Target::X86_64UnknownLinuxGnu)
            || (cfg!(all(target_os = "linux", target_arch = "aarch64"))
                && p.target() == Target::Aarch64UnknownLinuxGnu);
        let mut cc = if gnu {
            if !host_gnu {
                continue;
            }
            std::process::Command::new(std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()))
        } else {
            let mut c = std::process::Command::new("clang");
            c.args(["-target", p.target().triple()]);
            c
        };
        cc.args(["-std=gnu11", "-O0", "-S"])
            .arg(&input)
            .arg("-o")
            .arg(&output);
        for &(name, source, gnu_ok, clang_ok) in CASES {
            std::fs::write(&input, source).unwrap();
            let out = cc.output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&out).unwrap(),
                if gnu { gnu_ok } else { clang_ok },
                "{p:?} {name}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
    std::fs::write(&input, EXECUTION_SOURCE).unwrap();
    for compiler in ["gcc", "clang"] {
        if compiler == "gcc" && !cfg!(target_os = "linux") {
            continue;
        }
        let cc = if compiler == "gcc" {
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into())
        } else {
            compiler.into()
        };
        for optimization in ["-O0", "-O2"] {
            let exe = d.path().join("evaluate");
            let out = std::process::Command::new(&cc)
                .args(["-std=gnu11", optimization])
                .arg(&input)
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                toucan_test_support::compiler_acceptance(&out).unwrap(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let out = std::process::Command::new(&exe).output().unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.stdout, b"2 2 80 30\n");
        }
    }
}

#[test]
fn dead_value_operands_suppress_feature_uses_but_keep_type_checks() {
    for profile in CompilerProfile::ALL.into_iter().filter(|p| {
        matches!(
            p.target(),
            Target::X86_64UnknownLinuxGnu | Target::X86_64AppleDarwin | Target::X86_64PcWindowsMsvc
        )
    }) {
        for dead in [true, false] {
            let guard = if dead { "if(0)" } else { "" };
            let source = format!(
                "typedef int V __attribute__((vector_size(16))); __attribute__((target(\"no-mmx\"))) int f(void){{{guard}return __builtin_shufflevector((__builtin_ia32_emms(),(V){{0}}),(V){{0}},0)[0];return 0;}}"
            );
            assert_eq!(
                check(&source, profile).is_ok(),
                dead || profile.compiler() == Compiler::Gnu,
                "{profile:?} {source}"
            );
        }
    }
}

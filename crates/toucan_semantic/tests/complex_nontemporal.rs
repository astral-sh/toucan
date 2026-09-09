use toucan_semantic::{
    Analysis, AnalysisOptions, FloatKind, TypeKind, analyze_with_profile,
    checked::{Builtin, Conversion, ExprKind, NontemporalOperation as Op, QueryEvaluation},
};
use toucan_target::{Compiler, CompilerProfile};
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
const SOURCE: &str = "void f(_Complex double*p,double*real,_Complex float z){__builtin_nontemporal_store(1.5,p);__builtin_nontemporal_store(z,p);__builtin_nontemporal_store(z,real);_Complex double x=__builtin_nontemporal_load(p);__builtin_nontemporal_store(__builtin_nontemporal_load(p),p);_Bool b=__builtin_nontemporal_load(p);}";
const BINARY128_SOURCE: &str = "typedef __typeof__(1.0Q) Q; typedef __typeof__(1.0Qi) C; void f(const volatile Q *p, C *z){Q real=__builtin_nontemporal_load(p);C value=__builtin_nontemporal_load(z);__builtin_nontemporal_store(real,z);__builtin_nontemporal_store(value,p);}";

#[test]
fn binary128_accesses_preserve_real_and_complex_identity() {
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let analysis = check(BINARY128_SOURCE, profile).unwrap();
        let code = analysis.checked().unwrap();
        let types = code
            .expressions()
            .filter_map(|(_, expression)| {
                if let ExprKind::BuiltinCall {
                    builtin: Builtin::Nontemporal(Op::Load),
                    ..
                } = expression.kind()
                {
                    Some(code.ty(expression.ty()).unwrap().kind.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            types,
            [
                TypeKind::Float(FloatKind::FLOAT128),
                TypeKind::Complex(FloatKind::FLOAT128)
            ]
        );
    }
}

#[test]
fn complex_values_use_existing_assignment_conversions_and_access_identity() {
    for profile in CompilerProfile::ALL {
        let result = check(SOURCE, profile);
        if profile.compiler() == Compiler::Gnu {
            assert!(result.is_err());
            continue;
        }
        let a = result.unwrap();
        let code = a.checked().unwrap();
        let stores = code
            .expressions()
            .filter_map(|(_, e)| match e.kind() {
                ExprKind::BuiltinCall {
                    builtin: Builtin::Nontemporal(Op::Store),
                    arguments,
                    ..
                } => Some(arguments),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(stores.len(), 4);
        for (index, store) in stores[..3].iter().enumerate() {
            assert!(
                store[0]
                    .conversions()
                    .iter()
                    .any(|step| step.kind() == Conversion::Assignment)
            );
            let destination = &code.ty(store[0].effective_type()).unwrap().kind;
            assert_eq!(
                destination,
                &if index == 2 {
                    TypeKind::Float(FloatKind::Double)
                } else {
                    TypeKind::Complex(FloatKind::Double)
                }
            );
        }
        let loads = code
            .expressions()
            .filter(|(_, e)| {
                matches!(
                    e.kind(),
                    ExprKind::BuiltinCall {
                        builtin: Builtin::Nontemporal(Op::Load),
                        ..
                    }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(loads.len(), 3);
        for (_, load) in loads {
            assert!(matches!(
                code.ty(load.ty()).unwrap().kind,
                TypeKind::Complex(FloatKind::Double)
            ));
        }
    }
}
#[test]
fn complex_memory_queries_remain_unevaluated_and_results_are_unqualified() {
    let source = "int bound(void);void f(const volatile _Complex double*p){_Static_assert(sizeof(__builtin_nontemporal_load(p))==sizeof(*p),\"size\");_Static_assert(_Generic(__builtin_nontemporal_load(p),_Complex double:1,default:0),\"type\");_Static_assert(!__builtin_constant_p((double)__builtin_nontemporal_load(p)),\"query\");(void)__builtin_object_size((int(*)[bound()])(unsigned long long)(double)__builtin_nontemporal_load(p),0);}";
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let a = check(source, profile).unwrap();
        let code = a.checked().unwrap();
        for (_, e) in code.expressions() {
            if let ExprKind::BuiltinCall {
                builtin: Builtin::Nontemporal(Op::Load),
                ..
            } = e.kind()
            {
                let ty = code.ty(e.ty()).unwrap();
                assert!(!ty.qualifiers.is_const && !ty.qualifiers.is_volatile);
            }
            if let ExprKind::BuiltinCall {
                builtin: Builtin::ObjectSize,
                query_evaluation,
                ..
            } = e.kind()
            {
                assert!(matches!(
                    query_evaluation,
                    Some(QueryEvaluation::Unevaluated(_))
                ));
            }
        }
        let error = check("int f(_Complex double *p){return __builtin_constant_p(__builtin_nontemporal_load(p));}", profile).unwrap_err();
        assert!(
            error
                .message
                .contains("complex constant-query fallback evaluation is unsupported")
        );
        assert!(
            check(
                "void f(_Atomic(_Complex double)*p){(void)__builtin_nontemporal_load(p);}",
                profile
            )
            .is_err()
        );
    }
}
#[test]
#[ignore = "requires Clang for all five targets; checks source constraints without invoking defective lowering paths"]
fn native_complex_source_constraints() {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("complex.c");
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let sources = ["float", "double", "long double"].map(|ty| format!(
            "_Complex {ty} load(const volatile _Complex {ty}*p){{return __builtin_nontemporal_load(p);}}void store(_Complex {ty}*p){{__builtin_nontemporal_store(1.5,p);}}"
        ));
        for source in sources
            .into_iter()
            .chain(std::iter::once(BINARY128_SOURCE.to_owned()))
        {
            check(&source, profile).unwrap();
            std::fs::write(&input, source).unwrap();
            let output = std::process::Command::new("clang")
                .args([
                    "-target",
                    profile.target().triple(),
                    "-std=gnu11",
                    "-fsyntax-only",
                ])
                .arg(&input)
                .output()
                .unwrap();
            assert!(
                toucan_test_support::compiler_acceptance(&output).unwrap(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn native_lvalue_stores_and_discarded_loads_work_with_c_callers() {
    let source = include_str!("fixtures/complex_nontemporal/access.c");
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        check(&source.replace("__ATOMIC_SEQ_CST", "5"), profile).unwrap();
    }
    let d = tempfile::tempdir().unwrap();
    let access = d.path().join("access.c");
    let caller = d.path().join("caller.c");
    std::fs::write(&access, source).unwrap();
    std::fs::write(
        &caller,
        include_str!("fixtures/complex_nontemporal/caller.c"),
    )
    .unwrap();
    for optimization in ["-O0", "-O2"] {
        let object = d.path().join("access.o");
        let out = std::process::Command::new("clang")
            .args(["-std=gnu11", optimization, "-c"])
            .arg(&access)
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        for cc in ["clang", "gcc"] {
            if cc == "gcc" && !cfg!(target_os = "linux") {
                continue;
            }
            let cc = if cc == "gcc" {
                std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into())
            } else {
                cc.into()
            };
            let binary = d.path().join("caller.exe");
            let out = std::process::Command::new(cc)
                .args(["-std=gnu11", optimization])
                .arg(&caller)
                .arg(&object)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let out = std::process::Command::new(&binary).output().unwrap();
            assert!(
                out.status.success(),
                "{:?} {}",
                out.status.code(),
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

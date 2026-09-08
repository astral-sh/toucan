use toucan_semantic::{
    Analysis, AnalysisOptions, FunctionType, Type, TypeKind, analyze_with_profile,
    checked::ExprKind,
};
use toucan_target::{Compiler, CompilerProfile, Target};
fn clang() -> CompilerProfile {
    CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap()
}
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
    }
    kept
}
fn function<'a>(a: &'a Analysis, ty: &'a Type) -> &'a FunctionType {
    let ty = a.unit().resolve(ty).unwrap();
    match &ty.kind {
        TypeKind::Function(f) => f,
        TypeKind::Pointer(t) => function(a, t),
        _ => panic!("{ty:?}"),
    }
}
fn positions<'a>(a: &'a Analysis, f: &FunctionType) -> &'a [u32] {
    f.parameter_contracts.map_or(&[], |id| {
        &a.unit().parameter_contracts(id).unwrap().no_escape
    })
}
const TYPES: &str = "typedef void N(int*__attribute__((noescape)));typedef void P(int*);";
#[test]
fn parameter_adjustment_and_nominal_contracts() {
    for profile in CompilerProfile::ALL {
        let source = "void a(int p[4]__attribute__((noescape)),void cb(void)__attribute__((noescape)),int x __attribute__((noescape))); void b(int*__attribute__((noescape)) p); void c(int(*__attribute__((noescape))p));";
        let a = check(source, profile).unwrap();
        assert_eq!(
            positions(&a, function(&a, &a.unit().declarations[0].ty)),
            if profile.compiler() == Compiler::Clang {
                &[0, 1][..]
            } else {
                &[]
            }
        );
        for d in &a.unit().declarations[1..] {
            assert_eq!(
                positions(&a, function(&a, &d.ty)),
                if profile.compiler() == Compiler::Clang {
                    &[0][..]
                } else {
                    &[]
                }
            );
        }
        assert_eq!(a.checked().unwrap().noescape_attributes().len(), 5);
        a.unit().validate_parameter_contracts().unwrap();
    }
}
#[test]
fn direct_strengthening_crossing_and_nested_contracts() {
    for profile in CompilerProfile::ALL {
        for suffix in [
            "void f(N*n,P*p){p=n;}",
            "void f(N***n,P***p){n=p;p=n;}",
            "void f(N**n,P**p){p=n;}",
            "typedef void X(N*);typedef void Y(P*);void f(X*x,Y*y){x=y;y=x;}",
            "N*f(P*p){return (N*)p;}",
            "_Static_assert(__builtin_types_compatible_p(N,P),\"compatible\");",
        ] {
            check(&format!("{TYPES}{suffix}"), profile).unwrap();
        }
        for suffix in [
            "void f(N**n,P**p){n=p;}",
            "void f(N*n,P*p){n=p;}",
            "N*f(int c,N*n,P*p){return c?n:p;}",
            "void f(N*);void g(P*p){f(p);}",
            "typedef void F(int*__attribute__((noescape)));typedef void F(int*);",
        ] {
            let result = check(&format!("{TYPES}{suffix}"), profile);
            assert_eq!(
                result.is_err(),
                profile.compiler() == Compiler::Clang,
                "{profile:?} {suffix} {result:?}"
            );
        }
        let crossing = "typedef void A(int*__attribute__((noescape)),int*);typedef void B(int*,int*__attribute__((noescape)));void f(A*a,B*b){a=b;b=a;}";
        check(crossing, profile).unwrap();
    }
}
#[test]
fn written_annotations_and_declaration_time_calls() {
    let source = "void f(int*p __attribute__((noescape)));void g(int*p){f(p);}void f(int*p);void h(int*p){f(p);}void f(int*p __attribute__((noescape))){*p=1;}";
    let a = check(source, clang()).unwrap();
    assert!(positions(&a, function(&a, &a.unit().declarations[0].ty)).is_empty());
    let code = a.checked().unwrap();
    let calls: Vec<_> = code
        .expressions()
        .filter_map(|(_, e)| {
            if let ExprKind::Call { callee, .. } = e.kind() {
                Some(
                    positions(&a, function(&a, code.ty(callee.effective_type()).unwrap())).to_vec(),
                )
            } else {
                None
            }
        })
        .collect();
    assert_eq!(calls, [vec![0], vec![]]);
    assert_eq!(code.noescape_attributes().len(), 2);
    for attribute in code.noescape_attributes() {
        assert_eq!(attribute.parameters().len(), 1);
        assert!(attribute.parameters()[0].applies());
    }
}
#[test]
fn intersection_is_sparse_and_deduplicated() {
    let source = "typedef void A(int*__attribute__((noescape)),int*__attribute__((noescape)),int*);typedef void B(int*,int*__attribute__((noescape)),int*__attribute__((noescape)));void f(int c,A*a,B*b){__typeof__(c?a:b) p=c?a:b;p(0,0,0);}";
    let a = check(source, clang()).unwrap();
    assert_eq!(a.unit().parameter_contracts.len(), 3);
    assert_eq!(a.unit().parameter_contracts[2].no_escape, [1]);
    assert_eq!(std::mem::size_of::<FunctionType>(), 72);
    assert_eq!(std::mem::size_of::<toucan_semantic::Parameter>(), 64);
}
#[test]
fn ignored_subjects_and_arity() {
    for profile in CompilerProfile::ALL {
        for source in [
            "int*p __attribute__((noescape));",
            "struct S{int*p __attribute__((noescape));};",
            "typedef int*P __attribute__((noescape));",
            "void f(void) __attribute__((noescape(1)));",
            "void f(_Atomic(int*)p __attribute__((noescape)));",
            "void f(void __attribute__((noescape)));",
            "void f(void){int*p __attribute__((noescape));}",
        ] {
            check(source, profile).unwrap();
        }
        for source in [
            "void f(int*p __attribute__((noescape(1))));",
            "void f(int p __attribute__((noescape(1))));",
        ] {
            assert_eq!(
                check(source, profile).is_err(),
                profile.compiler() == Compiler::Clang
            );
        }
    }
}
#[test]
fn old_style_parameter_promise_does_not_create_a_prototype() {
    let a = check(
        "void f(p) int*p __attribute__((noescape));{}void g(int*p){f(p);}",
        clang(),
    )
    .unwrap();
    assert!(!function(&a, &a.unit().declarations[0].ty).prototype);
    assert!(positions(&a, function(&a, &a.unit().declarations[0].ty)).is_empty());
    assert!(a.checked().unwrap().noescape_attributes()[0].parameters()[0].applies());
}

#[test]
fn visible_scopes_control_contract_merging() {
    let n = "void f(int*p __attribute__((noescape)));";
    let p = "void f(int*p);";
    for (source, expected) in [
        (
            format!("{n}void g(int*p){{{p}f(p);}}void h(int*p){{f(p);}}"),
            vec![vec![], vec![0]],
        ),
        (
            format!("{p}void g(int*p){{{n}f(p);}}void h(int*p){{f(p);}}"),
            vec![vec![], vec![]],
        ),
        (
            format!("void g(int*p){{{p}f(p);}}{n}void h(int*p){{f(p);}}"),
            vec![vec![], vec![0]],
        ),
        (
            format!("void g(int*p){{{p}f(p);}}void h(int*p){{{n}f(p);}}"),
            vec![vec![], vec![0]],
        ),
        (
            format!("void g(int*p){{{n}{{{p}f(p);}}f(p);}}"),
            vec![vec![], vec![0]],
        ),
        (
            format!("void g(int*p){{{p}{{{n}f(p);}}f(p);}}"),
            vec![vec![], vec![]],
        ),
    ] {
        let a = check(&source, clang()).unwrap();
        let code = a.checked().unwrap();
        let calls: Vec<_> = code
            .expressions()
            .filter_map(|(_, e)| {
                if let ExprKind::Call { callee, .. } = e.kind() {
                    Some(
                        positions(&a, function(&a, code.ty(callee.effective_type()).unwrap()))
                            .to_vec(),
                    )
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(calls, expected, "{source}");
    }
}
#[test]
fn old_style_body_signature_keeps_only_effective_entry_promises() {
    for (prefix, expected) in [
        ("", &[0][..]),
        ("void f(int*);", &[][..]),
        ("void f(int*__attribute__((noescape)));", &[0][..]),
    ] {
        let a = check(
            &format!("{prefix}void f(p) int*p __attribute__((noescape));{{}}"),
            clang(),
        )
        .unwrap();
        let code = a.checked().unwrap();
        let body = code.bodies().next().unwrap().1;
        assert_eq!(
            positions(&a, function(&a, code.ty(body.signature()).unwrap())),
            expected,
            "{prefix} {:?}",
            code.ty(body.signature()).unwrap()
        );
    }
}
#[test]
fn caller_built_contracts_are_checked_before_queries() {
    use toucan_semantic::{ParameterContracts, ParameterContractsId, evaluate_integer};
    let a = check("void f(int*__attribute__((noescape)));", clang()).unwrap();
    let mut u = a.unit().clone();
    let TypeKind::Function(f) = &mut u.declarations[0].ty.kind else {
        panic!()
    };
    f.parameter_contracts = ParameterContractsId::new(9);
    assert!(
        evaluate_integer(&u, "1")
            .unwrap_err()
            .message
            .contains("invalid parameter-contract ID")
    );
    let mut u = a.unit().clone();
    u.parameter_contracts[0] = ParameterContracts { no_escape: vec![1] };
    assert!(
        u.validate_parameter_contracts()
            .unwrap_err()
            .message
            .contains("outside")
    );
    u.parameter_contracts[0] = ParameterContracts {
        no_escape: vec![0, 0],
    };
    assert!(
        u.validate_parameter_contracts()
            .unwrap_err()
            .message
            .contains("sorted unique")
    );
}

const ORACLE_CASES: &[(&str, bool)] = &[
    ("void f(__attribute__((noescape)) int*p);", true),
    ("void f(int*p __attribute__((noescape)));", true),
    ("void f(int*__attribute__((noescape)) p);", true),
    (
        "void f(int p[static const 4]__attribute__((noescape)));",
        true,
    ),
    ("void f(void p(void)__attribute__((noescape)));", true),
    ("void f(int p __attribute__((noescape)));", true),
    ("void f(_Atomic(int*)p __attribute__((noescape)));", true),
    ("void f(void)__attribute__((noescape(1)));", true),
    ("void f(int*p __attribute__((noescape(1))));", false),
    ("void f(int p __attribute__((noescape(1))));", false),
    (
        "typedef void N(int*__attribute__((noescape)));typedef void P(int*);void f(N*n,P*p){n=p;}",
        false,
    ),
    (
        "typedef void N(int*__attribute__((noescape)));typedef void P(int*);void f(N*n,P*p){p=n;}",
        true,
    ),
    (
        "typedef void N(int*__attribute__((noescape)));typedef void P(int*);void f(N**n,P**p){n=p;}",
        false,
    ),
    (
        "typedef void N(int*__attribute__((noescape)));typedef void P(int*);typedef void X(N*);typedef void Y(P*);void f(X*x,Y*y){x=y;}",
        true,
    ),
    (
        "typedef void N(int*__attribute__((noescape)));typedef void P(int*);N*f(P*p){return (N*)p;}",
        true,
    ),
    (
        "typedef void A(int*__attribute__((noescape)),int*);typedef void B(int*,int*__attribute__((noescape)));void f(A*a,B*b){a=b;}",
        true,
    ),
    (
        "typedef void F(int*__attribute__((noescape)));typedef void F(int*);",
        false,
    ),
    ("int*q;void f(int*p __attribute__((noescape))){q=p;}", true),
    ("int*f(int*p __attribute__((noescape))){return p;}", true),
];
#[test]
#[ignore = "requires Clang and GNU GCC; checks five Clang targets and the native GNU target"]
fn native_source_constraints_and_nocapture_lowering() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("case.c");
    let llvm = dir.path().join("case.ll");
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        for &(source, accepted) in ORACLE_CASES {
            std::fs::write(&input, source).unwrap();
            let out = std::process::Command::new("clang")
                .args([
                    "-target",
                    profile.target().triple(),
                    "-std=gnu11",
                    "-fsyntax-only",
                ])
                .arg(&input)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&out).unwrap(),
                accepted,
                "{profile:?} {source}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(
                check(source, profile).is_ok(),
                accepted,
                "{profile:?} {source}"
            );
        }
        for to in 0..4 {
            for from in 0..4 {
                let declaration = |name, mask| {
                    format!(
                        "typedef void {name}(int*{}){};",
                        if mask & 1 != 0 {
                            "__attribute__((noescape))"
                        } else {
                            ""
                        },
                        if mask & 2 != 0 {
                            "__attribute__((noreturn))"
                        } else {
                            ""
                        }
                    )
                };
                let source = format!(
                    "{}{}void f(A*a,B*b){{a=b;}}",
                    declaration("A", to),
                    declaration("B", from)
                );
                std::fs::write(&input, &source).unwrap();
                let out = std::process::Command::new("clang")
                    .args([
                        "-target",
                        profile.target().triple(),
                        "-std=gnu11",
                        "-fsyntax-only",
                    ])
                    .arg(&input)
                    .output()
                    .unwrap();
                let rejected = to != from && (from & to) == from;
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&out).unwrap(),
                    !rejected,
                    "{profile:?} {source}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
        }
        let source = "void f(int*p __attribute__((noescape)));void a(int*p){f(p);}void f(int*p);void b(int*p){f(p);}";
        std::fs::write(&input, source).unwrap();
        let out = std::process::Command::new("clang")
            .args([
                "-target",
                profile.target().triple(),
                "-std=gnu11",
                "-O0",
                "-S",
                "-emit-llvm",
            ])
            .arg(&input)
            .arg("-o")
            .arg(&llvm)
            .output()
            .unwrap();
        assert!(
            toucan_test_support::compiler_acceptance(&out).unwrap(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let output = std::fs::read_to_string(&llvm).unwrap();
        let calls: Vec<_> = output
            .lines()
            .filter(|line| line.contains("call void @f("))
            .collect();
        assert_eq!(calls.len(), 2);
        assert!(calls[0].contains("nocapture"));
        assert!(!calls[1].contains("nocapture"));
    }
    if cfg!(target_os = "linux") {
        let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
        for &(source, _) in ORACLE_CASES {
            std::fs::write(&input, source).unwrap();
            let out = std::process::Command::new(&gcc)
                .args(["-std=gnu11", "-fsyntax-only"])
                .arg(&input)
                .output()
                .unwrap();
            assert!(
                toucan_test_support::compiler_acceptance(&out).unwrap(),
                "{source}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[test]
fn noreturn_crossing_promises_and_c11_spelling() {
    for profile in CompilerProfile::ALL {
        for to in 0..4 {
            for from in 0..4 {
                let declaration = |name, mask| {
                    format!(
                        "typedef void {name}(int*{}){};",
                        if mask & 1 != 0 {
                            "__attribute__((noescape))"
                        } else {
                            ""
                        },
                        if mask & 2 != 0 {
                            "__attribute__((noreturn))"
                        } else {
                            ""
                        }
                    )
                };
                let source = format!(
                    "{}{}void f(A*a,B*b){{a=b;}}",
                    declaration("A", to),
                    declaration("B", from)
                );
                let rejected =
                    profile.compiler() == Compiler::Clang && to != from && (from & to) == from;
                assert_eq!(
                    check(&source, profile).is_err(),
                    rejected,
                    "{profile:?} {source}"
                );
            }
        }
        let source = "_Noreturn void g(int*);typedef void F(int*__attribute__((noescape)));void f(F*p){p=g;}";
        assert_eq!(
            check(source, profile).is_err(),
            profile.compiler() == Compiler::Clang
        );
        check("void g(int*)__attribute__((noreturn));typedef void F(int*__attribute__((noescape)));void f(F*p){p=g;}",profile).unwrap();
    }
}
#[test]
fn noreturn_declarations_union_but_conditionals_intersect() {
    let a=check("void f(int*)__attribute__((noreturn));void f(int*);typedef void N(int*)__attribute__((noreturn));typedef void P(int*);void g(int c,N*n,P*p){(c?n:p)(0);}",clang()).unwrap();
    assert!(function(&a, &a.unit().declarations[0].ty).noreturn);
    let code = a.checked().unwrap();
    for (_, expression) in code.expressions() {
        if let ExprKind::Call { callee, .. } = expression.kind() {
            assert!(!function(&a, code.ty(callee.effective_type()).unwrap()).noreturn);
        }
    }
    assert!(
        check(
            "typedef void F(int*)__attribute__((noreturn));typedef void F(int*);",
            clang()
        )
        .is_err()
    );
    let a = check("_Noreturn void f(void);", clang()).unwrap();
    assert!(!function(&a, &a.unit().declarations[0].ty).noreturn);
}

#[test]
fn noreturn_wrappers_and_returns_twice_scope_remain_explicit() {
    let a = check(
        "typedef int F(void);typedef F *P;typedef P Q __attribute__((noreturn));Q callback;",
        clang(),
    )
    .unwrap();
    assert!(function(&a, a.unit().typedefs.get("Q").unwrap()).noreturn);
    let error = check(
        "typedef void F(void)__attribute__((noreturn));F f __attribute__((returns_twice));",
        clang(),
    )
    .unwrap_err();
    assert!(
        error
            .message
            .contains("combining returns_twice and noreturn is unsupported")
    );
}

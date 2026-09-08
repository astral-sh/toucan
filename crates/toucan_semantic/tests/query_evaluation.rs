use toucan_semantic::checked::{
    Binary, BoundEvaluation, Builtin, ExprKind, QueryEvaluation, QuerySideEffects,
    QuerySuppression, TypeOperandEvaluation, Unary, UseContext,
};
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

fn options() -> AnalysisOptions {
    AnalysisOptions {
        retain_code: true,
        ..Default::default()
    }
}

fn gnu(target: Target) -> bool {
    matches!(
        target,
        Target::X86_64UnknownLinuxGnu
            | Target::X86_64UnknownLinuxMusl
            | Target::Aarch64UnknownLinuxGnu
            | Target::Aarch64UnknownLinuxMusl
    )
}

#[test]
fn query_gates_preserve_conversions_and_explicit_uncertainty() {
    use QueryEvaluation::{ClangFallback, Unevaluated};
    use QuerySideEffects::{Absent, Unresolved};
    use QuerySuppression::{NonNumericConstantQuery, OrdinarySideEffects};
    for (operand, clang) in [
        (
            "__builtin_inff() + sizeof(int[a++])",
            ClangFallback {
                side_effects: Absent,
            },
        ),
        (
            "__builtin_nan(\"1\") + sizeof(int[a++])",
            ClangFallback {
                side_effects: Absent,
            },
        ),
        (
            "__builtin_nans((const char *)p++) + sizeof(int[a++])",
            Unevaluated(OrdinarySideEffects),
        ),
        (
            "__builtin___snprintf_chk((char *)p, 1, 0, 1, \"%d\", n) + sizeof(int[a++])",
            Unevaluated(OrdinarySideEffects),
        ),
        (
            "sizeof(int[n++])",
            ClangFallback {
                side_effects: Absent,
            },
        ),
        (
            "0 && sizeof(int[n++])",
            ClangFallback {
                side_effects: Absent,
            },
        ),
        (
            "n ? sizeof(int[a++]) : sizeof(int[b++])",
            ClangFallback {
                side_effects: Absent,
            },
        ),
        (
            "sizeof *(p++)",
            ClangFallback {
                side_effects: Absent,
            },
        ),
        (
            "sizeof(typeof(*(int (*)[n++])0))",
            ClangFallback {
                side_effects: Absent,
            },
        ),
        (
            "__builtin_constant_p(sizeof(int[n++]))",
            ClangFallback {
                side_effects: Absent,
            },
        ),
        (
            "__builtin_constant_p(n++) + sizeof(int[a++])",
            Unevaluated(OrdinarySideEffects),
        ),
        ("n++ + sizeof(int[a++])", Unevaluated(OrdinarySideEffects)),
        (
            "0 ? n++ : sizeof(int[a++])",
            Unevaluated(OrdinarySideEffects),
        ),
        (
            "volatile_value + sizeof(int[a++])",
            Unevaluated(OrdinarySideEffects),
        ),
        ("(int (*)[n++])0", Unevaluated(NonNumericConstantQuery)),
        (
            "helper() + sizeof(int[a++])",
            ClangFallback {
                side_effects: Unresolved,
            },
        ),
        (
            "pure_helper() + sizeof(int[a++])",
            ClangFallback {
                side_effects: Unresolved,
            },
        ),
        (
            "((int){0}) + sizeof(int[a++])",
            ClangFallback {
                side_effects: Unresolved,
            },
        ),
        (
            "({ n; }) + sizeof(int[a++])",
            ClangFallback {
                side_effects: Unresolved,
            },
        ),
        (
            "_Generic(volatile_value, int: sizeof(int[a++]), default: n++)",
            ClangFallback {
                side_effects: Absent,
            },
        ),
    ] {
        let source = format!(
            "int helper(void); int pure_helper(void) __attribute__((pure)); int f(int n, int a, int b, int (*p)[n]) {{volatile int volatile_value=0; return __builtin_constant_p({operand});}}"
        );
        for target in Target::ALL {
            let plain =
                analyze(&source, target).unwrap_or_else(|e| panic!("{target}: {operand}: {e}"));
            let retained = analyze_with_options(&source, target, &options()).unwrap();
            assert_eq!(format!("{plain:?}"), format!("{:?}", retained.unit()));
            let code = retained.checked().unwrap();
            let (_, expression) = code
                .expressions()
                .rfind(|(_, expr)| {
                    matches!(
                        expr.kind(),
                        ExprKind::BuiltinCall {
                            builtin: Builtin::ConstantQuery,
                            ..
                        }
                    )
                })
                .unwrap();
            let ExprKind::BuiltinCall {
                arguments,
                query_evaluation,
                ..
            } = expression.kind()
            else {
                panic!()
            };
            let expected = if gnu(target) {
                Unevaluated(QuerySuppression::GnuProfile)
            } else {
                clang
            };
            assert_eq!(*query_evaluation, Some(expected), "{target}: {operand}");
            assert_eq!(
                arguments[0].context(),
                if matches!(expected, ClangFallback { .. }) {
                    UseContext::CompilerQuery
                } else {
                    UseContext::UnevaluatedValue
                }
            );
        }
    }
}

#[test]
fn conditional_and_existing_vla_operands_remain_in_the_plan() {
    let source = "int f(int n, int a, int b, int (*p)[n]) {return __builtin_constant_p(n ? sizeof *(p++) : sizeof(typeof(*(int (*)[a++])0)));}";
    let analysis = analyze_with_options(source, Target::X86_64AppleDarwin, &options()).unwrap();
    let code = analysis.checked().unwrap();
    let (_, expression) = code
        .expressions()
        .find(|(_, expr)| {
            matches!(
                expr.kind(),
                ExprKind::BuiltinCall {
                    builtin: Builtin::ConstantQuery,
                    ..
                }
            )
        })
        .unwrap();
    let ExprKind::BuiltinCall { arguments, .. } = expression.kind() else {
        panic!()
    };
    let ExprKind::Conditional {
        condition,
        then_value,
        else_value,
    } = code.expression(arguments[0].expression()).unwrap().kind()
    else {
        panic!()
    };
    assert_eq!(condition.context(), UseContext::Value);
    let ExprKind::SizeOfValue {
        operand,
        variable: true,
    } = code.expression(then_value.expression()).unwrap().kind()
    else {
        panic!()
    };
    assert!(matches!(
        code.expression(operand.expression()).unwrap().kind(),
        ExprKind::Unary {
            operator: Unary::Indirection,
            ..
        }
    ));
    assert!(matches!(
        code.expression(else_value.expression()).unwrap().kind(),
        ExprKind::SizeOfType(_)
    ));
    assert!(code.expressions().any(|(_, expression)| matches!(
        expression.kind(),
        ExprKind::Unary {
            operator: Unary::PostIncrement,
            ..
        }
    )));
    assert!(
        code.type_operands()
            .any(|(_, operand)| operand.evaluation() == TypeOperandEvaluation::Required)
    );
    assert!(
        code.bounds()
            .any(|(_, bound)| bound.evaluation() == BoundEvaluation::Required)
    );

    // A nested unevaluated context still suppresses type facts; keeping a query
    // plan must not reactivate them. Its short-circuit structure also survives.
    let analysis = analyze_with_options(
        "int f(int n) {return __builtin_constant_p(0 && sizeof(sizeof(int[n++])));}",
        Target::X86_64AppleDarwin,
        &options(),
    )
    .unwrap();
    let code = analysis.checked().unwrap();
    assert!(
        code.bounds()
            .all(|(_, b)| b.evaluation() == BoundEvaluation::Unevaluated)
    );
    assert!(code.expressions().any(|(_, e)| matches!(
        e.kind(),
        ExprKind::Binary {
            operator: Binary::LogicalAnd,
            ..
        }
    )));
}

#[test]
fn object_size_modes_and_outer_queries_suppress_nested_effects() {
    for name in ["__builtin_object_size", "__builtin_dynamic_object_size"] {
        for mode in 0..4 {
            let source =
                format!("unsigned long long f(int n) {{return {name}((int (*)[n++])0,{mode});}}");
            for target in Target::ALL {
                analyze(&source, target).unwrap();
                let analysis = analyze_with_options(&source, target, &options()).unwrap();
                let code = analysis.checked().unwrap();
                let (_, expression) = code
                    .expressions()
                    .find(|(_, e)| matches!(e.kind(), ExprKind::BuiltinCall { .. }))
                    .unwrap();
                let ExprKind::BuiltinCall {
                    query_evaluation,
                    arguments,
                    ..
                } = expression.kind()
                else {
                    panic!()
                };
                let suppression = if gnu(target) {
                    Some(QuerySuppression::ObjectSizeFrontendFold)
                } else if mode == 3 {
                    Some(QuerySuppression::MinimumSubobjectSize)
                } else {
                    None
                };
                assert_eq!(
                    *query_evaluation,
                    Some(suppression.map_or(
                        QueryEvaluation::ClangFallback {
                            side_effects: QuerySideEffects::Absent
                        },
                        QueryEvaluation::Unevaluated
                    ))
                );
                assert_eq!(arguments[1].context(), UseContext::UnevaluatedValue);
                assert_eq!(
                    code.bounds().next().unwrap().1.evaluation(),
                    if suppression.is_some() {
                        BoundEvaluation::Unevaluated
                    } else {
                        BoundEvaluation::Required
                    }
                );
            }
        }
    }
    let analysis = analyze_with_options(
        "int f(int n) {return __builtin_constant_p(n++ + __builtin_constant_p(sizeof(int[n++])));}",
        Target::X86_64AppleDarwin,
        &options(),
    )
    .unwrap();
    assert!(
        analysis
            .checked()
            .unwrap()
            .bounds()
            .all(|(_, b)| b.evaluation() == BoundEvaluation::Unevaluated)
    );
}

#[test]
fn constant_context_roots_remain_distinct_from_runtime_fallbacks() {
    let source = "int f(int n) {enum { K=__builtin_constant_p(sizeof(int[n++])) }; static int saved=__builtin_constant_p(sizeof(int[n++])); _Static_assert(__builtin_constant_p(sizeof(int[n++])) == 0, \"query\"); return K+saved;}";
    for target in Target::ALL {
        let plain = analyze(source, target).unwrap();
        let analysis = analyze_with_options(source, target, &options()).unwrap();
        assert_eq!(format!("{plain:?}"), format!("{:?}", analysis.unit()));
        let code = analysis.checked().unwrap();
        assert!(
            code.initializers()
                .any(|(_, initializer)| initializer.requires_constant())
        );
        // A fallback is a conditional compiler policy, not a runtime root. The
        // static initializer, enumerator and assertion remain their owning sites.
        assert_eq!(
            code.expressions()
                .filter(|(_, e)| matches!(
                    e.kind(),
                    ExprKind::BuiltinCall {
                        builtin: Builtin::ConstantQuery,
                        ..
                    }
                ))
                .count(),
            3
        );
        assert!(analysis.unit().enums.iter().any(|enumeration| {
            enumeration
                .variants
                .iter()
                .any(|variant| variant.name == "K" && variant.value.value == 0)
        }));
    }
}

#[test]
fn repeated_queries_obey_retention_limits_and_keep_diagnostics() {
    let mut source = String::from("int f(int n) {");
    for _ in 0..64 {
        source.push_str("__builtin_constant_p(n ? sizeof(int[n++]) : 0);");
    }
    source.push_str("return 0;}");
    analyze(&source, Target::X86_64AppleDarwin).unwrap();
    let mut limited = options();
    limited.limits.payload_bytes = 64;
    assert!(
        analyze_with_options(&source, Target::X86_64AppleDarwin, &limited)
            .unwrap_err()
            .message
            .contains("retention payload byte limit")
    );
    let analysis = analyze_with_options(&source, Target::X86_64AppleDarwin, &options()).unwrap();
    assert_eq!(analysis.checked().unwrap().bounds().count(), 64);
    for source in [
        "int f(int n) {return __builtin_constant_p(sizeof(int[missing++]));}",
        "int f(int n) {return __builtin_object_size((int (*)[n++])0,4);}",
        "int f(int n) {return __builtin_constant_p(n ? sizeof(int[n++]) : missing);}",
    ] {
        let plain = analyze(source, Target::X86_64AppleDarwin).unwrap_err();
        let retained =
            analyze_with_options(source, Target::X86_64AppleDarwin, &options()).unwrap_err();
        assert_eq!(
            (plain.offset, plain.message),
            (retained.offset, retained.message)
        );
    }
}

#[test]
#[ignore = "requires GCC and Clang native execution; run with --include-ignored"]
fn query_vla_effects_match_native_compilers_at_both_optimization_levels() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("queries.c");
    let executable = directory
        .path()
        .join(format!("queries{}", std::env::consts::EXE_SUFFIX));
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    // The oracle checks observable effects, independently of query values that
    // may change at -O2. Each case resets n, a, b and the pointer.
    let cases = [
        ("__builtin_constant_p(sizeof(int[n++]))", [3, 3, 4, 0]),
        (
            "__builtin_constant_p(helper() + sizeof(int[n++]))",
            [2, 3, 4, 0],
        ),
        (
            "__builtin_constant_p(pure_helper() + sizeof(int[n++]))",
            [3, 3, 4, 0],
        ),
        (
            "__builtin_constant_p(volatile_value + sizeof(int[n++]))",
            [2, 3, 4, 0],
        ),
        ("__builtin_constant_p(sizeof(int (*)[n++]))", [2, 3, 4, 0]),
        ("__builtin_constant_p((int (*)[n++])0)", [2, 3, 4, 0]),
        ("__builtin_constant_p(_Alignof(int[n++]))", [2, 3, 4, 0]),
        ("__builtin_constant_p(0 && sizeof(int[n++]))", [2, 3, 4, 0]),
        ("__builtin_constant_p(1 || sizeof(int[n++]))", [2, 3, 4, 0]),
        (
            "__builtin_constant_p(0 ? sizeof(int[n++]) : 0)",
            [2, 3, 4, 0],
        ),
        (
            "__builtin_constant_p(n ? sizeof(int[a++]) : sizeof(int[b++]))",
            [2, 4, 4, 0],
        ),
        (
            "__builtin_constant_p(sizeof(sizeof(int[n++])))",
            [2, 3, 4, 0],
        ),
        (
            "__builtin_constant_p(_Generic(n, int: sizeof(int[a++]), default: sizeof(int[b++])))",
            [2, 4, 4, 0],
        ),
        ("__builtin_constant_p(n++ + sizeof(int[a++]))", [2, 3, 4, 0]),
        (
            "__builtin_constant_p(0 ? n++ : sizeof(int[a++]))",
            [2, 3, 4, 0],
        ),
        (
            "__builtin_constant_p((n++, sizeof(int[a++])))",
            [2, 3, 4, 0],
        ),
        ("__builtin_constant_p((sizeof(int[n++]),0))", [3, 3, 4, 0]),
        (
            "__builtin_constant_p(__builtin_constant_p(sizeof(int[n++])))",
            [3, 3, 4, 0],
        ),
        ("__builtin_constant_p(sizeof *(p++))", [2, 3, 4, 1]),
        (
            "__builtin_constant_p(sizeof(typeof(*(int (*)[n++])0)))",
            [3, 3, 4, 0],
        ),
        ("__builtin_object_size((int (*)[n++])0,0)", [3, 3, 4, 0]),
        ("__builtin_object_size((int (*)[n++])0,3)", [2, 3, 4, 0]),
        (
            "__builtin_dynamic_object_size((int (*)[n++])0,0)",
            [3, 3, 4, 0],
        ),
        (
            "__builtin_dynamic_object_size((int (*)[n++])0,3)",
            [2, 3, 4, 0],
        ),
    ];
    let mut source = String::from(
        "int printf(const char*, ...); int helper(void) {return 0;} int pure_helper(void) __attribute__((pure)); int pure_helper(void) {return 0;} volatile int volatile_value; int main(void) { int n,a,b; int data[8];\n",
    );
    for (query, _) in cases {
        source.push_str(&format!("n=2;a=3;b=4;{{ int (*p)[n]=(void*)data; unsigned long long q={query}; (void)q; printf(\"%d %d %d %d\\n\",n,a,b,(int)(p-(int (*)[2])data)); }}\n"));
    }
    source.push_str("n=2;{enum {K=__builtin_constant_p(sizeof(int[n++]))}; static int saved=__builtin_constant_p(sizeof(int[n++])); _Static_assert(__builtin_constant_p(sizeof(int[n++]))==0, \"query\"); (void)K;(void)saved; printf(\"%d 3 4 0\\n\",n);} return 0;}");
    std::fs::write(&input, &source).unwrap();
    for compiler in [gcc.as_str(), "clang"] {
        let version = std::process::Command::new(compiler)
            .arg("--version")
            .output()
            .unwrap();
        assert!(version.status.success());
        let clang = String::from_utf8_lossy(&version.stdout)
            .to_ascii_lowercase()
            .contains("clang");
        for optimization in ["-O0", "-O2"] {
            let build = std::process::Command::new(compiler)
                .args(["-std=gnu11", optimization])
                .arg(&input)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                build.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&build.stderr)
            );
            let output = std::process::Command::new(&executable).output().unwrap();
            assert!(output.status.success());
            let actual: Vec<Vec<i32>> = String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .map(|line| {
                    line.split_whitespace()
                        .map(|n| n.parse().unwrap())
                        .collect()
                })
                .collect();
            for (row, (query, expected_clang)) in actual.iter().zip(cases) {
                let expected = if clang { expected_clang } else { [2, 3, 4, 0] };
                assert_eq!(row, &expected, "{compiler} {optimization}: {query}");
            }
            assert_eq!(actual.len(), cases.len() + 1);
            assert_eq!(
                actual.last().unwrap(),
                &[2, 3, 4, 0],
                "{compiler} {optimization}: required constant contexts"
            );
        }
    }
}

use std::process::Command;

use toucan_semantic::checked::{
    BoundEvaluation, ExprKind, TypeOperandEvaluation, UseContext, ValueCategory,
};
use toucan_semantic::{
    AnalysisOptions, analyze, analyze_with_options, evaluate_arithmetic, evaluate_integer,
};
use toucan_target::Target;

const TYPES: &[(&str, &str, bool)] = &[
    ("int", "const volatile int", true),
    ("int *const", "int *", true),
    ("const int *", "int *", false),
    ("int[]", "int[5]", true),
    ("int[4]", "int[5]", false),
    ("const int[2][3]", "int[2][3]", true),
    ("int *const[2]", "int *[2]", true),
    ("const int *[2]", "int *[2]", false),
    ("enum E", "enum F", false),
    ("struct S", "struct T", false),
    ("struct U", "struct U", true),
    ("int(int)", "int()", true),
    ("int(float)", "int()", false),
    ("int(int, ...)", "int(int)", false),
    ("void", "void", true),
    ("_Atomic(int)", "const _Atomic(int)", true),
    ("_Atomic(int) *", "int *", false),
    ("_Atomic(int) (*)[2]", "int (*)[2]", false),
    ("_Atomic(int)(void)", "int(void)", false),
];
const PREAMBLE: &str = "enum E {E0}; enum F {F0}; struct S {int x;}; struct T {int x;}; struct U;";
const VALID: &[&str] = &[
    "_Atomic int a; int f(int n){__builtin_choose_expr(1,a,0)=7;return __builtin_choose_expr(1,a,n);}",
    "_Atomic int a; _Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_choose_expr(1,a,0)),_Atomic int),\"atomic lvalue\");",
    "int call(void); enum {X=__builtin_choose_expr(1,7,call())}; _Static_assert(X==7,\"selected\");",
    "enum {X=__builtin_choose_expr(1,7,1/0)};",
    "int x; int *p=&__builtin_choose_expr(1,x,0); void f(void){__builtin_choose_expr(1,x,(void)0)=7;}",
    "int f(int); void g(void); int h(void){return __builtin_choose_expr(1,f,g)(3);}",
    "int a[3]; _Static_assert(sizeof(__builtin_choose_expr(1,a,(void)0))==sizeof a,\"array\");",
    "struct S{int x:3;}; int f(struct S s){return __builtin_choose_expr(1,s.x,0)+1;}",
    "void f(void){__builtin_choose_expr(0,1,(void)0);}",
    "double x=__builtin_choose_expr(1,1.25,0);",
    "char *s=__builtin_choose_expr(1,\"value\",(void)0);",
    "_Static_assert(__builtin_types_compatible_p(struct Q {int x;},struct Q),\"tag\"); struct Q q;",
    "int f(int n,int m){return __builtin_types_compatible_p(int[n++],int[m++]);}",
    "int f(int n){return __builtin_choose_expr(sizeof(int(*)[n++]),17,2);}",
    "int f(int n){typedef int A[n++];return __builtin_types_compatible_p(A,int[4]);}",
    "int f(int n){return __builtin_types_compatible_p(typeof(({n++;(int(*)[n++])0;})),int(*)[3]);}",
];
const INVALID: &[&str] = &[
    "int x=__builtin_choose_expr(1.0,1,2);",
    "const int a=1; int x=__builtin_choose_expr(a,1,2);",
    "int x=__builtin_choose_expr((1,1),1,2);",
    "int f(void){return __builtin_choose_expr(1,0,missing);}",
    "int f(void){return __builtin_choose_expr(1,0,1());}",
    "int x; enum{X=__builtin_choose_expr(1,x,7)};",
    "const int x=0;void f(void){__builtin_choose_expr(1,x,0)=1;}",
    "struct S{int x:3;};struct S s;int *p=&__builtin_choose_expr(1,s.x,0);",
    "int x=__builtin_types_compatible_p(int[-1],int[]);",
    "int f(void){return __builtin_types_compatible_p(int[1.0],int[]);}",
    "int x;int y=__builtin_types_compatible_p(x,int);",
    "int x=__builtin_choose_expr(1,2);",
    "int x=__builtin_types_compatible_p(int,int,int);",
];

fn gnu(target: Target) -> bool {
    matches!(
        target,
        Target::X86_64UnknownLinuxGnu
            | Target::X86_64UnknownLinuxMusl
            | Target::Aarch64UnknownLinuxGnu
            | Target::Aarch64UnknownLinuxMusl
    )
}

fn assertions(gnu: bool, msvc: bool) -> String {
    let mut source = PREAMBLE.to_owned();
    for &(left, right, expected) in TYPES {
        source.push_str(&format!(
            "_Static_assert(__builtin_types_compatible_p({left},{right})=={},\"{left}/{right}\");",
            u8::from(expected)
        ));
    }
    for (left, right) in [
        ("const int(void)", "int(void)"),
        ("const int(*)(void)", "int(*)(void)"),
        ("int *const(void)", "int *(void)"),
    ] {
        source.push_str(&format!("_Static_assert(__builtin_types_compatible_p({left},{right})=={},\"return qualifiers\");",u8::from(gnu)));
    }
    for (left, right) in [
        ("_Atomic(int)", "int"),
        ("_Atomic(int)[2]", "int[2]"),
        ("_Atomic(int *)", "int *"),
    ] {
        source.push_str(&format!(
            "_Static_assert(__builtin_types_compatible_p({left},{right})=={},\"atomic identity\");",
            u8::from(gnu)
        ));
    }
    source.push_str(&format!(
        "_Static_assert(__builtin_types_compatible_p(enum E,{}) ,\"enum integer\");",
        if msvc { "int" } else { "unsigned int" }
    ));
    source.push('\n');
    source
}

#[test]
fn compatibility_queries_and_selections_check_all_targets() {
    for target in Target::ALL {
        analyze(
            &assertions(gnu(target), target == Target::X86_64PcWindowsMsvc),
            target,
        )
        .unwrap();
        let unit = analyze(PREAMBLE, target).unwrap();
        let value = evaluate_integer(
            &unit,
            "__builtin_choose_expr(__builtin_types_compatible_p(int,const int),19,1/0)",
        )
        .unwrap();
        assert_eq!((value.value, value.bits, value.signed), (19, 32, true));
        assert!(evaluate_arithmetic(&unit, "__builtin_choose_expr(1,1.25,0)").is_ok());
        for source in VALID {
            analyze(source, target).unwrap_or_else(|error| panic!("{target}: {source}: {error}"));
        }
        for source in INVALID {
            assert!(analyze(source, target).is_err(), "{target}: {source}");
        }
        let returns = "const int f(void);int f(void);int *const g(void);int *g(void);";
        assert_eq!(analyze(returns, target).is_ok(), gnu(target));
    }
}

fn retained(source: &str, target: Target) -> toucan_semantic::Analysis {
    analyze_with_options(
        source,
        target,
        &AnalysisOptions {
            retain_code: true,
            ..AnalysisOptions::default()
        },
    )
    .unwrap()
}

#[test]
fn retained_queries_preserve_categories_types_and_source_ownership() {
    let source = "int f(int n,int m){int x; int a[3]; __builtin_choose_expr(1,x,(void)0)=7; sizeof(__builtin_choose_expr(1,a,x)); return __builtin_types_compatible_p(int[n++],typeof((int(*)[m++])0));}";
    let analysis = retained(source, Target::X86_64UnknownLinuxGnu);
    let code = analysis.checked().unwrap();
    let mut selected = 0;
    for (_, expression) in code.expressions() {
        match expression.kind() {
            ExprKind::Choose {
                condition,
                then_expression,
                then_selected,
                ..
            } => {
                assert!(*then_selected);
                assert_eq!(condition.context(), UseContext::UnevaluatedValue);
                assert_eq!(expression.category(), ValueCategory::ObjectLvalue);
                assert_eq!(
                    expression.ty(),
                    code.expression(*then_expression).unwrap().ty()
                );
                selected += 1;
            }
            ExprKind::TypesCompatible {
                left,
                right,
                compatible,
            } => {
                assert!(!compatible);
                for operand in [left, right] {
                    assert!(code.occurrence(operand.occurrence).is_some());
                    assert!(code.type_use(operand.type_use).is_some());
                }
            }
            _ => {}
        }
    }
    assert_eq!(selected, 2);
    assert!(
        code.bounds()
            .all(|(_, bound)| bound.evaluation() == BoundEvaluation::Unevaluated)
    );
    assert!(
        code.type_operands()
            .all(|(_, operand)| operand.evaluation() == TypeOperandEvaluation::Unevaluated)
    );
}

#[test]
fn retained_evaluation_suppresses_only_discarded_operands() {
    let source = "int f(int a,int b,int c){return __builtin_choose_expr(sizeof(int(*)[a++]),sizeof(int[b++]),sizeof(typeof(*(int(*)[c++])0)));}";
    let analysis = retained(source, Target::X86_64AppleDarwin);
    let code = analysis.checked().unwrap();
    let evaluations: Vec<_> = code.bounds().map(|(_, bound)| bound.evaluation()).collect();
    assert_eq!(
        evaluations
            .iter()
            .filter(|&&value| value == BoundEvaluation::Required)
            .count(),
        1
    );
    assert_eq!(
        evaluations
            .iter()
            .filter(|&&value| value == BoundEvaluation::Unevaluated)
            .count(),
        2
    );
    assert!(
        code.type_operands()
            .all(|(_, operand)| operand.evaluation() == TypeOperandEvaluation::Unevaluated)
    );
}

#[test]
fn retention_does_not_change_constraints_or_declarations() {
    for target in Target::ALL {
        for source in VALID.iter().chain(INVALID) {
            let ordinary = analyze(source, target);
            let retained = analyze_with_options(
                source,
                target,
                &AnalysisOptions {
                    retain_code: true,
                    ..AnalysisOptions::default()
                },
            );
            match (ordinary, retained) {
                (Ok(left), Ok(right)) => {
                    assert_eq!(format!("{left:?}"), format!("{:?}", right.unit()))
                }
                (Err(left), Err(right)) => {
                    assert_eq!((left.offset, left.message), (right.offset, right.message))
                }
                (left, right) => panic!("{target}: {source}: {left:?} / {right:?}"),
            }
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn constraints_and_runtime_effects_match_native_compilers() {
    let temp = tempfile::tempdir().unwrap();
    for compiler in ["gcc", "clang"] {
        let version = Command::new(compiler).arg("--version").output().unwrap();
        assert!(version.status.success());
        let is_gnu = !String::from_utf8_lossy(&version.stdout)
            .to_ascii_lowercase()
            .contains("clang");
        for (index, (source, valid)) in VALID
            .iter()
            .map(|s| (*s, true))
            .chain(INVALID.iter().map(|s| (*s, false)))
            .enumerate()
        {
            let path = temp.path().join(format!("constraint-{index}.c"));
            std::fs::write(&path, format!("{source}\n")).unwrap();
            let output = Command::new(compiler)
                .args(["-std=gnu11", "-fsyntax-only"])
                .arg(&path)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(valid),
                "{compiler}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let compiler_target = Command::new(compiler).arg("-dumpmachine").output().unwrap();
        assert!(compiler_target.status.success());
        let msvc = String::from_utf8_lossy(&compiler_target.stdout).contains("msvc");
        let source = format!(
            "{}\nint main(void){{int n=3,m=4,k=5;int value=__builtin_types_compatible_p(int[n++],int[m++]);if(value!=1||n!=3||m!=4)return 1; value=__builtin_choose_expr(1,sizeof(int[n++]),sizeof(int[m++]));if(value!=12||n!=4||m!=4)return 2; value=__builtin_choose_expr(sizeof(int(*)[k++]),17,2);if(value!=17||k!=5)return 3; int x=0;__builtin_choose_expr(1,x,(void)0)=7;return x!=7;}}\n",
            assertions(is_gnu, msvc)
        );
        let path = temp.path().join("runtime.c");
        std::fs::write(&path, source).unwrap();
        for optimization in ["-O0", "-O2"] {
            let executable = temp.path().join("runtime");
            let output = Command::new(compiler)
                .args(["-std=gnu11", optimization])
                .arg(&path)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(Command::new(&executable).status().unwrap().success());
        }
    }
}

#[test]
fn constant_query_gates_follow_only_the_selected_arm() {
    let source =
        "int f(int n){return __builtin_constant_p(__builtin_choose_expr(1,sizeof(int[n++]),n++));}";
    for target in [Target::X86_64UnknownLinuxGnu, Target::X86_64AppleDarwin] {
        let analysis = retained(source, target);
        let code = analysis.checked().unwrap();
        let query = code
            .expressions()
            .find_map(|(_, e)| {
                if let ExprKind::BuiltinCall {
                    query_evaluation, ..
                } = e.kind()
                {
                    *query_evaluation
                } else {
                    None
                }
            })
            .unwrap();
        if gnu(target) {
            assert!(matches!(
                query,
                toucan_semantic::checked::QueryEvaluation::Unevaluated(_)
            ));
            assert!(
                code.bounds()
                    .all(|(_, b)| b.evaluation() == BoundEvaluation::Unevaluated)
            );
        } else {
            assert_eq!(
                query,
                toucan_semantic::checked::QueryEvaluation::ClangFallback {
                    side_effects: toucan_semantic::checked::QuerySideEffects::Absent
                }
            );
            assert!(
                code.bounds()
                    .all(|(_, b)| b.evaluation() == BoundEvaluation::Required)
            );
        }
    }
}

#[test]
fn constant_expression_prechecks_preserve_unevaluated_type_operands() {
    for source in [
        "int f(int n){int a[__builtin_types_compatible_p(int[n++],int[3])];return sizeof a;}",
        "int f(int n){int a[__builtin_choose_expr(1,1,sizeof(int[n++]))];return sizeof a;}",
        "int f(int n){_Static_assert(__builtin_types_compatible_p(typeof((int(*)[n++])0),int(*)[3]),\"type\");return n;}",
    ] {
        let analysis = retained(source, Target::X86_64UnknownLinuxGnu);
        let code = analysis.checked().unwrap();
        assert!(
            code.bounds().all(|(_, b)| matches!(
                b.evaluation(),
                BoundEvaluation::Constant | BoundEvaluation::Unevaluated
            )),
            "{source}: {:?}",
            code.bounds()
                .map(|(_, b)| b.evaluation())
                .collect::<Vec<_>>()
        );
        assert!(
            code.type_operands()
                .all(|(_, o)| o.evaluation() == TypeOperandEvaluation::Unevaluated)
        );
    }
}

#[test]
fn repeated_queries_keep_tag_identity_and_variadic_pack_uses() {
    let source = "enum{X=__builtin_choose_expr(1,2,sizeof(struct S{int x;}))};struct S object;";
    retained(source, Target::X86_64UnknownLinuxGnu);
    let source = "int g(int,...);extern inline __attribute__((gnu_inline,always_inline)) int f(int n,...){return g(n,__builtin_choose_expr(1,__builtin_va_arg_pack(),17));}";
    let analysis = retained(source, Target::X86_64UnknownLinuxGnu);
    let code = analysis.checked().unwrap();
    let arguments = code
        .expressions()
        .find_map(|(_, e)| {
            if let ExprKind::Call { arguments, .. } = e.kind() {
                Some(arguments)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(arguments[1].context(), UseContext::VariadicPack);
    assert!(arguments[1].conversions().is_empty());
}

#[test]
#[ignore = "requires Clang with all five target backends"]
fn target_type_compatibility_matches_clang() {
    let temp = tempfile::tempdir().unwrap();
    for target in Target::ALL {
        let path = temp.path().join("types.c");
        std::fs::write(
            &path,
            assertions(false, target == Target::X86_64PcWindowsMsvc),
        )
        .unwrap();
        let output = Command::new("clang")
            .args([
                "-target",
                target.triple(),
                "-std=gnu11",
                "-pedantic-errors",
                "-Wno-strict-prototypes",
                "-fsyntax-only",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

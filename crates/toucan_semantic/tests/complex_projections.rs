use toucan_semantic::checked::{
    AtomicAccess, Builtin, Conversion, ExprKind, Unary, UseContext, ValueCategory,
};
use toucan_semantic::{
    AnalysisOptions, ArithmeticConstant, FloatKind, analyze_with_profile, evaluate_arithmetic,
    evaluate_integer,
};
use toucan_target::{Compiler, CompilerProfile};

const COMMON: &[&str] = &[
    "double _Complex z; double f(void){return __real z + __imag z + __real__ z + __imag__ z;}",
    "double _Complex z; double *f(void){return &__real__ z;} void g(void){__real__ z=2;__imag__ z+=3;}",
    "const double _Complex z=0; void f(void){__real__ z=2;}",
    "volatile double _Complex z; _Static_assert(__builtin_types_compatible_p(__typeof__(&__real__ z),double*),\"unqualified\");",
    "const short x=1; _Static_assert(__builtin_types_compatible_p(__typeof__(__imag__ x)*,const short*),\"qualified\");",
    "const short x=1; _Static_assert(__builtin_types_compatible_p(__typeof__(&__real__ x),const short*),\"qualified\");",
    "int x; int *p=&__real__ x; void f(void){__real__ x=2;}",
    "double _Complex z; double _Complex f(void){return ~z;}",
    "double _Complex z; void f(void){z++;++z;z--;--z;}",
    "_Atomic(double _Complex) z; void f(void){z++;++z;z--;--z;}",
    "enum {N=__real__ 3, M=__imag__ 4};",
    "double f(double _Complex z){return __builtin_creal(z)+__builtin_cimag(z);} double _Complex g(double x){return __builtin_conj(x);}",
    "float f(double _Complex z){return __builtin_crealf(z);} long double g(float _Complex z){return __builtin_cimagl(z);}",
];
const INVALID: &[&str] = &[
    "void f(void){register double _Complex z; double *p=&__real__ z;}",
    "double _Complex f(void); double *g(void){return &__real__ f();}",
    "int x; void f(void){__imag__ x=2;}",
    "void *x; void f(void){(void)__real__ x;}",
    "void f(void){(void)__imag__ (void)0;}",
    "double x; double f(void){return ~x;}",
    "double f(void){return __builtin_creal();}",
    "double f(void){return __builtin_cimag(1.0,2.0);}",
    "double f(void *p){return __builtin_creal(p);}",
    "double _Complex f(void){return __builtin_conj((void)0);}",
];
fn check(source: &str, profile: CompilerProfile, accepted: bool) {
    let plain = analyze_with_profile(source, profile, &AnalysisOptions::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (plain, retained) {
        (Ok(plain), Ok(retained)) => {
            assert!(accepted, "unexpected acceptance {profile:?}: {source}");
            assert_eq!(
                format!("{:?}", plain.unit()),
                format!("{:?}", retained.unit())
            );
        }
        (Err(plain), Err(retained)) => {
            assert!(!accepted, "{profile:?}: {source}: {plain}");
            assert_eq!(plain.message, retained.message);
        }
        other => panic!("retention mismatch {profile:?}: {source}: {other:?}"),
    }
}
#[test]
fn projection_constraints_and_categories_match_profiles() {
    for profile in CompilerProfile::ALL {
        for source in COMMON {
            check(source, profile, true);
        }
        for source in INVALID {
            check(source, profile, false);
        }
        let gnu = profile.compiler() == Compiler::Gnu;
        for source in [
            "double _Complex z; double *p=&__real__ z;",
            "double _Complex z; double *p=&__imag__ z;",
            "struct S{double _Complex z;}s; double *p=&__imag__ s.z;",
            "struct S{unsigned x:3;}s;int a[sizeof(__real__ s.x)];",
        ] {
            check(source, profile, !gnu);
        }
        check(
            "struct S{unsigned x:3;}s;void f(void){__real__ s.x=2;}",
            profile,
            gnu,
        );
        for op in ["+", "-", "~", "++"] {
            let qualified = if gnu { "volatile " } else { "" };
            check(
                &format!(
                    "volatile double _Complex z; _Static_assert(__builtin_types_compatible_p(__typeof__({op}z)*,{qualified}double _Complex*),\"type\");"
                ),
                profile,
                true,
            );
        }
        for source in [
            "_Atomic(double _Complex) z;double f(void){return __real__ z;}",
            "_Atomic(double) z;double f(void){return __real__ z;}",
        ] {
            let error =
                analyze_with_profile(source, profile, &AnalysisOptions::default()).unwrap_err();
            assert!(error.message.contains("atomic"), "{error}");
            check(source, profile, false);
        }
        if gnu {
            let error = analyze_with_profile(
                "struct S{unsigned x:3;}s;int f(void){return __imag__ s.x;}",
                profile,
                &AnalysisOptions::default(),
            )
            .unwrap_err();
            assert!(error.message.contains("precise-width"));
        }
    }
}
#[test]
fn scalar_bits_conjugation_and_fixed_prototypes() {
    let cases = [
        (
            "__real__ __builtin_complex(-0.0,1.0)",
            0x8000_0000_0000_0000,
        ),
        (
            "__imag__ __builtin_complex(2.0,-0.0)",
            0x8000_0000_0000_0000,
        ),
        ("__imag__ -0.0", 0),
        (
            "__real__ __builtin_complex(__builtin_nan(\"7\"),2.0)",
            0x7ff8_0000_0000_0007,
        ),
        ("__imag__ ~__builtin_complex(2.0,-0.0)", 0),
        (
            "__builtin_creal(__builtin_complex(-0.0,1.0))",
            0x8000_0000_0000_0000,
        ),
        ("__builtin_cimag(2.0)", 0),
        (
            "__builtin_cimag(__builtin_conj(__builtin_complex(2.0,3.0)))",
            0xc008_0000_0000_0000,
        ),
    ];
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile("", profile, &AnalysisOptions::default()).unwrap();
        for (source, expected) in cases {
            if profile.compiler() == Compiler::Clang
                && (source.contains("__builtin_creal")
                    || source.contains("__builtin_cimag")
                    || source.contains("__builtin_conj"))
            {
                let error = evaluate_arithmetic(analysis.unit(), source).unwrap_err();
                assert!(error.message.contains("frontend constant"));
                check(&format!("double x={source};"), profile, false);
                continue;
            }
            let ArithmeticConstant::Floating(value) = evaluate_arithmetic(analysis.unit(), source)
                .unwrap_or_else(|e| panic!("{profile:?} {source}: {e}"))
            else {
                panic!("not floating")
            };
            assert_eq!(value.to_bits(), expected, "{profile:?}: {source}");
            check(&format!("double x={source};"), profile, true);
        }
        for (suffix, kind) in [
            ("f", FloatKind::Float),
            ("", FloatKind::Double),
            ("l", FloatKind::LongDouble),
        ] {
            let source = format!("__builtin_conj{suffix}(__builtin_complex(-0.0,-0.0))");
            if profile.compiler() == Compiler::Clang {
                assert!(evaluate_arithmetic(analysis.unit(), &source).is_err());
                continue;
            }
            let ArithmeticConstant::Complex(value) =
                evaluate_arithmetic(analysis.unit(), &source).unwrap()
            else {
                panic!("complex")
            };
            assert_eq!(value.kind(), kind);
            assert_ne!(value.real().to_bits(), 0);
            assert_eq!(value.imaginary().to_bits(), 0);
        }
        assert_eq!(
            evaluate_integer(analysis.unit(), "__real__ 3")
                .unwrap()
                .value,
            3
        );
        assert_eq!(
            evaluate_integer(analysis.unit(), "__imag__ 3")
                .unwrap()
                .value,
            0
        );
    }
}
#[test]
fn retained_places_preserve_volatile_and_whole_atomic_updates() {
    let source = "volatile double _Complex z; volatile double r; _Atomic(double _Complex) a; double next(void); void f(void){__real__ z=2; (void)__imag__ z; (void)&__real__ z; (void)__imag__ r; (void)__imag__ next(); a++; (void)~z; (void)__builtin_creal(z);}";
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile(
            source,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let code = analysis.checked().unwrap();
        let mut places = 0;
        let mut updates = 0;
        let mut builtin = 0;
        for (_, expr) in code.expressions() {
            match expr.kind() {
                ExprKind::Unary {
                    operator: Unary::Real | Unary::Imaginary,
                    operand,
                    ..
                } if expr.category() == ValueCategory::ObjectLvalue => {
                    assert!(expr.is_volatile_place());
                    assert!(
                        !analysis
                            .unit()
                            .qualifiers(code.ty(expr.ty()).unwrap())
                            .unwrap()
                            .is_volatile
                    );
                    assert_eq!(operand.context(), UseContext::Place);
                    assert!(operand.conversions().is_empty());
                    places += 1;
                }
                ExprKind::Unary {
                    operator: Unary::Imaginary,
                    operand,
                    ..
                } => assert_eq!(operand.context(), UseContext::Value),
                ExprKind::Unary {
                    operator: Unary::PostIncrement,
                    operand,
                    ..
                } => {
                    assert_eq!(expr.atomic_access(), Some(AtomicAccess::ReadModifyWrite));
                    assert_eq!(operand.context(), UseContext::ReadModifyWrite);
                    assert!(
                        operand
                            .conversions()
                            .iter()
                            .any(|c| c.kind() == Conversion::AtomicLoad)
                    );
                    updates += 1;
                }
                ExprKind::BuiltinCall {
                    builtin: Builtin::ComplexReal,
                    arguments,
                    ..
                } => {
                    assert_eq!(arguments.len(), 1);
                    assert_eq!(arguments[0].context(), UseContext::Value);
                    assert_eq!(
                        arguments[0].conversions().last().unwrap().kind(),
                        Conversion::Lvalue
                    );
                    builtin += 1;
                }
                _ => {}
            }
        }
        assert_eq!((places, updates, builtin), (3, 1, 1));
    }
}
#[test]
fn component_extent_proofs_keep_compiler_subobject_rules() {
    for profile in CompilerProfile::ALL {
        let retained = analyze_with_profile(
            "double _Complex z;void f(void){__builtin_object_size(&__real__ z,1);}",
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let proof = retained
            .checked()
            .unwrap()
            .expressions()
            .find_map(|(_, e)| match e.kind() {
                ExprKind::BuiltinCall {
                    object_size: Some(proof),
                    ..
                } => Some(proof),
                _ => None,
            })
            .unwrap();
        assert_eq!(proof.whole_bytes(), Some(16));
        assert_eq!(proof.subobject_bytes(), Some(8));
        let analysis = analyze_with_profile(
            "double _Complex z; struct S{int tag;double _Complex z;char tail[5];}s;",
            profile,
            &AnalysisOptions::default(),
        )
        .unwrap();
        let gnu = profile.compiler() == Compiler::Gnu;
        for (pointer, whole, clang_subobject) in [
            ("&__real__ z", 16, 16),
            ("&__imag__ z", 8, 8),
            ("&__real__ s.z", 24, 16),
            ("&__imag__ s.z", 16, 8),
        ] {
            for mode in 0..4 {
                let expected = if mode % 2 == 0 {
                    whole
                } else if gnu {
                    8
                } else {
                    clang_subobject
                };
                assert_eq!(
                    evaluate_integer(
                        analysis.unit(),
                        &format!("__builtin_object_size({pointer},{mode})")
                    )
                    .unwrap()
                    .value,
                    expected,
                    "{profile:?}: {pointer} {mode}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn native_projection_constraints_effects_and_values() {
    use std::process::Command;
    let temporary = tempfile::tempdir().unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc.as_str(), "clang"] {
        let version = Command::new(compiler).arg("--version").output().unwrap();
        assert!(version.status.success());
        let gnu = !String::from_utf8_lossy(&version.stdout)
            .to_ascii_lowercase()
            .contains("clang");
        for (source, expected) in COMMON
            .iter()
            .map(|s| (*s, true))
            .chain(INVALID.iter().map(|s| (*s, false)))
            .chain([
                ("double _Complex z;double *p=&__real__ z;", !gnu),
                ("double _Complex z;double *p=&__imag__ z;", !gnu),
                (
                    "struct S{unsigned x:3;}s;void f(void){__real__ s.x=2;}",
                    gnu,
                ),
                (
                    "struct S{unsigned x:3;}s;int a[sizeof(__real__ s.x)];",
                    !gnu,
                ),
            ])
        {
            let path = temporary.path().join("constraint.c");
            std::fs::write(&path, format!("{source}\n")).unwrap();
            let output = Command::new(compiler)
                .args(["-std=c11", "-fsyntax-only"])
                .arg(&path)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(expected),
                "{compiler}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let source = r#"#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <float.h>
_Static_assert(sizeof(double)==8 && DBL_MANT_DIG==53,"binary64 values");
int count;double next(void){count++;return 5.0;}double _Complex cn(void){count++;return __builtin_complex(5.0,7.0);}
int main(void){double values[]={__real__ __builtin_complex(-0.0,1.0),__imag__ __builtin_complex(2.0,-0.0),__imag__ -0.0,__real__ __builtin_complex(__builtin_nan("7"),2.0),__imag__ ~__builtin_complex(2.0,-0.0),__builtin_creal(__builtin_complex(-0.0,1.0)),__builtin_cimag(2.0),__builtin_cimag(__builtin_conj(__builtin_complex(2.0,3.0)))};
for(unsigned i=0;i<sizeof(values)/sizeof(values[0]);i++){uint64_t bits;memcpy(&bits,&values[i],8);printf("%016llx\n",(unsigned long long)bits);}
double x=3; double _Complex z=__builtin_complex(3.0,4.0);double a=__imag__ x++;double b=__imag__ next();double c=__imag__ cn();double d=__imag__ z++;double e=__real__ ++z;
printf("%d %.0f %.0f %.0f %.0f %.0f %.0f %.0f %.0f\n",count,x,a,b,c,d,e,__real__ z,__imag__ z);return 0;}
"#;
        let path = temporary.path().join("runtime.c");
        std::fs::write(&path, source).unwrap();
        for optimization in ["-O0", "-O2"] {
            let binary = temporary.path().join("runtime");
            let output = Command::new(compiler)
                .args(["-std=c11", optimization])
                .arg(&path)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(true),
                "{compiler} {optimization}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(&binary).output().unwrap();
            assert!(output.status.success(), "{compiler}: {output:?}");
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                "8000000000000000\n8000000000000000\n0000000000000000\n7ff8000000000007\n0000000000000000\n8000000000000000\n0000000000000000\nc008000000000000\n2 4 0 0 7 4 5 5 4\n"
            );
        }
    }
}

#[test]
fn projection_recursion_uses_existing_budgets() {
    for profile in CompilerProfile::ALL {
        for depth in [63, 600] {
            let source = format!("int x={0}1;", "__real__ ".repeat(depth));
            let result = analyze_with_profile(&source, profile, &AnalysisOptions::default());
            if depth == 63 {
                result.unwrap();
            } else {
                let error = result.unwrap_err();
                assert!(
                    error.message.contains("limit") || error.message.contains("budget"),
                    "{error}"
                );
            }
        }
    }
}

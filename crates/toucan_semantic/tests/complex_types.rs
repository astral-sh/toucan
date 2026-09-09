use toucan_semantic::checked::{AtomicAccess, Binary, Builtin, Conversion, ExprKind, UseContext};
use toucan_semantic::{
    AnalysisOptions, ArithmeticConstant, FloatKind, Type, TypeKind, analyze_with_profile,
    evaluate_arithmetic, evaluate_integer,
};
use toucan_target::{Compiler, CompilerProfile};

const VALID: &[&str] = &[
    "float _Complex f; double _Complex d; long double _Complex l;",
    "_Atomic(double _Complex) z=1.0;",
    "_Atomic(double _Complex) make(double _Complex z){return z;} void f(void){_Atomic(double _Complex) z=make(1.0);z=make(2.0);}",
    "double _Complex z=__builtin_complex(1.0,2.0); double _Complex a[]={1,2.0i,3.0};",
    "const double _Complex z={1}; struct S{double _Complex z;}; struct S s={2.0i};",
    "double _Complex f(double _Complex z,double x){ z+=x;z*=x;z/=2.0;return -z+x; }",
    "double f(double _Complex z){ return z; } int g(double _Complex z){return z;} _Bool h(double _Complex z){return z;}",
    "int f(double _Complex z){if(z)return z==2;return !z || z!=0;}",
    "double _Complex f(int n,float _Complex z,double x){return n?z:x;}",
    "_Atomic(double _Complex) z; double _Complex f(double _Complex x){z=x;z+=x;return z*=x;}",
    "void variadic(int,...); void f(float _Complex z){variadic(0,z);}",
    "float _Complex f(x) float _Complex x; {return x;}",
    "_Static_assert(_Generic(1.0fi,float _Complex:1,default:0),\"type\");",
    "_Static_assert(__builtin_constant_p(__builtin_complex(1.0,2.0)),\"known\");",
];
const INVALID: &[&str] = &[
    "int _Complex z;",
    "_Complex z;",
    "double double _Complex z;",
    "double _Complex _Complex z;",
    "float double _Complex z;",
    "signed double _Complex z;",
    "__bf16 _Complex z;",
    "double _Complex z; int f(void){return z<1;}",
    "double _Complex z; int f(void){return z%1;}",
    "double _Complex z; int f(void){return z&1;}",
    "double _Complex z; void *p=(void*)z;",
    "void *p; double _Complex f(void){return (double _Complex)p;}",
    "double _Complex z=__builtin_complex(1,2);",
    "double _Complex z=__builtin_complex(1.0f,2.0);",
    "double _Complex z=__builtin_complex(1.0);",
    "const double _Complex z=1; void f(void){z=2;}",
    "void f(void){float (*p)(void);float _Complex (*q)(void);p=q;}",
    "double _Complex z; _Static_assert(z,\"not an ICE\");",
];

#[test]
fn standard_complex_constraints_and_retention_match() {
    for profile in CompilerProfile::ALL {
        for source in VALID {
            let plain = analyze_with_profile(source, profile, &AnalysisOptions::default())
                .unwrap_or_else(|e| panic!("{profile:?}: {source}: {e}"));
            let retained = analyze_with_profile(
                source,
                profile,
                &AnalysisOptions {
                    retain_code: true,
                    ..Default::default()
                },
            )
            .unwrap_or_else(|e| panic!("retained {profile:?}: {source}: {e}"));
            assert_eq!(
                format!("{:?}", plain.unit()),
                format!("{:?}", retained.unit())
            );
        }
        for source in INVALID {
            let plain =
                analyze_with_profile(source, profile, &AnalysisOptions::default()).unwrap_err();
            let retained = analyze_with_profile(
                source,
                profile,
                &AnalysisOptions {
                    retain_code: true,
                    ..Default::default()
                },
            )
            .unwrap_err();
            assert_eq!(plain.message, retained.message, "{profile:?}: {source}");
        }
    }
}

#[test]
fn corresponding_real_storage_and_atomic_alignment() {
    for profile in CompilerProfile::ALL {
        let analyzed = analyze_with_profile("", profile, &AnalysisOptions::default()).unwrap();
        let unit = analyzed.unit();
        for kind in [FloatKind::Float, FloatKind::Double, FloatKind::LongDouble] {
            let real = Type::new(TypeKind::Float(kind));
            let complex = Type::new(TypeKind::Complex(kind));
            assert_eq!(
                (unit.layout(&complex).unwrap().size_bits / 8),
                2 * (unit.layout(&real).unwrap().size_bits / 8)
            );
            assert_eq!(
                unit.alignment(&complex).unwrap(),
                unit.alignment(&real).unwrap()
            );
            let atomic = Type::new(TypeKind::Atomic(Box::new(complex.clone())));
            assert_eq!(
                (unit.layout(&atomic).unwrap().size_bits / 8),
                (unit.layout(&complex).unwrap().size_bits / 8)
            );
            assert_eq!(
                unit.alignment(&atomic).unwrap(),
                if profile.target().is_armv7() {
                    8
                } else if profile.target() == toucan_target::Target::I686UnknownLinuxGnu {
                    match kind {
                        FloatKind::LongDouble => 4,
                        FloatKind::Double if profile.compiler() == Compiler::Clang => 4,
                        _ => (unit.layout(&complex).unwrap().size_bits / 8).min(16),
                    }
                } else {
                    (unit.layout(&complex).unwrap().size_bits / 8).min(16)
                }
            );
        }
    }
}

const PAIRS: &[(&str, u128, u128)] = &[
    ("2.5i", 0, 0x4004_0000_0000_0000),
    (
        "__builtin_complex(-0.0,-0.0)",
        0x8000_0000_0000_0000,
        0x8000_0000_0000_0000,
    ),
    ("(double _Complex)-0.0", 0x8000_0000_0000_0000, 0),
    ("__builtin_complex(-0.0,-0.0)+0.0", 0, 0x8000_0000_0000_0000),
    (
        "__builtin_complex(__builtin_inf(),1.0)*2.0",
        0x7ff0_0000_0000_0000,
        0x4000_0000_0000_0000,
    ),
    (
        "__builtin_complex(__builtin_inf(),1.0)/2.0",
        0x7ff0_0000_0000_0000,
        0x3fe0_0000_0000_0000,
    ),
    (
        "__builtin_complex(1.0,2.0)*__builtin_complex(3.0,4.0)",
        0xc014_0000_0000_0000,
        0x4024_0000_0000_0000,
    ),
    (
        "__builtin_complex(1.0,2.0)/__builtin_complex(3.0,4.0)",
        0x3fdc_28f5_c28f_5c29,
        0x3fb4_7ae1_47ae_147b,
    ),
    (
        "__builtin_complex(__builtin_nan(\"7\"),__builtin_inf())",
        0x7ff8_0000_0000_0007,
        0x7ff0_0000_0000_0000,
    ),
];

#[test]
fn complex_constants_preserve_domains_formats_and_exceptional_components() {
    for profile in CompilerProfile::ALL {
        let analyzed = analyze_with_profile("", profile, &AnalysisOptions::default()).unwrap();
        for (source, real, imaginary) in PAIRS {
            let ArithmeticConstant::Complex(value) = evaluate_arithmetic(analyzed.unit(), source)
                .unwrap_or_else(|e| panic!("{profile:?}: {source}: {e}"))
            else {
                panic!("not complex: {source}");
            };
            assert_eq!(
                (value.real().to_bits(), value.imaginary().to_bits()),
                (*real, *imaginary),
                "{profile:?}: {source}"
            );
        }
        for (source, expected) in [
            ("(_Bool)2.0i", 1),
            ("(int)2.0i", 0),
            ("!2.0i", 0),
            ("2.0i == 0.0", 0),
            ("2.0i != 0.0", 1),
        ] {
            let ArithmeticConstant::Integer(value) =
                evaluate_arithmetic(analyzed.unit(), source).unwrap()
            else {
                panic!("not integer")
            };
            assert_eq!(value.value, expected, "{profile:?}: {source}");
            assert!(
                evaluate_integer(analyzed.unit(), source).is_err(),
                "complex expression is not a C11 ICE: {source}"
            );
        }
        let nan_source = "__builtin_complex(__builtin_nan(\"7\"),1.0)*__builtin_complex(2.0,3.0)";
        let ArithmeticConstant::Complex(nan) =
            evaluate_arithmetic(analyzed.unit(), nan_source).unwrap()
        else {
            panic!("not complex")
        };
        let payload = if profile.compiler() == Compiler::Gnu {
            0
        } else {
            7
        };
        assert_eq!(nan.real().to_bits(), 0x7ff8_0000_0000_0000 | payload);
        assert_eq!(nan.imaginary().to_bits(), 0x7ff8_0000_0000_0000 | payload);
        let source = "__builtin_complex(1.0e308,1.0e308)/__builtin_complex(1.0e308,1.0e308)";
        let ArithmeticConstant::Complex(value) =
            evaluate_arithmetic(analyzed.unit(), source).unwrap()
        else {
            panic!("not complex")
        };
        assert_eq!(
            value.real().to_bits(),
            if profile.compiler() == Compiler::Gnu {
                0x3ff0_0000_0000_0000
            } else {
                0x7ff0_0000_0000_0000
            }
        );
        assert_eq!(value.imaginary().to_bits(), 0);
    }
}

#[test]
fn component_conversion_preserves_target_precision_and_extended_fold_limits() {
    for profile in CompilerProfile::ALL {
        let analyzed = analyze_with_profile("", profile, &AnalysisOptions::default()).unwrap();
        let ArithmeticConstant::Complex(value) = evaluate_arithmetic(
            analyzed.unit(),
            "(float _Complex)__builtin_complex(16777217.0,-0.0)",
        )
        .unwrap() else {
            panic!("not complex")
        };
        assert_eq!(value.kind(), FloatKind::Float);
        assert_eq!(value.real().to_bits(), 0x4b80_0000);
        assert_eq!(value.imaginary().to_bits(), 0x8000_0000);
        let ArithmeticConstant::Complex(value) = evaluate_arithmetic(
            analyzed.unit(),
            "__builtin_complex(0x1.0000000000000002p0L,-0.0L)",
        )
        .unwrap() else {
            panic!("not complex")
        };
        assert_eq!(value.kind(), FloatKind::LongDouble);
        let (real, imaginary) = match profile.target() {
            toucan_target::Target::X86_64UnknownLinuxGnu
            | toucan_target::Target::X86_64UnknownLinuxMusl
            | toucan_target::Target::I686UnknownLinuxGnu
            | toucan_target::Target::X86_64AppleDarwin => (0x3fff_8000_0000_0000_0001, 1u128 << 79),
            toucan_target::Target::Aarch64UnknownLinuxGnu
            | toucan_target::Target::Aarch64UnknownLinuxMusl => {
                ((0x3fffu128 << 112) | (1 << 49), 1u128 << 127)
            }
            _ => (0x3ff0_0000_0000_0000, 1u128 << 63),
        };
        assert_eq!(
            (value.real().to_bits(), value.imaginary().to_bits()),
            (real, imaginary)
        );
        if profile.compiler() == Compiler::Gnu {
            let expression =
                "__builtin_complex(0x1p16380L,0x1p16380L)/__builtin_complex(0x1p16380L,0x1p16380L)";
            assert!(
                evaluate_arithmetic(analyzed.unit(), expression)
                    .unwrap_err()
                    .message
                    .contains("rounding could not be proven")
            );
            analyze_with_profile(
                "long double _Complex f(long double _Complex x){return x/x;}",
                profile,
                &AnalysisOptions {
                    retain_code: true,
                    ..Default::default()
                },
            )
            .unwrap();
        }
    }
}

#[test]
fn retained_operations_keep_real_operands_and_atomic_updates() {
    let source = "_Atomic(double _Complex) z; double _Complex f(double _Complex x,double y){z*=x;return x*y+1.0i;} void variadic(int,...); void g(float _Complex x){variadic(0,x);}";
    for profile in CompilerProfile::ALL {
        let analyzed = analyze_with_profile(
            source,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let code = analyzed.checked().unwrap();
        let mut mixed = false;
        let mut atomic = false;
        let mut variadic = false;
        let mut imaginary = false;
        for (_, expression) in code.expressions() {
            match expression.kind() {
                ExprKind::Binary {
                    operator: Binary::Multiply,
                    left,
                    right,
                    ..
                } => {
                    assert!(matches!(
                        code.ty(left.effective_type()).unwrap().kind,
                        TypeKind::Complex(FloatKind::Double)
                    ));
                    assert!(matches!(
                        code.ty(right.effective_type()).unwrap().kind,
                        TypeKind::Float(FloatKind::Double)
                    ));
                    mixed = true;
                }
                ExprKind::Binary {
                    operator: Binary::AssignMultiply,
                    left,
                    ..
                } => {
                    assert_eq!(
                        expression.atomic_access(),
                        Some(AtomicAccess::ReadModifyWrite)
                    );
                    assert_eq!(left.context(), UseContext::ReadModifyWrite);
                    assert_eq!(
                        left.conversions()
                            .iter()
                            .filter(|c| c.kind() == Conversion::AtomicLoad)
                            .count(),
                        1
                    );
                    atomic = true;
                }
                ExprKind::Call { arguments, .. } => {
                    assert!(matches!(
                        code.ty(arguments[1].effective_type()).unwrap().kind,
                        TypeKind::Complex(FloatKind::Float)
                    ));
                    variadic = true;
                }
                ExprKind::ImaginaryFloat { .. } => imaginary = true,
                _ => {}
            }
        }
        assert!(mixed && atomic && variadic && imaginary);
    }
}

#[test]
fn complex_constructor_argument_uses_and_query_boundaries() {
    for profile in CompilerProfile::ALL {
        let source = "double _Complex f(double x,double y){return __builtin_complex(x,y);}";
        let analyzed = analyze_with_profile(
            source,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let code = analyzed.checked().unwrap();
        let arguments = code
            .expressions()
            .find_map(|(_, expr)| match expr.kind() {
                ExprKind::BuiltinCall {
                    builtin: Builtin::Complex,
                    arguments,
                    ..
                } => Some(arguments),
                _ => None,
            })
            .unwrap();
        assert!(arguments.iter().all(|arg| {
            arg.conversions()
                .iter()
                .any(|c| c.kind() == Conversion::Lvalue)
        }));
        let source = "int f(double _Complex z){return __builtin_constant_p(z);}";
        let result = analyze_with_profile(source, profile, &AnalysisOptions::default());
        if profile.compiler() == Compiler::Clang {
            assert!(
                result
                    .unwrap_err()
                    .message
                    .contains("complex constant-query fallback")
            );
        } else {
            result.unwrap();
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn constraints_and_static_component_bits_match_native_compilers() {
    use std::process::Command;
    let temporary = tempfile::tempdir().unwrap();
    // Only binary64 component values are compared here. GNU Darwin is not a
    // Toucan profile; these expressions have the same double format in both
    // supported GNU profiles and on the native macOS oracle. Target-dependent
    // long-double layouts and conversions have separate seven-profile tests.
    let target = toucan_target::Target::X86_64UnknownLinuxGnu;
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc.as_str(), "clang"] {
        let version = Command::new(compiler).arg("--version").output().unwrap();
        assert!(version.status.success());
        let flavor = if String::from_utf8_lossy(&version.stdout)
            .to_ascii_lowercase()
            .contains("clang")
        {
            Compiler::Clang
        } else {
            Compiler::Gnu
        };
        let profile = CompilerProfile::new(target, flavor).unwrap();
        for (source, accepted) in VALID
            .iter()
            .map(|s| (*s, true))
            .chain(INVALID.iter().map(|s| (*s, false)))
        {
            let path = temporary.path().join("constraint.c");
            std::fs::write(&path, format!("{source}\n")).unwrap();
            let mut command = Command::new(compiler);
            command.args(["-std=c11", "-fsyntax-only"]);
            // The valid fixtures include GNU imaginary literal spellings used by
            // system complex.h. Invalid integer/implicit complex spellings are
            // C11 constraints, even though GNU separately extends those types.
            if !accepted {
                command.arg("-pedantic-errors");
            }
            let output = command.arg(&path).output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(accepted),
                "{compiler}: {source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let mut expressions: Vec<String> = PAIRS
            .iter()
            .map(|(source, _, _)| (*source).into())
            .collect();
        expressions.extend(
            [
                "__builtin_complex(1.0e308,1.0e308)/__builtin_complex(1.0e308,1.0e308)",
                "__builtin_complex(1.0e308,1.0e308)*__builtin_complex(1.0e308,-1.0e308)",
                "__builtin_complex(1.0,1.0)/__builtin_complex(1.0e308,1.0e308)",
                "__builtin_complex(1.0e-308,1.0e-308)/__builtin_complex(1.0e-308,1.0e-308)",
                "__builtin_complex(1.0,2.0)/__builtin_complex(-0.0,0.0)",
                "__builtin_complex(1.0,2.0)/__builtin_complex(__builtin_inf(),__builtin_inf())",
            ]
            .map(str::to_owned),
        );
        // Deterministic exact binary inputs exercise rounding in both products
        // and quotients without introducing a host floating-point oracle.
        let mut state = 0x7a31_004d_1193_aa1du64;
        for _ in 0..64 {
            let mut numbers = Vec::new();
            for _ in 0..4 {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                numbers.push(format!(
                    "{}0x1.{:013x}p{}",
                    if state & 1 == 0 { "-" } else { "" },
                    state & 0x000f_ffff_ffff_ffff,
                    (state % 80) as i32 - 40
                ));
            }
            for op in ["*", "/"] {
                expressions.push(format!(
                    "__builtin_complex({},{}){op}__builtin_complex({},{})",
                    numbers[0], numbers[1], numbers[2], numbers[3]
                ));
            }
        }
        let source = format!(
            r#"#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <float.h>
_Static_assert(sizeof(double)==8 && DBL_MANT_DIG==53 && DBL_MAX_EXP==1024,"binary64 oracle");
_Static_assert(sizeof(double _Complex)==16,"complex storage");
static double _Complex values[]={{ {} }};
int main(void){{for(unsigned i=0;i<sizeof(values)/sizeof(values[0]);i++){{uint64_t bits[2];memcpy(bits,&values[i],sizeof(bits));printf("%016llx %016llx\n",(unsigned long long)bits[0],(unsigned long long)bits[1]);}}return 0;}}
"#,
            expressions.join(",\n")
        );
        let path = temporary.path().join("values.c");
        std::fs::write(&path, source).unwrap();
        let unit = analyze_with_profile("", profile, &AnalysisOptions::default()).unwrap();
        for optimization in ["-O0", "-O2"] {
            let executable = temporary.path().join("values");
            let output = Command::new(compiler)
                .args(["-std=c11", optimization])
                .arg(&path)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(true),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(&executable).output().unwrap();
            assert!(output.status.success());
            let lines = String::from_utf8(output.stdout).unwrap();
            assert_eq!(lines.lines().count(), expressions.len());
            for (source, line) in expressions.iter().zip(lines.lines()) {
                let ArithmeticConstant::Complex(value) = evaluate_arithmetic(unit.unit(), source)
                    .unwrap_or_else(|e| panic!("{compiler}: {source}: {e}"))
                else {
                    panic!("not complex")
                };
                let native: Vec<_> = line
                    .split_whitespace()
                    .map(|part| u128::from_str_radix(part, 16).unwrap())
                    .collect();
                for (actual, expected) in [value.real().to_bits(), value.imaginary().to_bits()]
                    .into_iter()
                    .zip(native)
                {
                    // Arithmetic-generated NaN signs are not specified. Payload
                    // construction is still checked bit-for-bit in PAIRS above.
                    let nan = |bits: u128| {
                        bits & 0x7ff0_0000_0000_0000 == 0x7ff0_0000_0000_0000
                            && bits & 0x000f_ffff_ffff_ffff != 0
                    };
                    assert!(
                        actual == expected || (nan(actual) && nan(expected)),
                        "{compiler} {optimization}: {source}: {actual:016x} != {expected:016x}"
                    );
                }
            }
        }
    }
}

#[test]
fn complex_expression_resources_remain_bounded() {
    let profile = CompilerProfile::default_for(toucan_target::Target::X86_64UnknownLinuxGnu);
    let source = format!(
        "double _Complex z={}1.0i{};",
        "(".repeat(63),
        ")".repeat(63)
    );
    analyze_with_profile(
        &source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    )
    .unwrap();
    for expression in [
        format!("{}1.0i", "- ".repeat(600)),
        vec!["1.0i"; 600].join("+"),
    ] {
        let source = format!("double _Complex z={expression};");
        for retained in [false, true] {
            let error = analyze_with_profile(
                &source,
                profile,
                &AnalysisOptions {
                    retain_code: retained,
                    ..Default::default()
                },
            )
            .unwrap_err();
            assert!(
                error.message.contains("limit") || error.message.contains("depth"),
                "{error}"
            );
        }
    }
}

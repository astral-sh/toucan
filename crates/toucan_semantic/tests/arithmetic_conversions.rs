use toucan_semantic::checked::{Binary, CheckedCode, Conversion, ExprKind, ExprUse, Unary};
use toucan_semantic::{
    AnalysisOptions, ArithmeticConstant, FloatKind, IntegerKind, TypeKind, analyze,
    analyze_with_options, evaluate_arithmetic,
};
use toucan_target::Target;

const SOURCE: &str = r#"
struct Bits { unsigned small: 3; };
enum Small { ZERO, ONE };
void declared(float);
void variadic(int, ...);
void audit(short s, unsigned char u, _Bool b, enum Small e, unsigned int integer,
           float f, double d, long double l, struct Bits *bits) {
    float mixed = s + f;
    float truth = b + f;
    float enumeration = e + f;
    double choice = b ? u : d;
    long double field = bits->small + l;
    s += f;
    f = s;
    f = (float)s;
    declared(s);
    variadic(0, s, f);
    integer = s + integer;
    integer = +s;
    f = -f;
}
_Static_assert(_Generic((short)1 + 0.5f, float: 1, default: 0), "float");
_Static_assert(_Generic(1 ? (unsigned char)1 : 0.5, double: 1, default: 0), "double");
_Static_assert(_Generic((_Bool)1 + 0.5L, long double: 1, default: 0), "long double");
_Static_assert(_Generic(+(short)1, int: 1, default: 0), "promotion");
_Static_assert(_Generic((short)1 + 1U, unsigned int: 1, default: 0), "integer");
"#;

fn expression<'a>(code: &'a CheckedCode, spelling: &str) -> &'a ExprKind {
    code.expressions()
        .find(|(_, node)| {
            &SOURCE[code.occurrence(node.occurrence()).unwrap().source().range()] == spelling
        })
        .unwrap_or_else(|| panic!("missing expression {spelling}"))
        .1
        .kind()
}

fn steps(code: &CheckedCode, operand: &ExprUse, expected: &[(Conversion, TypeKind)]) {
    let actual: Vec<_> = operand
        .conversions()
        .iter()
        .map(|step| {
            (
                step.kind(),
                code.ty(step.target_type()).unwrap().kind.clone(),
            )
        })
        .collect();
    assert_eq!(actual, expected);
    if let Some((_, last)) = expected.last() {
        assert_eq!(&code.ty(operand.effective_type()).unwrap().kind, last);
    }
}

#[test]
fn arithmetic_conversions_follow_c11_order_on_every_target() {
    use Conversion::{
        Arithmetic, Assignment, Conditional, DefaultArgument, ExplicitCast, IntegerPromotion,
        Lvalue,
    };
    use FloatKind::{Double, Float, LongDouble};
    use IntegerKind::{Int, Short, UnsignedChar, UnsignedInt};
    for target in Target::ALL {
        let analysis = analyze_with_options(
            SOURCE,
            target,
            &AnalysisOptions {
                retain_code: true,
                ..AnalysisOptions::default()
            },
        )
        .unwrap();
        let code = analysis.checked().unwrap();
        for (source, source_type) in [
            ("s + f", TypeKind::Integer(Short)),
            ("b + f", TypeKind::Bool),
            ("e + f", TypeKind::Enum(0)),
        ] {
            let ExprKind::Binary { left, .. } = expression(code, source) else {
                panic!()
            };
            steps(
                code,
                left,
                &[(Lvalue, source_type), (Arithmetic, TypeKind::Float(Float))],
            );
        }
        let ExprKind::Conditional { then_value, .. } = expression(code, "b ? u : d") else {
            panic!()
        };
        steps(
            code,
            then_value,
            &[
                (Lvalue, TypeKind::Integer(UnsignedChar)),
                (Conditional, TypeKind::Float(Double)),
            ],
        );
        let ExprKind::Binary { left, .. } = expression(code, "bits->small + l") else {
            panic!()
        };
        steps(
            code,
            left,
            &[
                (Lvalue, TypeKind::Integer(UnsignedInt)),
                (Arithmetic, TypeKind::Float(LongDouble)),
            ],
        );
        let ExprKind::Binary {
            operator: Binary::AssignPlus,
            left,
            computation_type,
            write_back,
            ..
        } = expression(code, "s += f")
        else {
            panic!()
        };
        steps(
            code,
            left,
            &[
                (Lvalue, TypeKind::Integer(Short)),
                (Arithmetic, TypeKind::Float(Float)),
            ],
        );
        assert_eq!(
            code.ty(computation_type.unwrap()).unwrap().kind,
            TypeKind::Float(Float)
        );
        assert_eq!(
            code.ty(write_back.unwrap()).unwrap().kind,
            TypeKind::Integer(Short)
        );
        let ExprKind::Binary { right, .. } = expression(code, "f = s") else {
            panic!()
        };
        steps(
            code,
            right,
            &[
                (Lvalue, TypeKind::Integer(Short)),
                (Assignment, TypeKind::Float(Float)),
            ],
        );
        let ExprKind::Cast { value, .. } = expression(code, "(float)s") else {
            panic!()
        };
        steps(
            code,
            value,
            &[
                (Lvalue, TypeKind::Integer(Short)),
                (ExplicitCast, TypeKind::Float(Float)),
            ],
        );
        let ExprKind::Call { arguments, .. } = expression(code, "declared(s)") else {
            panic!()
        };
        steps(
            code,
            &arguments[0],
            &[
                (Lvalue, TypeKind::Integer(Short)),
                (Assignment, TypeKind::Float(Float)),
            ],
        );
        let ExprKind::Call { arguments, .. } = expression(code, "variadic(0, s, f)") else {
            panic!()
        };
        steps(
            code,
            &arguments[1],
            &[
                (Lvalue, TypeKind::Integer(Short)),
                (DefaultArgument, TypeKind::Integer(Int)),
            ],
        );
        steps(
            code,
            &arguments[2],
            &[
                (Lvalue, TypeKind::Float(Float)),
                (DefaultArgument, TypeKind::Float(Double)),
            ],
        );
        let ExprKind::Binary { left, .. } = expression(code, "s + integer") else {
            panic!()
        };
        steps(
            code,
            left,
            &[
                (Lvalue, TypeKind::Integer(Short)),
                (IntegerPromotion, TypeKind::Integer(Int)),
                (Arithmetic, TypeKind::Integer(UnsignedInt)),
            ],
        );
        let ExprKind::Unary {
            operator: Unary::Plus,
            operand,
            ..
        } = expression(code, "+s")
        else {
            panic!()
        };
        steps(
            code,
            operand,
            &[
                (Lvalue, TypeKind::Integer(Short)),
                (IntegerPromotion, TypeKind::Integer(Int)),
            ],
        );
        let ExprKind::Unary {
            operator: Unary::Minus,
            operand,
            ..
        } = expression(code, "-f")
        else {
            panic!()
        };
        steps(code, operand, &[(Lvalue, TypeKind::Float(Float))]);
    }
}

// These values exercise real target conversion/rounding. Observable values and
// _Generic types are compiler oracles; GCC/Clang intermediate cast trees may add
// or elide equivalent casts and are not the normative C11 conversion sequence.
const CONSTANTS: &[&str] = &[
    "(unsigned long long)((short)7 + 0.5f)",
    "(unsigned long long)(1 ? (unsigned char)255 : 0.5)",
    "(unsigned long long)((_Bool)1 + 0.5L)",
    "(unsigned long long)(16777217U + 0.0f)",
    "(unsigned long long)((unsigned short)65535 + 0.5f)",
    "(unsigned long long)((unsigned char)255 + 0.5)",
    "(unsigned long long)(9007199254740993ULL + 0.0L)",
];

fn constant_values(target: Target) -> Vec<u128> {
    let unit = analyze("", target).unwrap();
    CONSTANTS
        .iter()
        .map(|source| {
            let ArithmeticConstant::Integer(value) = evaluate_arithmetic(&unit, source).unwrap()
            else {
                panic!()
            };
            value.value
        })
        .collect()
}

fn compiler_input(command: &mut std::process::Command, source: &str) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
#[ignore = "requires Clang with all five target backends; run with --include-ignored"]
fn result_types_and_constants_match_clang_targets() {
    use std::process::Command;
    let source = format!(
        "{SOURCE}\nunsigned long long values[] = {{ {} }};\n",
        CONSTANTS.join(",")
    );
    for target in Target::ALL {
        let output = compiler_input(
            Command::new("clang").args([
                "-target",
                target.triple(),
                "-std=c11",
                "-pedantic-errors",
                "-S",
                "-emit-llvm",
                "-x",
                "c",
                "-",
                "-o",
                "-",
            ]),
            &source,
        );
        let ir = String::from_utf8(output.stdout).unwrap();
        let line = ir
            .lines()
            .find(|line| line.starts_with("@values ="))
            .unwrap();
        let actual: Vec<u128> = line
            .split("i64 ")
            .skip(1)
            .map(|part| part.split([',', ']']).next().unwrap().parse().unwrap())
            .collect();
        assert_eq!(actual, constant_values(target), "{target:?}: {line}");
    }
}

// MinGW long double differs from the MSVC target. Windows values are covered
// by the explicit Clang target probe above.
#[cfg(not(target_os = "windows"))]
#[test]
#[ignore = "requires native GNU GCC and Clang; run with --include-ignored"]
fn result_types_and_values_match_native_compilers() {
    use std::process::Command;
    let target = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            Target::Aarch64AppleDarwin
        } else {
            Target::X86_64AppleDarwin
        }
    } else if cfg!(target_arch = "aarch64") {
        Target::Aarch64UnknownLinuxGnu
    } else {
        Target::X86_64UnknownLinuxGnu
    };
    let directory = tempfile::tempdir().unwrap();
    let source = format!(
        "#include <stdio.h>\n{SOURCE}\nvoid declared(float x) {{ (void)x; }}\nvoid variadic(int x, ...) {{ (void)x; }}\nint main(void) {{ unsigned long long values[]={{ {} }}; for (unsigned i=0;i<sizeof values/sizeof values[0];i++) printf(\"%llu\\n\",values[i]); return 0; }}\n",
        CONSTANTS.join(",")
    );
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        "clang".into(),
    ] {
        let output = compiler_input(
            Command::new(&compiler).args(["-dM", "-E", "-x", "c", "-"]),
            "",
        );
        let macros = String::from_utf8(output.stdout).unwrap();
        if compiler != "clang" {
            assert!(
                macros.contains("#define __GNUC__") && !macros.contains("#define __clang__"),
                "TOUCAN_GCC must select GNU GCC"
            );
        }
        let binary = directory.path().join("probe");
        compiler_input(
            Command::new(&compiler)
                .args(["-std=c11", "-pedantic-errors", "-x", "c", "-", "-o"])
                .arg(&binary),
            &source,
        );
        let output = Command::new(&binary).output().unwrap();
        assert!(output.status.success());
        let actual: Vec<u128> = String::from_utf8(output.stdout)
            .unwrap()
            .split_whitespace()
            .map(|word| word.parse().unwrap())
            .collect();
        assert_eq!(actual, constant_values(target), "{compiler}");
    }
}

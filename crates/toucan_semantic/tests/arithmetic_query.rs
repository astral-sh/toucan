use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::{
    ArithmeticConstant, FloatKind, FloatingFormat, FloatingValue, analyze, evaluate_arithmetic,
    evaluate_integer,
};
use toucan_target::Target;

const FLOATS: &[(&str, FloatKind)] = &[
    ("__builtin_inff()", FloatKind::Float),
    ("-__builtin_inf()", FloatKind::Double),
    ("__builtin_huge_valf()", FloatKind::Float),
    ("__builtin_huge_val()", FloatKind::Double),
    ("(double)__builtin_infl()", FloatKind::Double),
    ("(float)__builtin_huge_vall()", FloatKind::Float),
    ("__builtin_inff() + 1.0f", FloatKind::Float),
    ("-1.0 / __builtin_inf()", FloatKind::Double),
    ("0.0f", FloatKind::Float),
    ("-0.0f", FloatKind::Float),
    ("0.1f", FloatKind::Float),
    ("0x1p-149f", FloatKind::Float),
    ("0x1.fffffep127f", FloatKind::Float),
    ("0x1p-149f / 2.0f", FloatKind::Float),
    ("-0x1p-149f / 2.0f", FloatKind::Float),
    ("1.0f + 0x1p-24f", FloatKind::Float),
    ("(float)0x8000008000000001ULL", FloatKind::Float),
    ("(float)(1.0 + 0x1p-24)", FloatKind::Float),
    ("(16777216.0f + 1.0f) - 16777216.0f", FloatKind::Float),
    ("-0.0", FloatKind::Double),
    ("0.1", FloatKind::Double),
    ("1.0 / 3.0", FloatKind::Double),
    ("0x1p-1074", FloatKind::Double),
    ("0x1.fffffffffffffp1023", FloatKind::Double),
    ("1 ? 7 : 0.0", FloatKind::Double),
    ("(double)(~0UL)", FloatKind::Double),
    ("(double)((0x1p63L + 1.0L) - 0x1p63L)", FloatKind::Double),
    ("(double)((0x1p100L + 1.0L) - 0x1p100L)", FloatKind::Double),
];

fn floating(expression: &str, target: Target) -> FloatingValue {
    let ArithmeticConstant::Floating(value) =
        evaluate_arithmetic(&analyze("", target).unwrap(), expression).unwrap()
    else {
        panic!("expected floating result: {expression}");
    };
    value
}

#[test]
fn query_retains_type_bits_and_integer_expression_boundary() {
    for target in Target::ALL {
        for &(expression, kind) in FLOATS {
            assert_eq!(
                floating(expression, target).kind(),
                kind,
                "{target}: {expression}"
            );
        }
        assert_eq!(floating("-0.0f", target).to_bits(), 0x80000000);
        assert_eq!(floating("-0.0", target).to_bits(), 0x8000000000000000);
        assert_eq!(floating("0.1f", target).to_bits(), 0x3dcccccd);
        assert_eq!(floating("0.1", target).to_bits(), 0x3fb999999999999a);
        assert_eq!(floating("0x1p-149f", target).to_bits(), 1);
        assert_eq!(floating("0x1p-1074", target).to_bits(), 1);
        let unit = analyze("typedef float Sample; enum { COUNT=3 };", target).unwrap();
        let ArithmeticConstant::Floating(value) =
            evaluate_arithmetic(&unit, "(Sample)COUNT / 2.0f").unwrap()
        else {
            panic!("floating");
        };
        assert_eq!(value.to_bits(), 0x3fc00000);
        for expression in ["(int)(1.0 + 2.0)", "1.0 < 2.0"] {
            assert!(evaluate_integer(&unit, expression).is_err());
            assert!(matches!(
                evaluate_arithmetic(&unit, expression).unwrap(),
                ArithmeticConstant::Integer(_)
            ));
        }
        let value = floating("1.0L", target);
        assert_eq!(value.kind(), FloatKind::LongDouble);
        let (format, bits) = match target {
            Target::X86_64UnknownLinuxGnu | Target::X86_64AppleDarwin => {
                (FloatingFormat::X87, 0x3fff8000000000000000)
            }
            Target::Aarch64UnknownLinuxGnu => (
                FloatingFormat::Binary128,
                0x3fff0000000000000000000000000000,
            ),
            _ => (FloatingFormat::Binary64, 0x3ff0000000000000),
        };
        assert_eq!((value.format(), value.to_bits()), (format, bits));
    }
}

#[test]
fn query_reports_nonfinite_values_and_offsets() {
    let unit = analyze("typedef double Real;", Target::X86_64UnknownLinuxGnu).unwrap();
    for (expression, reason) in [
        ("1.0 / 0.0", "division by zero"),
        ("1e9999", "overflow"),
        ("0.0 / 0.0", "invalid operation"),
        ("__builtin_nanf(\"invalid\")", "payload"),
        ("(Real)1.0 + missing", "missing"),
    ] {
        let error = evaluate_arithmetic(&unit, expression).unwrap_err();
        assert!(error.message.contains(reason), "{expression}: {error}");
        assert!(error.offset <= expression.len(), "{error}");
    }
    for expression in ["1.0); int injected; (0", "1.0; 2.0", "({ return 1.0; })"] {
        assert!(evaluate_arithmetic(&unit, expression).is_err());
    }
}

fn compiler_input(command: &mut Command, source: &str) -> std::process::Output {
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
    child.wait_with_output().unwrap()
}

#[test]
#[ignore = "requires Clang with all five target backends; run with --include-ignored"]
fn exact_bits_match_clang_on_every_target() {
    for target in Target::ALL {
        let mut source = String::new();
        let mut expected = Vec::new();
        for (index, &(expression, kind)) in FLOATS.iter().enumerate() {
            let (c_type, integer) = if kind == FloatKind::Float {
                ("float", "unsigned")
            } else {
                ("double", "unsigned long long")
            };
            source.push_str(&format!("{integer} bits_{index}(void) {{ {c_type} value = {expression}; {integer} bits; __builtin_memcpy(&bits, &value, sizeof value); return bits; }}\n"));
            expected.push(floating(expression, target).to_bits());
        }
        for (index, expression) in [
            "-0.0L",
            "0.1L",
            "0x1p63L + 1.0L",
            "0x1p100L + 1.0L",
            "__builtin_infl()",
            "-__builtin_huge_vall()",
        ]
        .iter()
        .enumerate()
        {
            let value = floating(expression, target);
            let count = match value.format() {
                FloatingFormat::X87 => 2,
                FloatingFormat::Binary128 => 8,
                _ => 0,
            };
            source.push_str(&format!("unsigned long long wide_{index}_lo(void) {{ long double value = {expression}; unsigned long long bits; __builtin_memcpy(&bits, &value, 8); return bits; }}\n"));
            source.push_str(&format!("unsigned long long wide_{index}_hi(void) {{ long double value = {expression}; unsigned long long bits = 0; __builtin_memcpy(&bits, (const unsigned char *)&value + 8, {count}); return bits; }}\n"));
            expected.push(value.to_bits() as u64 as u128);
            expected.push(value.to_bits() >> 64);
        }
        let output = compiler_input(
            Command::new("clang").args([
                "-target",
                target.triple(),
                "-std=c11",
                "-O2",
                "-ffp-contract=off",
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
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ir = String::from_utf8(output.stdout).unwrap();
        let actual: Vec<u128> = ir
            .lines()
            .filter_map(|line| {
                let line = line.trim().strip_prefix("ret i")?;
                let (width, value) = line.split_once(' ')?;
                let value = value.parse::<i128>().unwrap() as u128;
                Some(if width == "32" {
                    value as u32 as u128
                } else {
                    value as u64 as u128
                })
            })
            .collect();
        assert_eq!(actual, expected, "{target}: {ir}");
    }
}

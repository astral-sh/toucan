use std::process::Command;

use toucan_semantic::checked::{Builtin, ExprKind};
use toucan_semantic::{
    AnalysisOptions, ArithmeticConstant, FloatKind, FloatingFormat, analyze, analyze_with_options,
    evaluate_arithmetic, evaluate_integer,
};
use toucan_target::Target;

const INTRINSICS: &[(&str, &str, FloatKind)] = &[
    ("__builtin_inff", "float", FloatKind::Float),
    ("__builtin_inf", "double", FloatKind::Double),
    ("__builtin_infl", "long double", FloatKind::LongDouble),
    ("__builtin_huge_valf", "float", FloatKind::Float),
    ("__builtin_huge_val", "double", FloatKind::Double),
    ("__builtin_huge_vall", "long double", FloatKind::LongDouble),
];
const INVALID: &[&str] = &[
    "float f(void) { return __builtin_inff(1); }",
    "double f(void) { return __builtin_inf(0, 1); }",
    "long double f(void) { return __builtin_infl(0); }",
    "float f(void) { return __builtin_huge_valf(0); }",
    "double f(void) { return __builtin_huge_val(0); }",
    "long double f(void) { return __builtin_huge_vall(0); }",
    "void f(void) { __builtin_inff() = 0; }",
    "void f(int __builtin_inf) { __builtin_inf(); }",
    "float (*p)(void) = __builtin_inff;",
];

fn source() -> String {
    let mut source =
        String::from("float shadow(float (*__builtin_inf)(int)) { return __builtin_inf(1); }\n");
    for (index, (name, ty, _)) in INTRINSICS.iter().enumerate() {
        source.push_str(&format!("_Static_assert(_Generic({name}(), {ty}:1, default:0), \"type\");\nstatic {ty} value_{index} = {name}();\n{ty} get_{index}(void) {{ return {name}(); }}\n"));
    }
    source
}

#[test]
fn infinity_types_values_and_retained_calls_match() {
    for target in Target::ALL {
        let text = source();
        let plain = analyze(&text, target).unwrap();
        let retained = analyze_with_options(
            &text,
            target,
            &AnalysisOptions {
                retain_code: true,
                ..AnalysisOptions::default()
            },
        )
        .unwrap();
        assert_eq!(format!("{plain:?}"), format!("{:?}", retained.unit()));
        let code = retained.checked().unwrap();
        let mut calls = 0;
        for (_, expression) in code.expressions() {
            if let ExprKind::BuiltinCall {
                builtin, arguments, ..
            } = expression.kind()
            {
                assert!(matches!(
                    builtin,
                    Builtin::Infinity
                        | Builtin::InfinityFloat
                        | Builtin::InfinityLongDouble
                        | Builtin::HugeValue
                        | Builtin::HugeValueFloat
                        | Builtin::HugeValueLongDouble
                ));
                assert!(arguments.is_empty());
                calls += 1;
            }
        }
        assert_eq!(calls, 18);
        for (name, _, kind) in INTRINSICS {
            let expression = format!("{name}()");
            let ArithmeticConstant::Floating(value) =
                evaluate_arithmetic(&plain, &expression).unwrap()
            else {
                panic!("float")
            };
            assert_eq!(value.kind(), *kind);
            let expected = match value.format() {
                FloatingFormat::Binary32 => 0x7f800000,
                FloatingFormat::Binary64 => 0x7ff0000000000000,
                FloatingFormat::X87 => 0x7fff8000000000000000,
                FloatingFormat::Binary128 => 0x7fff0000000000000000000000000000,
            };
            assert_eq!(value.to_bits(), expected, "{target}: {expression}");
            assert!(evaluate_integer(&plain, &expression).is_err());
        }
        for (expression, expected) in [
            ("(_Bool)__builtin_inf()", 1),
            ("__builtin_inf() > 1.0", 1),
            ("!__builtin_inff()", 0),
        ] {
            let ArithmeticConstant::Integer(value) =
                evaluate_arithmetic(&plain, expression).unwrap()
            else {
                panic!("integer")
            };
            assert_eq!(value.value, expected);
        }
        for (expression, message) in [
            (
                "(int)__builtin_inf()",
                "outside the destination integer range",
            ),
            (
                "(unsigned long long)-__builtin_infl()",
                "outside the destination integer range",
            ),
            ("__builtin_inf() - __builtin_inf()", "invalid operation"),
            ("1e9999", "overflow"),
            ("1.0 / 0.0", "division by zero"),
        ] {
            let error = evaluate_arithmetic(&plain, expression).unwrap_err();
            assert!(
                error.message.contains(message),
                "{target}: {expression}: {error}"
            );
        }
        for source in INVALID {
            let plain = analyze(source, target).unwrap_err();
            let retained = analyze_with_options(
                source,
                target,
                &AnalysisOptions {
                    retain_code: true,
                    ..AnalysisOptions::default()
                },
            )
            .unwrap_err();
            assert_eq!(
                (plain.offset, plain.message),
                (retained.offset, retained.message)
            );
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang cross targets; run with --include-ignored"]
fn infinity_signatures_match_native_gcc_and_clang_targets() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("infinity.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for (compiler, targets) in [
        (gcc.as_str(), vec![None]),
        ("clang", Target::ALL.into_iter().map(Some).collect()),
    ] {
        for target in targets {
            for (source, valid) in std::iter::once((source(), true))
                .chain(INVALID.iter().map(|source| ((*source).to_owned(), false)))
            {
                std::fs::write(&input, &source).unwrap();
                let mut command = Command::new(compiler);
                command.args(["-std=c11", "-pedantic-errors", "-fsyntax-only"]);
                if let Some(target) = target {
                    command.arg(format!("--target={target}"));
                }
                let output = command.arg(&input).output().unwrap();
                assert_eq!(
                    output.status.success(),
                    valid,
                    "{compiler} {target:?}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

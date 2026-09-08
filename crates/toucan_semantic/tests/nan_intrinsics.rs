use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::checked::{Builtin, Conversion, ExprKind};
use toucan_semantic::{
    AnalysisOptions, ArithmeticConstant, FloatKind, FloatingFormat, FloatingValue, IntegerKind,
    TypeKind, analyze, analyze_with_options, evaluate_arithmetic, evaluate_integer,
};
use toucan_target::Target;

const INTRINSICS: &[(&str, &str)] = &[
    ("__builtin_nanf", "float"),
    ("__builtin_nan", "double"),
    ("__builtin_nanl", "long double"),
    ("__builtin_nansf", "float"),
    ("__builtin_nans", "double"),
    ("__builtin_nansl", "long double"),
];
const PAYLOADS: &[&str] = &[
    "",
    "0",
    "1",
    "077",
    "0x12345",
    "0xffffffffffffffff",
    "0x10000000000000000",
    "0xffffffffffffffffffffffffffffffff",
    "0x100000000000000000000000000000000",
    r"\x31",
];
const EXTRA: &[(&str, &str)] = &[
    ("float", "__builtin_nanf(\"1\") * -1.0f"),
    ("float", "__builtin_nanf(\"1\") / -1.0f"),
    ("float", "__builtin_nanf(\"1\") * 0.0f"),
    ("float", "__builtin_nanf(\"1\") - __builtin_nanf(\"2\")"),
    ("float", "__builtin_nanf(\"1\") / __builtin_nanf(\"2\")"),
    ("float", "+__builtin_nansf(\"1\")"),
    ("double", "-__builtin_nans(\"1\")"),
    ("float", "(float)__builtin_nansf(\"1\")"),
    ("double", "(double)__builtin_nansf(\"1\")"),
    ("float", "(float)__builtin_nans(\"1\")"),
    ("long double", "(long double)__builtin_nans(\"1\")"),
    ("double", "(double)__builtin_nansl(\"1\")"),
    ("float", "__builtin_nanf(\"1\") + 2.0f"),
    ("float", "2.0f + __builtin_nanf(\"2\")"),
    ("float", "__builtin_nanf(\"1\") + __builtin_nanf(\"2\")"),
    ("double", "__builtin_nan((const char *)\"1\")"),
];

fn expressions() -> Vec<(&'static str, String)> {
    INTRINSICS
        .iter()
        .flat_map(|(name, ty)| {
            PAYLOADS
                .iter()
                .map(move |payload| (*ty, format!("{name}(\"{payload}\")")))
        })
        .chain(
            EXTRA
                .iter()
                .map(|(ty, expression)| (*ty, (*expression).to_owned())),
        )
        .collect()
}

fn floating(expression: &str, target: Target) -> FloatingValue {
    let ArithmeticConstant::Floating(value) =
        evaluate_arithmetic(&analyze("", target).unwrap(), expression)
            .unwrap_or_else(|error| panic!("{target}: {expression}: {error}"))
    else {
        panic!("float")
    };
    value
}

#[test]
fn nan_payloads_preserve_signaling_and_runtime_signatures() {
    for target in Target::ALL {
        for (ty, expression) in expressions() {
            let kind = match ty {
                "float" => FloatKind::Float,
                "double" => FloatKind::Double,
                _ => FloatKind::LongDouble,
            };
            assert_eq!(floating(&expression, target).kind(), kind);
        }
        for (expression, bits) in [
            ("__builtin_nanf(\"\")", 0x7fc00000),
            ("__builtin_nansf(\"\")", 0x7fa00000),
            ("__builtin_nansf(\"1\")", 0x7f800001),
            ("-__builtin_nansf(\"1\")", 0xff800001),
            ("__builtin_nan(\"0x12345\")", 0x7ff8000000012345),
            ("__builtin_nans(\"1\")", 0x7ff0000000000001),
        ] {
            assert_eq!(floating(expression, target).to_bits(), bits);
        }
        let mut source = String::from(
            "double shadow(double (*__builtin_nan)(int)) { return __builtin_nan(1); }\nstruct __builtin_nan { int member; };\n",
        );
        for (index, (name, ty)) in INTRINSICS.iter().enumerate() {
            source.push_str(&format!("_Static_assert(_Generic({name}(\"1\"), {ty}:1, default:0), \"type\");\nstatic {ty} value_{index}={name}(\"1\");\n{ty} runtime_{index}(const char *p) {{ return {name}(p); }}\n"));
        }
        let ordinary = analyze(&source, target).unwrap();
        let retained = analyze_with_options(
            &source,
            target,
            &AnalysisOptions {
                retain_code: true,
                ..AnalysisOptions::default()
            },
        )
        .unwrap();
        assert_eq!(format!("{ordinary:?}"), format!("{:?}", retained.unit()));
        let code = retained.checked().unwrap();
        let mut calls = 0;
        for (_, expression) in code.expressions() {
            if let ExprKind::BuiltinCall {
                builtin, arguments, ..
            } = expression.kind()
            {
                assert!(matches!(
                    builtin,
                    Builtin::Nan
                        | Builtin::NanFloat
                        | Builtin::NanLongDouble
                        | Builtin::SignalingNan
                        | Builtin::SignalingNanFloat
                        | Builtin::SignalingNanLongDouble
                ));
                assert_eq!(arguments.len(), 1);
                let TypeKind::Pointer(character) =
                    &code.ty(arguments[0].effective_type()).unwrap().kind
                else {
                    panic!("pointer")
                };
                assert!(character.qualifiers.is_const);
                assert_eq!(character.kind, TypeKind::Integer(IntegerKind::Char));
                if matches!(
                    code.expression(arguments[0].expression()).unwrap().kind(),
                    ExprKind::String(_)
                ) {
                    assert!(
                        arguments[0]
                            .conversions()
                            .iter()
                            .any(|step| step.kind() == Conversion::Assignment)
                    );
                }
                calls += 1;
            }
        }
        assert_eq!(calls, 18);
        for expression in [
            "(_Bool)__builtin_nans(\"1\")",
            "__builtin_nan(\"1\") != __builtin_nan(\"1\")",
        ] {
            let ArithmeticConstant::Integer(value) =
                evaluate_arithmetic(&ordinary, expression).unwrap()
            else {
                panic!("integer")
            };
            assert_eq!(value.value, 1);
        }
        assert!(evaluate_integer(&ordinary, "__builtin_nan(\"1\")").is_err());
        for (expression, reason) in [
            (
                "(int)__builtin_nan(\"1\")",
                "outside the destination integer range",
            ),
            (
                "(unsigned)__builtin_nans(\"1\")",
                "outside the destination integer range",
            ),
            ("__builtin_nansf(\"1\") * 1.0f", "signaling NaN"),
            ("__builtin_nan(\"invalid\")", "payload"),
            ("__builtin_nan(\" 1\")", "payload"),
            ("__builtin_nan(\"+1\")", "payload"),
            ("__builtin_nan(\"08\")", "payload"),
            ("__builtin_nan(\"0x\")", "payload"),
            ("__builtin_nan(0)", "string literal"),
            ("__builtin_nan(\"1\\0ignored\")", "embedded NUL"),
            ("__builtin_nan((const char *)(int)\"1\")", "string literal"),
        ] {
            let error = evaluate_arithmetic(&ordinary, expression).unwrap_err();
            assert!(
                error.message.contains(reason),
                "{target}: {expression}: {error}"
            );
        }
        assert!(
            evaluate_arithmetic(
                &ordinary,
                &format!("__builtin_nan(\"{}\")", "1".repeat(4097))
            )
            .unwrap_err()
            .message
            .contains("4096-byte")
        );
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
fn nan_exact_bits_match_clang_targets() {
    for target in Target::ALL {
        let mut source = String::new();
        let mut expected = Vec::new();
        for (index, (ty, expression)) in expressions().iter().enumerate() {
            let value = floating(expression, target);
            let count = if *ty == "float" { 4 } else { 8 };
            source.push_str(&format!("unsigned long long low_{index}(void) {{ {ty} value={expression}; unsigned long long bits=0; __builtin_memcpy(&bits,&value,{count}); return bits; }}\n"));
            expected.push(value.to_bits() as u64);
            if *ty == "long double" {
                let count = match value.format() {
                    FloatingFormat::X87 => 2,
                    FloatingFormat::Binary128 => 8,
                    _ => 0,
                };
                source.push_str(&format!("unsigned long long high_{index}(void) {{ {ty} value={expression}; unsigned long long bits=0; __builtin_memcpy(&bits,(const char*)&value+8,{count}); return bits; }}\n"));
                expected.push((value.to_bits() >> 64) as u64);
            }
        }
        let result = compiler_input(
            Command::new("clang").args([
                "-target",
                target.triple(),
                "-std=c11",
                "-O2",
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
            result.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let ir = String::from_utf8(result.stdout).unwrap();
        let actual: Vec<u64> = ir
            .lines()
            .filter_map(|line| {
                line.trim().strip_prefix("ret i64 ").map(|value| {
                    value
                        .parse::<i64>()
                        .unwrap_or_else(|_| panic!("{target}: {line}")) as u64
                })
            })
            .collect();
        assert_eq!(actual, expected, "{target}: {ir}");
    }
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn nan_signatures_match_compilers() {
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for (compiler, targets) in [
        (gcc.as_str(), vec![None]),
        ("clang", Target::ALL.into_iter().map(Some).collect()),
    ] {
        for target in targets {
            for (name, ty) in INTRINSICS {
                let valid = format!(
                    "_Static_assert(_Generic({name}(\"1\"),{ty}:1,default:0),\"type\");\n{ty} f(const char *p){{return {name}(p);}}\n"
                );
                for (source, accepted) in [
                    (valid, true),
                    (format!("void f(void){{{name}();}}\n"), false),
                    (format!("void f(void){{{name}(\"\",\"\");}}\n"), false),
                    (format!("void f(void){{{name}(L\"1\");}}\n"), false),
                ] {
                    let selected = target.unwrap_or(Target::X86_64UnknownLinuxGnu);
                    let plain = analyze(&source, selected);
                    let retained = analyze_with_options(
                        &source,
                        selected,
                        &AnalysisOptions {
                            retain_code: true,
                            ..AnalysisOptions::default()
                        },
                    );
                    assert_eq!(plain.is_ok(), accepted, "{source}");
                    match (plain, retained) {
                        (Ok(plain), Ok(retained)) => {
                            assert_eq!(format!("{plain:?}"), format!("{:?}", retained.unit()))
                        }
                        (Err(plain), Err(retained)) => assert_eq!(
                            (plain.offset, plain.message),
                            (retained.offset, retained.message)
                        ),
                        _ => panic!("retention changed acceptance: {source}"),
                    }
                    let mut command = Command::new(compiler);
                    command.args([
                        "-std=c11",
                        "-pedantic-errors",
                        "-Werror=incompatible-pointer-types",
                        "-fsyntax-only",
                        "-x",
                        "c",
                        "-",
                    ]);
                    if let Some(target) = target {
                        command.arg(format!("--target={target}"));
                    }
                    let output = compiler_input(&mut command, &source);
                    assert_eq!(
                        toucan_test_support::compiler_acceptance(&output),
                        Ok(accepted),
                        "{compiler} {target:?}: {source}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "requires native GCC/Clang; run with --include-ignored"]
fn nan_object_bytes_match_native_compilers() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let mut source = String::new();
    let mut body = String::new();
    for (index, (ty, expression)) in expressions().iter().enumerate() {
        let value = floating(expression, target);
        let bytes = match value.format() {
            FloatingFormat::Binary16 | FloatingFormat::BFloat16 => 2,
            FloatingFormat::Binary32 => 4,
            FloatingFormat::Binary64 => 8,
            FloatingFormat::X87 => 10,
            FloatingFormat::Binary128 => 16,
        };
        source.push_str(&format!("static const {ty} value_{index}={expression};\n"));
        // Byte loads avoid floating operations that could quiet a signaling NaN;
        // volatile also prevents the compiler from replacing all comparisons.
        for byte in 0..bytes {
            let expected = (value.to_bits() >> (byte * 8)) & 255;
            body.push_str(&format!("if (((const volatile unsigned char *)&value_{index})[{byte}] != {expected}) return {};\n",index+1));
        }
    }
    source.push_str(&format!("int main(void) {{ {body} return 0; }}\n"));
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("nan.c");
    let executable = directory.path().join("nan");
    std::fs::write(&input, &source).unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let compilers = if cfg!(target_os = "linux") {
        vec![gcc.as_str(), "clang"]
    } else {
        vec!["clang"]
    };
    for compiler in compilers {
        for optimization in ["-O0", "-O2"] {
            let output = Command::new(compiler)
                .args(["-std=c11", optimization, "-Wall", "-Wextra", "-Werror"])
                .arg(&input)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler} {optimization}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let status = Command::new(&executable).status().unwrap();
            assert!(
                status.success(),
                "{compiler} {optimization}: first mismatched expression index {:?}",
                status.code()
            );
        }
    }
}

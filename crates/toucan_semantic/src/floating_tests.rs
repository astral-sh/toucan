use std::io::Write;
use std::process::{Command, Output, Stdio};

use lang_c::{ast, driver};
use toucan_target::Target;

use super::{ArithmeticValue, Float};
use crate::analyze::Analyzer;
use crate::{Error, analyze};

const EXPRESSIONS: &[&str] = &[
    "1.0 < 2.0",
    "(int)-3.75",
    "-0.0 == 0.0",
    "(_Bool)-0.0",
    "(_Bool)0x1p-149f",
    "1.0f + 0x1p-24f == 1.0f",
    "1.0f + 0x1p-23f > 1.0f",
    "(16777216.0f + 1.0f) - 16777216.0f",
    "(16777216.0 + 1.0) - 16777216.0",
    "0.0 ? 99.0 : 42",
    "1.0 ? 42 : 99.0",
    "1.0 || 1 / 0",
    "0.0 && 1 / 0",
    "!0.0",
    "!1.0",
    "(unsigned)-0.5",
    "(float)16777217 == 16777216.0f",
    "(float)0x8000008000000001ULL == 0x1.000002p63f",
    "0x1p-149f + 0x1p-149f == 0x1p-148f",
    "0x1p-149f / 2.0f == 0.0f",
    "0x1p53L + 1.0L != 0x1p53L",
    "0x1p63L + 1.0L != 0x1p63L",
    "0x1p64L + 1.0L != 0x1p64L",
    "0x1p112L + 1.0L != 0x1p112L",
    "0x1p113L + 1.0L != 0x1p113L",
    "(double)(0x1p53L + 1.0L) == 0x1p53",
    "(float)(1.0 + 0x1p-24) == 1.0f",
    "(int)(1.0 + 2.0) * 7",
    "(int)0x1.fp2",
    "(unsigned long long)9007199254740993.0",
    "0.1f == (float)0.1",
    "(int)((double)1 / 4.0 * 20.0)",
];

fn evaluate(expression: &str, target: Target) -> Result<ArithmeticValue, Error> {
    let source = format!("int value = ({expression});\n");
    let config = driver::Config {
        flavor: driver::Flavor::ClangC11,
        ..driver::Config::default()
    };
    let parsed = driver::parse_preprocessed(&config, source).unwrap();
    let ast::ExternalDeclaration::Declaration(declaration) = &parsed.unit.0[0].node else {
        panic!("declaration")
    };
    let ast::Initializer::Expression(expression) = &declaration.node.declarators[0]
        .node
        .initializer
        .as_ref()
        .unwrap()
        .node
    else {
        panic!("expression")
    };
    Analyzer::from_unit(analyze("", target).unwrap(), &parsed.arena).eval_arithmetic(expression)
}

fn values(target: Target) -> Vec<u128> {
    EXPRESSIONS
        .iter()
        .map(|expression| {
            evaluate(&format!("(unsigned long long)({expression})"), target)
                .unwrap_or_else(|error| panic!("{target:?}: {expression}: {error}"))
                .integer(0)
                .unwrap()
                .value
        })
        .collect()
}

fn compiler_input(command: &mut Command, source: &str) -> Output {
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
fn arithmetic_rounding_retains_target_precision_and_signed_zero() {
    for target in Target::ALL {
        let values = values(target);
        assert_eq!(&values[..8], &[1, u64::MAX as u128 - 2, 1, 0, 1, 1, 1, 0]);
        let wide = matches!(
            target,
            Target::I686UnknownLinuxGnu
                | Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::X86_64AppleDarwin
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        );
        assert_eq!(values[20], u128::from(wide));
        assert_eq!(values[21], u128::from(wide));
        assert_eq!(
            values[22],
            u128::from(matches!(
                target,
                Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
            ))
        );
        assert_eq!(
            values[23],
            u128::from(matches!(
                target,
                Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
            ))
        );
        assert_eq!(values[24], 0);
        assert_eq!(values[17], 1, "integer conversion must not double round");
        let ArithmeticValue::Floating { value, .. } = evaluate("-0.0 + -0.0", target).unwrap()
        else {
            panic!("floating")
        };
        assert!(value.is_zero() && value.is_negative());
    }
}

#[test]
#[ignore = "requires native C compilers; run with --include-ignored"]
fn arithmetic_values_match_native_compilers() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => panic!("unsupported native compiler target"),
    };
    let temporary = tempfile::tempdir().unwrap();
    let executable = temporary.path().join("floating-probe");
    let source = format!(
        "#include <stdio.h>\nunsigned long long values[] = {{ {} }};\nint main(void) {{ for (unsigned i=0; i<sizeof(values)/sizeof(values[0]); ++i) printf(\"%llu\\n\", values[i]); }}\n",
        EXPRESSIONS.join(", ")
    );
    let compilers: &[&str] = if cfg!(target_os = "linux") {
        &["gcc", "clang"]
    } else {
        &["clang"]
    };
    for compiler in compilers {
        let output = compiler_input(
            Command::new(compiler)
                .args(["-std=c11", "-x", "c", "-", "-o"])
                .arg(&executable),
            &source,
        );
        assert!(
            output.status.success(),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(&executable).output().unwrap();
        assert!(output.status.success());
        let actual: Vec<u128> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|value| value.parse().unwrap())
            .collect();
        assert_eq!(actual, values(target), "{compiler}");
    }
}

#[test]
#[ignore = "requires Clang with all five target backends; run with --include-ignored"]
fn arithmetic_values_match_clang_on_every_target() {
    let source = format!(
        "unsigned long long values[] = {{ {} }};\n",
        EXPRESSIONS.join(", ")
    );
    for target in Target::ALL {
        let output = compiler_input(
            Command::new("clang").args([
                "-target",
                target.triple(),
                "-std=c11",
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
            "{target:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ir = String::from_utf8(output.stdout).unwrap();
        let line = ir
            .lines()
            .find(|line| line.starts_with("@values ="))
            .unwrap();
        let actual: Vec<u128> = line
            .split("i64 ")
            .skip(1)
            .map(|value| {
                let value = value
                    .split([',', ']'])
                    .next()
                    .unwrap()
                    .parse::<i128>()
                    .unwrap();
                value as u64 as u128
            })
            .collect();
        assert_eq!(actual, values(target), "{target:?}: {line}");
    }
}

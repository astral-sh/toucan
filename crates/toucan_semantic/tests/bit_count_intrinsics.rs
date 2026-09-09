use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options, evaluate_integer};
use toucan_target::Target;

const NAMES: [&str; 6] = [
    "__builtin_clz",
    "__builtin_clzl",
    "__builtin_clzll",
    "__builtin_ctz",
    "__builtin_ctzl",
    "__builtin_ctzll",
];

fn constants(target: Target) -> Vec<(String, u128)> {
    let mut values = vec![
        ("__builtin_clz(1)".into(), 31),
        ("__builtin_clz(0x80000000U)".into(), 0),
        ("__builtin_clz(-1)".into(), 0),
        ("__builtin_ctz(-8)".into(), 3),
        ("__builtin_ctz(0x80000000U)".into(), 31),
        ("__builtin_ctz(0x100000008ULL)".into(), 3),
        ("__builtin_clz(0x100000001ULL)".into(), 31),
        ("__builtin_ctz(8.9)".into(), 3),
        ("__builtin_clzll(1)".into(), 63),
        ("__builtin_ctzll(0x8000000000000000ULL)".into(), 63),
        ("__builtin_clzll(-1)".into(), 0),
        ("__builtin_ctzll(__builtin_ctzll(256))".into(), 3),
        (
            "__builtin_clzl(1)".into(),
            u128::from(target.long_width() - 1),
        ),
        ("__builtin_clzl(0x100000001ULL)".into(), 31),
        ("__builtin_ctzl(0x100000008ULL)".into(), 3),
    ];
    // Exercise every bit position in each target parameter width.
    for name in NAMES {
        let bits = if name.ends_with("ll") {
            64
        } else if name.ends_with('l') {
            target.long_width()
        } else {
            32
        };
        for bit in 0..bits {
            values.push((
                format!("{name}(1ULL << {bit})"),
                u128::from(if name.starts_with("__builtin_clz") {
                    bits - bit - 1
                } else {
                    bit
                }),
            ));
        }
    }
    values
}

fn valid() -> Vec<String> {
    let mut cases = NAMES
        .map(|name| format!("int f(double value) {{ return {name}(value); }}"))
        .to_vec();
    cases.extend([
        "int f(void) { return __builtin_clz(0); }".into(),
        "void f(unsigned *p) { __builtin_ctz(++*p); }".into(),
        "void f(long (*__builtin_clz)(int)) { _Static_assert(_Generic(__builtin_clz(1), long:1, default:0), \"shadow\"); }".into(),
    ]);
    cases
}

fn invalid() -> Vec<String> {
    let mut cases = Vec::new();
    for name in NAMES {
        for arguments in ["", "1, 2", "(void*)0", "(void)0", "(struct S){1}"] {
            cases.push(format!(
                "struct S {{ int x; }}; void f(void) {{ {name}({arguments}); }}"
            ));
        }
    }
    cases.push("void f(int __builtin_clz) { __builtin_clz(1); }".into());
    cases.push("void f(void) { __builtin_ctz(1) = 1; }".into());
    cases
}

#[test]
fn bit_counts_convert_arguments_and_return_target_c_int() {
    for target in Target::ALL {
        let unit = analyze("", target).unwrap();
        for (expression, expected) in constants(target) {
            let value = evaluate_integer(&unit, &expression).unwrap();
            assert_eq!(value.value, expected, "{target}: {expression}");
            assert_eq!((value.bits, value.signed, value.rank), (32, true, 3));
        }
        for (source, accepted) in valid()
            .into_iter()
            .map(|source| (source, true))
            .chain(invalid().into_iter().map(|source| (source, false)))
        {
            let plain = analyze(&source, target);
            let retained = analyze_with_options(
                &source,
                target,
                &AnalysisOptions {
                    retain_code: true,
                    ..AnalysisOptions::default()
                },
            );
            assert_eq!(plain.is_ok(), accepted, "{target}: {source}");
            assert_eq!(
                retained.is_ok(),
                accepted,
                "{target}: {source}: {retained:?}"
            );
            match (plain, retained) {
                (Ok(plain), Ok(retained)) => {
                    assert_eq!(format!("{plain:?}"), format!("{:?}", retained.unit()))
                }
                (Err(plain), Err(retained)) => {
                    assert_eq!(
                        (plain.offset, plain.message),
                        (retained.offset, retained.message)
                    );
                }
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn undefined_and_nonconstant_bit_counts_are_not_folded() {
    for target in Target::ALL {
        let unit = analyze("int runtime(void);", target).unwrap();
        for name in NAMES {
            for value in ["0", "0.9"] {
                let expression = format!("{name}({value})");
                let error = evaluate_integer(&unit, &expression).unwrap_err();
                assert!(
                    error.message.contains("undefined result for zero"),
                    "{error}"
                );
                assert!(analyze(&format!("enum {{ n = {expression} }};"), target).is_err());
            }
            assert!(evaluate_integer(&unit, &format!("{name}(runtime())")).is_err());
        }
        for name in ["__builtin_clz", "__builtin_ctz"] {
            assert!(evaluate_integer(&unit, &format!("{name}(0x100000000ULL)")).is_err());
        }
    }
}

#[test]
#[ignore = "requires native GNU GCC and Clang cross targets; run with --include-ignored"]
fn bit_count_values_signatures_and_constraints_match_compilers() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("bit-counts.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let host = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    for (compiler, targets) in [
        (gcc.as_str(), vec![None]),
        ("clang", Target::ALL.iter().copied().map(Some).collect()),
    ] {
        let version = std::process::Command::new(compiler)
            .arg("--version")
            .output()
            .unwrap();
        assert!(version.status.success());
        if compiler != "clang" {
            assert!(
                !String::from_utf8_lossy(&version.stdout)
                    .to_lowercase()
                    .contains("clang"),
                "GNU GCC is required"
            );
        }
        for target in targets {
            let mut values = constants(target.unwrap_or(host))
                .into_iter()
                .map(|(expression, value)| {
                    format!("_Static_assert({expression} == {value}, \"value\");\n")
                })
                .collect::<String>();
            for name in NAMES {
                values.push_str(&format!(
                    "_Static_assert(_Generic({name}(1), int:1, default:0), \"type\");\n"
                ));
            }
            for (source, accepted) in valid()
                .into_iter()
                .map(|s| (s, true))
                .chain(invalid().into_iter().map(|s| (s, false)))
                .chain([(values, true)])
            {
                std::fs::write(&input, format!("{source}\n")).unwrap();
                let mut command = std::process::Command::new(compiler);
                command.args([
                    "-std=c11",
                    "-pedantic-errors",
                    "-Werror=int-conversion",
                    "-Wno-unused-value",
                    "-Wno-overflow",
                    "-fsyntax-only",
                ]);
                if let Some(target) = target {
                    command.arg(format!("--target={target}"));
                }
                let output = command.arg(&input).output().unwrap();
                assert_eq!(
                    output.status.success(),
                    accepted,
                    "{compiler} {target:?}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

use toucan_semantic::{IntegerValue, TranslationUnit, analyze, evaluate_integer};
use toucan_target::Target;

const GNU: Target = Target::X86_64UnknownLinuxGnu;
const APPLE: Target = Target::X86_64AppleDarwin;

struct Case {
    source: &'static str,
    names: &'static [&'static str],
    values: &'static [i128],
    gnu_types: &'static [&'static str],
    clang_types: &'static [&'static str],
    compatible_type: &'static str,
}

const CASES: &[Case] = &[
    Case {
        source: "enum E { A = 0U, B = 3ULL, C = -1 };",
        names: &["A", "B", "C"],
        values: &[0, 3, -1],
        gnu_types: &["int", "int", "int"],
        clang_types: &["int", "int", "int"],
        compatible_type: "int",
    },
    Case {
        source: "enum E { A = 0, B = 0xffffffffU, C = 3 };",
        names: &["A", "B", "C"],
        values: &[0, 4_294_967_295, 3],
        gnu_types: &["unsigned int", "unsigned int", "unsigned int"],
        clang_types: &["int", "unsigned int", "int"],
        compatible_type: "unsigned int",
    },
    Case {
        source: "enum E { A = 0, B = 1ULL << 40, C = 3 };",
        names: &["A", "B", "C"],
        values: &[0, 1 << 40, 3],
        gnu_types: &["unsigned long", "unsigned long", "unsigned long"],
        clang_types: &["int", "unsigned long", "int"],
        compatible_type: "unsigned long",
    },
    Case {
        source: "enum E { A = -1, B = 1ULL << 40, C = 3 };",
        names: &["A", "B", "C"],
        values: &[-1, 1 << 40, 3],
        gnu_types: &["long", "long", "long"],
        clang_types: &["int", "long", "int"],
        compatible_type: "long",
    },
    Case {
        source: "enum E { A = -1, B = 0xffffffffU, C = 3 };",
        names: &["A", "B", "C"],
        values: &[-1, 4_294_967_295, 3],
        gnu_types: &["long", "long", "long"],
        clang_types: &["int", "long", "int"],
        compatible_type: "long",
    },
    Case {
        source: "enum E { A = -1, B = (-9223372036854775807LL - 1), C = 3 };",
        names: &["A", "B", "C"],
        values: &[-1, i64::MIN as i128, 3],
        gnu_types: &["long", "long", "long"],
        clang_types: &["int", "long", "int"],
        compatible_type: "long",
    },
    Case {
        source: "enum E { A = 1LL << 40, B = sizeof(A), C = 3 };",
        names: &["A", "B", "C"],
        values: &[1 << 40, 8, 3],
        gnu_types: &["unsigned long", "unsigned long", "unsigned long"],
        clang_types: &["unsigned long", "int", "int"],
        compatible_type: "unsigned long",
    },
    Case {
        source: "enum E { A = -1, B = sizeof(A), C = 1ULL << 40, D = sizeof(A), F = sizeof(C), G = (C + A) > 0 };",
        names: &["A", "B", "C", "D", "F", "G"],
        values: &[-1, 4, 1 << 40, 4, 8, 1],
        gnu_types: &["long", "long", "long", "long", "long", "long"],
        clang_types: &["int", "int", "long", "int", "int", "int"],
        compatible_type: "long",
    },
    Case {
        source: "enum E { A = 2147483647, B, C = sizeof(B) };",
        names: &["A", "B", "C"],
        values: &[2_147_483_647, 2_147_483_648, 8],
        gnu_types: &["unsigned int", "unsigned int", "unsigned int"],
        clang_types: &["int", "unsigned int", "int"],
        compatible_type: "unsigned int",
    },
    Case {
        source: "enum E { A = 0xffffffffU, B, C = sizeof(B) };",
        names: &["A", "B", "C"],
        values: &[4_294_967_295, 4_294_967_296, 8],
        gnu_types: &["unsigned long", "unsigned long", "unsigned long"],
        clang_types: &["unsigned long", "unsigned long", "int"],
        compatible_type: "unsigned long",
    },
];

fn c_type(value: IntegerValue) -> &'static str {
    match (value.bits, value.signed, value.rank) {
        (32, true, 3) => "int",
        (32, false, 3) => "unsigned int",
        (64, true, 4) => "long",
        (64, false, 4) => "unsigned long",
        (64, true, 5) => "long long",
        (64, false, 5) => "unsigned long long",
        (128, true, 6) => "__int128",
        (128, false, 6) => "unsigned __int128",
        _ => panic!("unexpected integer representation: {value:?}"),
    }
}

#[test]
fn enumerator_types_change_only_after_the_definition() {
    for target in [
        GNU,
        Target::Aarch64UnknownLinuxGnu,
        APPLE,
        Target::Aarch64AppleDarwin,
    ] {
        let gnu = matches!(target, GNU | Target::Aarch64UnknownLinuxGnu);
        for case in CASES {
            let unit = analyze(case.source, target).unwrap();
            let expected = if gnu {
                case.gnu_types
            } else {
                case.clang_types
            };
            for ((name, expected_type), expected_value) in
                case.names.iter().zip(expected).zip(case.values)
            {
                let value = unit.constants[*name];
                assert_eq!(
                    c_type(value),
                    *expected_type,
                    "{target}: {}: {name}",
                    case.source
                );
                assert_eq!(
                    value.signed_value(),
                    *expected_value,
                    "{target}: {}: {name}",
                    case.source
                );
            }
            assert_eq!(
                c_type(evaluate_integer(&unit, "(enum E)0").unwrap()),
                case.compatible_type
            );
        }
    }
}

#[test]
fn final_enum_types_are_visible_in_later_definitions_and_macro_evaluation() {
    for target in [GNU, APPLE] {
        let source = "enum E { A = 1LL << 40 }; enum F { B = A < -1, C = sizeof(A) };";
        let unit = analyze(source, target).unwrap();
        // The positive-only enum changes A from signed long long to unsigned
        // long, so the later comparison converts -1 to unsigned long.
        assert_eq!(unit.constants["B"].value, 1);
        assert_eq!(unit.constants["C"].value, 8);
        assert_eq!(evaluate_integer(&unit, "A < -1").unwrap().value, 1);
        let source = "enum E { A = -1, B = 1ULL << 40 };";
        let unit = analyze(source, target).unwrap();
        // Conversely, the negative member changes B from unsigned long long
        // to signed long when the definition ends.
        assert_eq!(evaluate_integer(&unit, "B < -1").unwrap().value, 0);
    }
}

const GNU_WIDE: &[(&str, &str)] = &[
    ("enum E { A = -1, B = ~0ULL, C = 3 };", "__int128"),
    (
        "enum E { A = ~0ULL, B, C = sizeof(B) };",
        "unsigned __int128",
    ),
    (
        "enum E { A = 9223372036854775807L, B, C = sizeof(B) };",
        "unsigned long",
    ),
    (
        "typedef int Wide __attribute__((mode(TI))); enum E { A = ((Wide)1) << 100, B = 3 };",
        "unsigned __int128",
    ),
];

#[test]
fn gnu_widens_extended_enums_and_apple_rejects_lossy_recovery() {
    for (source, compatible_type) in GNU_WIDE {
        let unit = analyze(source, GNU).unwrap();
        for value in unit.constants.values() {
            assert_eq!(c_type(*value), *compatible_type, "{source}");
        }
        assert!(analyze(source, APPLE).is_err(), "{source}");
    }
    for target in [GNU, APPLE] {
        assert!(
            analyze(
                "typedef unsigned int Wide __attribute__((mode(TI))); enum E { A = ~(Wide)0 };",
                target
            )
            .is_err()
        );
    }
    assert!(analyze("enum E { A = 0xffffffffU };", Target::X86_64PcWindowsMsvc).is_err());
}

const COMPLETENESS: &[(&str, bool)] = &[
    ("enum E { A = sizeof(enum E) };", false),
    ("enum E { A = 1, B = sizeof(enum E) };", false),
    ("enum E { A = 1, B = _Alignof(enum E) };", false),
    ("enum E { A = 1, B = (enum E)1 };", false),
    ("enum E; enum F { A = sizeof(enum E) };", false),
    ("enum E { A = sizeof(enum E *) };", true),
    ("enum E { A = 1 }; enum F { B = sizeof(enum E) };", true),
    ("enum E; enum E *pointer;", true),
];

#[test]
fn enum_type_is_incomplete_until_the_closing_brace() {
    for target in [GNU, APPLE] {
        for (source, accepted) in COMPLETENESS {
            let result = analyze(source, target);
            assert_eq!(result.is_ok(), *accepted, "{target}: {source}: {result:?}");
        }
    }
}

/// Builds independent C assertions for widths, signedness, ranks, values, and the
/// compatible enum type represented by the analyzed translation unit.
fn compiler_probe(source: &str, unit: &TranslationUnit) -> String {
    use std::fmt::Write;
    let mut probe = source.to_owned();
    for (name, value) in &unit.constants {
        let c_type = c_type(*value);
        writeln!(
            probe,
            "\n_Static_assert(_Generic(({name}), {c_type}: 1, default: 0), \"type of {name}\");"
        )
        .unwrap();
        let bits = if value.signed {
            value.signed_value() as u128
        } else {
            value.value
        };
        writeln!(probe, "_Static_assert((unsigned __int128){name} == (((unsigned __int128){}ULL << 64) | {}ULL), \"value of {name}\");", bits >> 64, bits as u64).unwrap();
    }
    let compatible_type = c_type(evaluate_integer(unit, "(enum E)0").unwrap());
    writeln!(probe, "_Static_assert(_Generic((enum E)0, {compatible_type}: 1, default: 0), \"compatible enum type\");").unwrap();
    probe
}

#[test]
#[ignore = "requires GCC 13+ and Clang; run with --include-ignored"]
fn enum_values_and_types_match_c_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    for (compiler, target) in [
        ("gcc", GNU),
        ("clang", APPLE),
        ("clang", Target::Aarch64AppleDarwin),
    ] {
        let extra = GNU_WIDE
            .iter()
            .filter(|_| target == GNU)
            .map(|(source, _)| *source);
        for source in CASES.iter().map(|case| case.source).chain(extra) {
            let unit = analyze(source, target).unwrap();
            let probe = compiler_probe(source, &unit);
            let mut command = Command::new(compiler);
            command.args(["-std=c11", "-fsyntax-only", "-x", "c", "-"]);
            if compiler == "clang" {
                command.args(["-target", target.triple()]);
            }
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
                .write_all(probe.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{compiler} {target}: {probe}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn enum_completeness_matches_c_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    for compiler in ["gcc", "clang"] {
        for (source, accepted) in COMPLETENESS {
            let mut child = Command::new(compiler)
                .args(["-std=c11", "-fsyntax-only", "-x", "c", "-"])
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
            assert_eq!(
                output.status.success(),
                *accepted,
                "{compiler}: {source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

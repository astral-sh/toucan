use toucan_semantic::{analyze, evaluate_integer};
use toucan_target::Target;

const TARGETS: &[Target] = &[
    Target::X86_64UnknownLinuxGnu,
    Target::Aarch64UnknownLinuxGnu,
    Target::X86_64AppleDarwin,
    Target::Aarch64AppleDarwin,
    Target::X86_64PcWindowsMsvc,
];

const CASTS: &[(&str, u128)] = &[
    ("(int)0.0", 0),
    ("(int)3.99", 3),
    ("(int)0x1.fp2", 7),
    ("(int)16777217.0f", 16_777_216),
    ("(int)16777219.0f", 16_777_220),
    (
        "(unsigned long long)9007199254740993.0",
        9_007_199_254_740_992,
    ),
    ("(_Bool)0.0", 0),
    ("(_Bool)0.1", 1),
    ("(int)1.0e-1000", 0),
    ("(unsigned char)255.9", 255),
];

#[test]
fn c11_immediate_float_casts_round_in_the_target_format() {
    for target in TARGETS {
        let unit = analyze("", *target).unwrap();
        for (expression, expected) in CASTS {
            assert_eq!(
                evaluate_integer(&unit, expression)
                    .unwrap_or_else(|error| panic!("{target:?}: {expression}: {error}"))
                    .value,
                *expected,
                "{target:?}: {expression}"
            );
        }
        for expression in [
            "(int)2147483648.0",
            "(unsigned int)4294967296.0",
            "(unsigned char)256.0",
            "(int)1.0e100",
        ] {
            assert!(
                evaluate_integer(&unit, expression).is_err(),
                "{target:?}: {expression}"
            );
        }
        // These are arithmetic constant expressions, but are not C11 integer ICEs.
        for expression in [
            "1.0 < 2.0",
            "(int)(1.0 + 2.0)",
            "(int)-1.0",
            "(int)(float)1",
        ] {
            assert!(
                evaluate_integer(&unit, expression).is_err(),
                "{target:?}: {expression}"
            );
        }
    }
}

#[test]
fn static_initializers_accept_arithmetic_constants_and_check_conversion_ranges() {
    for target in TARGETS {
        for source in [
            "int x = 1.0 < 2.0;",
            "int x = (int)(1.0 + 2.0);",
            "int x = (int)-1.0;",
            "double x = 1.0 / 3.0;",
            "float x = 0.1;",
            "int x = 1.0 ? 2 : 3;",
            "int x = 0.0 && 1 / 0;",
            "int x = 1.0 || 1 / 0;",
            "unsigned x = -0.5;",
            "_Bool x = -0.0;",
            "_Bool x = 1.0e100;",
            "int x = (int){1};",
        ] {
            analyze(source, *target)
                .unwrap_or_else(|error| panic!("{target:?}: {source}: {error}"));
        }
        for source in [
            "int x = 2147483648.0;",
            "unsigned x = -1.0;",
            "unsigned char x = 256.0;",
            "float x = 1.0e100;",
            "double x = 1.0 / 0.0;",
            "double x = 0.0 / 0.0;",
        ] {
            assert!(analyze(source, *target).is_err(), "{target:?}: {source}");
        }
    }
}

#[test]
fn decimal_conversion_work_is_bounded() {
    let source = format!("int x = (int)0.{}1;", "0".repeat(5000));
    let error = analyze(&source, Target::X86_64UnknownLinuxGnu).unwrap_err();
    assert!(error.message.contains("4096-byte limit"));
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn immediate_casts_match_c11_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let valid = CASTS.iter().map(|(expression, expected)| {
        (
            format!("_Static_assert(({expression}) == {expected}ULL, \"value\");\n"),
            true,
        )
    });
    let invalid = [
        "1.0 < 2.0",
        "(int)(1.0 + 2.0) == 3",
        "(int)-1.0 == -1",
        "(int)(float)1 == 1",
        "(int)2147483648.0 != 0",
    ]
    .map(|expression| (format!("_Static_assert({expression}, \"ICE\");\n"), false));
    let sources: Vec<_> = valid.chain(invalid).collect();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc.as_str(), "clang"] {
        for (source, accepted) in &sources {
            let mut child = Command::new(compiler)
                .args([
                    "-std=c11",
                    "-pedantic-errors",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
                ])
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
            assert_eq!(
                analyze(source, Target::X86_64UnknownLinuxGnu).is_ok(),
                *accepted,
                "{source}"
            );
        }
    }
}

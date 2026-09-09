use std::io::Write;
use std::process::{Command, Stdio};
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

const VALID: &[&str] = &[
    "int f(void) { return __extension__ (int)1; }",
    "typedef unsigned char Byte; enum { value = __extension__ (Byte)257 }; _Static_assert(value == 1, \"narrowing\");",
    "int f(int x) { __extension__ x = 2; return x; }",
    "int f(void) { return __extension__ __extension__ (int)1 + 2; }",
    "int f(void) { return sizeof __extension__ (int)1; }",
    "int f(void) { return __extension__ ({ int x = 4; x; }); }",
    "_Static_assert(_Generic(__extension__ (unsigned char)257, unsigned char: 1, default: 0), \"cast type\");",
    "int f(void) { return __extension__ /* comment */ (int)1; }",
];
const INVALID: &[&str] = &[
    "int f(void) { return __extension__ (int); }",
    "int f(void) { __extension__ (int)1 = 2; return 0; }",
    "int f(void) { return __extension__ (void)1; }",
    "int f(void) { return __extension__ int; }",
    "int f(void) { return __extension__ (int)missing; }",
];

#[test]
fn extension_casts_preserve_types_values_and_checked_analysis() {
    let options = AnalysisOptions {
        retain_code: true,
        ..AnalysisOptions::default()
    };
    for target in Target::ALL {
        for source in VALID {
            let ordinary =
                analyze(source, target).unwrap_or_else(|error| panic!("{source}: {error}"));
            let retained = analyze_with_options(source, target, &options).unwrap();
            assert_eq!(format!("{ordinary:?}"), format!("{:?}", retained.unit()));
            assert!(retained.checked().is_some());
        }
        for source in INVALID {
            let ordinary = analyze(source, target).unwrap_err();
            let retained = analyze_with_options(source, target, &options).unwrap_err();
            assert_eq!(
                (ordinary.offset, ordinary.message),
                (retained.offset, retained.message)
            );
        }
    }
}

#[test]
fn keyword_prefix_runs_are_bounded_before_parser_recursion() {
    for prefix in [
        "__extension__ ",
        "sizeof ",
        "__extension__ /* gap */ ",
        "sizeof // gap\n",
        "__extension__ + ",
    ] {
        for count in [17, 4096] {
            let source = format!("int f(void) {{ return {}0; }}", prefix.repeat(count));
            let error = analyze(&source, Target::X86_64UnknownLinuxGnu).unwrap_err();
            assert!(
                error.message.contains("prefix operators"),
                "{prefix}: {error}"
            );
        }
    }
    let source = format!(
        "int f(void) {{ {} return 0; }}",
        "int __extension_name; ".repeat(32)
    );
    // Ordinary identifiers are not counted as prefix operators.
    assert!(
        !analyze(&source, Target::X86_64UnknownLinuxGnu)
            .unwrap_err()
            .message
            .contains("prefix operators")
    );
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn extension_cast_acceptance_matches_c_compilers() {
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc.as_str(), "clang"] {
        for (sources, accepted) in [(VALID, true), (INVALID, false)] {
            for source in sources {
                let mut child = Command::new(compiler)
                    .args(["-std=gnu11", "-Werror", "-fsyntax-only", "-x", "c", "-"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                writeln!(child.stdin.take().unwrap(), "{source}").unwrap();
                let result = child.wait_with_output().unwrap();
                assert_eq!(
                    result.status.success(),
                    accepted,
                    "{compiler}: {source}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
        }
    }
}

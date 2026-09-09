use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::{AnalysisOptions, analyze, analyze_with_profile, evaluate_integer};
use toucan_target::{CompilerProfile, Target};

const TARGET: Target = Target::X86_64UnknownLinuxGnu;
const ENVIRONMENT: &str = r#"
int i, j; const int ci = 0; float f; double d;
int *p, *q; const int *cp; void *vp;
int **pp; const int **cpp;
int arr[4]; const int carr[4];
struct S { int value; unsigned int bits : 3; } s, other;
const struct S cs;
struct T { int value; } t;
struct C { const int value; } c;
int takes_int(int); int takes_intp(int *); int takes_constp(const int *);
int takes_voidp(void *); int takes_s(struct S); int variadic(int, ...);
int (*fp)(int);
"#;

#[test]
#[ignore = "requires a C compiler (CC or cc); run with --include-ignored"]
fn expression_constraints_match_c11_compiler() {
    let valid = [
        "f + d",
        "-f",
        "f / i",
        "p + i",
        "i + p",
        "p - q",
        "p += i",
        "p -= i",
        "p == 0",
        "p == (void *)0",
        "p == cp",
        "p == vp",
        "p < q",
        "p && f",
        "i ? p : 0",
        "i ? p : cp",
        "i ? vp : cp",
        "i ? f : d",
        "i ? s : other",
        "takes_intp(arr)",
        "takes_constp(carr)",
        "takes_constp(p)",
        "takes_voidp(p)",
        "takes_s(s)",
        "variadic(1, p, f)",
        "fp(i)",
        "(*fp)(i)",
        "(&takes_int)(i)",
        "takes_intp(0)",
        "takes_intp(2-2)",
        "takes_intp((void *)0)",
        "_Generic(i, int: i, default: j) = 2",
        "&_Generic(i, int: i, default: j)",
        "_Generic((void)0, default: 1)",
        "_Generic(i, int: 1, default: 1/0)",
        "p = arr",
        "cp = p",
        "vp = p",
        "p = vp",
        "i = f",
        "s = other",
        "++i",
        "p++",
        "s.bits++",
        "i += d",
        "s.bits += i",
        "arr[2]",
        "2[arr]",
        "p[i]",
        "&s.value",
        "&*vp",
        "&*fp",
        "&arr",
        "cs.bits + 1",
        "s.bits = 2",
        "(void)p, i",
        "(int)p",
        "(int *)i",
    ];
    let invalid = [
        "&1",
        "&(i+j)",
        "&(i=1)",
        "&s.bits",
        "sizeof(s.bits)",
        "&cs.bits",
        "_Generic(i, int: 1, int: 2)",
        "_Generic(i, float: 1)",
        "_Generic(i, int: 1, default: missing)",
        "_Generic(i, void: 1, default: 2)",
        "_Generic(i, default: 1, default: 2)",
        "_Generic(i, int: i+1, default: j) = 2",
        "ci = 1",
        "cs.value = 1",
        "arr = p",
        "c = c",
        "(i,j) = 1",
        "s + i",
        "p + q",
        "p - f",
        "p -= q",
        "vp + 1",
        "fp + 1",
        "i - p",
        "i += p",
        "i += arr",
        "s.bits += p",
        "p == 1",
        "p == f",
        "p < 0",
        "fp < fp",
        "p == fp",
        "p == &t",
        "f % i",
        "f << 1",
        "~f",
        "s && i",
        "-p",
        "i ? p : 1",
        "i ? p : f",
        "i ? s : t",
        "takes_intp(i ? (const void *)0 : p)",
        "takes_intp(cp)",
        "takes_intp(carr)",
        "takes_intp(1)",
        "takes_intp(f)",
        "takes_s(t)",
        "takes_int(s)",
        "variadic(1, (void)0)",
        "cpp = pp",
        "p = cp",
        "p = fp",
        "s = t",
        "++cs.bits",
        "p[f]",
        "s[1]",
        "s.missing",
        "cs.bits = 1",
        "(struct S)i",
        "(int)s",
        "(float)p",
        "(int *)f",
    ];
    for (expected, expressions) in [(true, valid.as_slice()), (false, invalid.as_slice())] {
        for expression in expressions {
            let source = format!(
                "{ENVIRONMENT}\n_Static_assert(sizeof({expression}) > 0, \"expression\");\n"
            );
            let actual = analyze(&source, TARGET);
            let compiler = compile(&source);
            assert_eq!(
                compiler, expected,
                "unexpected C reference result for {expression}"
            );
            assert_eq!(actual.is_ok(), compiler, "{expression}: {actual:?}");
        }
    }
    // The profile supports these GNU extensions. Keep the pedantic C11
    // rejections separate from their compiler-extension semantics.
    for source in [
        "_Static_assert(_Alignof(void) == 1, \"GNU void alignment\");",
        "_Static_assert(sizeof(void) == 1, \"GNU void size\");",
        "_Static_assert(sizeof(void(void)) == 1, \"GNU function size\");",
    ] {
        assert!(!compile(source));
        analyze(source, TARGET).unwrap();
    }
}

#[test]
fn compound_pointer_addition_requires_a_pointer_destination() {
    for profile in CompilerProfile::ALL {
        for retain_code in [false, true] {
            let options = AnalysisOptions {
                retain_code,
                ..Default::default()
            };
            for expression in ["i += p", "i += arr", "s.bits += p"] {
                let source = format!("{ENVIRONMENT}\nvoid test(void) {{ {expression}; }}");
                let error = analyze_with_profile(&source, profile, &options).unwrap_err();
                assert!(
                    error.to_string().contains("arithmetic"),
                    "{profile:?}, retained={retain_code}: {expression}: {error}"
                );
            }
            let source = format!(
                "{ENVIRONMENT}\nvoid test(void) {{ p += i; p -= i; i + p; p + i; p - q; i += d; }}"
            );
            analyze_with_profile(&source, profile, &options)
                .unwrap_or_else(|error| panic!("{profile:?}, retained={retain_code}: {error}"));
        }
    }
}

#[test]
fn sizeof_uses_arithmetic_and_pointer_expression_types() {
    let unit = analyze(ENVIRONMENT, TARGET).unwrap();
    for (expression, expected) in [
        ("sizeof(f + f)", 4),
        ("sizeof(f + d)", 8),
        ("sizeof(-f)", 4),
        ("sizeof(i ? f : d)", 8),
        ("sizeof(p + i)", 8),
        ("sizeof(p - q)", 8),
        ("_Generic(s.bits + i, int: 1, default: 0)", 1),
        ("_Generic(+s.bits, int: 1, default: 0)", 1),
        ("_Generic(cp, const int*: 1, default: 0)", 1),
        ("_Generic(i ? p : cp, const int*: 1, default: 0)", 1),
        (
            "_Generic(i ? (const void*)0 : p, const void*: 1, default: 0)",
            1,
        ),
        ("_Generic(carr, const int*: 1, default: 0)", 1),
        ("_Generic(i, int: 7, default: 1 / 0)", 7),
        ("sizeof(s.bits + i)", 4),
        ("sizeof(i = f)", 4),
        ("sizeof(arr)", 16),
    ] {
        assert_eq!(
            evaluate_integer(&unit, expression).unwrap().value,
            expected,
            "{expression}"
        );
    }
}

#[test]
fn nested_generic_selection_work_remains_bounded() {
    let unit = analyze("", TARGET).unwrap();
    let mut expression = String::from("1");
    for _ in 0..24 {
        expression = format!("_Generic(0, int: {expression}, default: 0)");
    }
    assert_eq!(evaluate_integer(&unit, &expression).unwrap().value, 1);
    assert_eq!(
        evaluate_integer(&unit, &format!("sizeof({expression})"))
            .unwrap()
            .value,
        4
    );
}

fn compile(source: &str) -> bool {
    let mut child = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .args([
            "-x",
            "c",
            "-std=c11",
            "-pedantic-errors",
            "-fsyntax-only",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the differential test requires a C compiler (set CC)");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    toucan_test_support::compiler_acceptance(&output)
        .unwrap_or_else(|failure| panic!("C compiler failed for source:\n{source}\n{failure}"))
}

use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::analyze;
use toucan_target::Target;

const PREFIX: &str = "struct S { int x; }; struct T { int x; }; union U { int x; double y; };";
const CASES: &[(&str, bool)] = &[
    ("int f(struct S s) { return ((struct S)s).x; }", true),
    (
        "struct S f(const struct S *s) { return (struct S)*s; }",
        true,
    ),
    ("union U f(union U u) { return (union U)u; }", true),
    ("struct S s = (struct S)(struct S){1};", true),
    (
        "int f(struct S s) { return _Generic((const struct S)s, struct S: 1, default: 0); }",
        true,
    ),
    ("void f(struct S s) { ((struct S)s).x = 1; }", false),
    ("void f(struct S s) { struct S *p = &(struct S)s; }", false),
    ("struct S f(struct T t) { return (struct S)t; }", false),
    ("struct S f(int i) { return (struct S)i; }", false),
    ("struct S f(struct S *p) { return (struct S)p; }", false),
    ("struct S a; struct S b = (struct S)a;", false),
];

#[test]
fn same_record_casts_produce_values() {
    for target in Target::ALL {
        for (body, accepted) in CASES {
            let source = format!("{PREFIX}{body}\n");
            let result = analyze(&source, target);
            assert_eq!(result.is_ok(), *accepted, "{target}: {source}: {result:?}");
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn record_casts_match_gnu_compilers() {
    for compiler in ["gcc", "clang"] {
        for (body, accepted) in CASES {
            let mut child = Command::new(compiler)
                .args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"])
                .stdin(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            writeln!(child.stdin.take().unwrap(), "{PREFIX}{body}").unwrap();
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(*accepted),
                "{compiler}: {body}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

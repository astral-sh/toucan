use toucan_semantic::{TypeKind, analyze};
use toucan_target::Target;

const TARGET: Target = Target::X86_64UnknownLinuxGnu;

const CASES: &[(&str, bool)] = &[
    ("int f(int); enum { SIZE = sizeof(f()) };", false),
    ("int f(void); enum { SIZE = sizeof(f(1)) };", false),
    ("int f(int, ...); enum { SIZE = sizeof(f()) };", false),
    ("int f(int); enum { SIZE = sizeof(f(unknown)) };", false),
    ("int a[1]; enum { SIZE = sizeof(a[\"hello\"]) };", false),
    ("int a[1]; enum { SIZE = sizeof(a[1.0]) };", false),
    ("int a[1]; enum { SIZE = sizeof(a[unknown]) };", false),
    (
        "int f(int); _Static_assert(sizeof(f(1 / 0)) == sizeof(int), \"unevaluated argument\");",
        true,
    ),
    (
        "int f(int, ...); _Static_assert(sizeof(f(1, 2)) == sizeof(int), \"variadic arguments\");",
        true,
    ),
    (
        "int f(); _Static_assert(sizeof(f(1, 2)) == sizeof(int), \"no prototype\");",
        true,
    ),
    (
        "int a[1]; _Static_assert(sizeof(a[1 / 0]) == sizeof(int), \"unevaluated index\");",
        true,
    ),
    (
        "int a[1]; _Static_assert(sizeof(0[a]) == sizeof(int), \"commuted subscript\");",
        true,
    ),
    (
        "extern int (*p)[]; extern int (*p)[4]; _Static_assert(sizeof(*p) == 4 * sizeof(int), \"pointer composite\");",
        true,
    ),
    (
        "extern int (*p)[4]; extern int (*p)[]; _Static_assert(sizeof(*p) == 4 * sizeof(int), \"reverse pointer composite\");",
        true,
    ),
    (
        "int (*f(void))[]; int (*f(void))[4]; _Static_assert(sizeof(*f()) == 4 * sizeof(int), \"function result composite\");",
        true,
    ),
    (
        "extern int (*(*factory)(void))[]; extern int (*(*factory)(void))[4]; _Static_assert(sizeof(*factory()) == 4 * sizeof(int), \"nested function composite\");",
        true,
    ),
    (
        "typedef int Array[]; extern Array *p; extern int (*p)[4]; _Static_assert(sizeof(*p) == 4 * sizeof(int), \"typedef composite\");",
        true,
    ),
];

#[test]
fn expression_constraints_and_nested_composite_types() {
    for (source, accepted) in CASES {
        let result = analyze(source, TARGET);
        assert_eq!(result.is_ok(), *accepted, "{source}: {result:?}");
    }
    let unit = analyze("int f(int (*)[]); int f(int (*)[4]);", TARGET).unwrap();
    let TypeKind::Function(function) = &unit.declarations[0].ty.kind else {
        panic!("expected function");
    };
    let TypeKind::Pointer(element) = &function.parameters[0].ty.kind else {
        panic!("expected pointer parameter");
    };
    assert_eq!(unit.layout(element).unwrap().size_bytes(), 16);
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn compiler_constraints_match_gcc_and_clang() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    for compiler in ["gcc", "clang"] {
        for (source, accepted) in CASES {
            let mut child = Command::new(compiler)
                .args(["-std=c11", "-fsyntax-only", "-x", "c", "-"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap_or_else(|error| panic!("starting {compiler}: {error}"));
            child
                .stdin
                .take()
                .unwrap()
                .write_all(source.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(*accepted),
                "{compiler}: {source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                analyze(source, TARGET).is_ok(),
                output.status.success(),
                "{compiler}: {source}"
            );
        }
    }
}

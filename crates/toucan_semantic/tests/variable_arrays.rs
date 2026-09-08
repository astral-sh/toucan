use toucan_semantic::{TypeKind, analyze};
use toucan_target::Target;

// C11 6.7.6.2, 6.7.7, 6.8.4.2 and 6.8.6.1. Both valid and
// invalid cases are also passed to independent compilers below.
const CASES: &[(&str, bool)] = &[
    ("void f(int a[*]);", true),
    ("void f(int n, int a[n]);", true),
    ("void f(int n, int a[static restrict n]);", true),
    ("void f(int n, int a[const n][n]);", true),
    ("void f(int a[*][*]); void f(int a[3][4]);", true),
    ("void f(int n, int (*p)[n]);", true),
    ("void f(int n, int a[n]) { a[0] = n; }", true),
    ("void f(int n, int a[n][n]) { a[0][0] = n; ++a; }", true),
    ("void f(int n, int (*p)(int a[*])) {}", true),
    ("void (*f(int n))(int a[*]) { return 0; }", true),
    ("void f(int n) { int a[n]; a[0] = 1; int *p = a; }", true),
    (
        "void f(int n) { int a[n]; int (*p)[n] = &a; ++p; --p; p += 1; p -= 1; long d = p - &a; }",
        true,
    ),
    ("void f(int n) { int a[2][n]; int (*p)[n] = a; }", true),
    ("void f(int n) { typedef int A[n]; A a; a[0] = 1; }", true),
    (
        "void f(int n) { typedef int A[n]; int a[2][n]; A *p = a; }",
        true,
    ),
    (
        "void f(int n) { typedef int A[n]; { typedef int A[n]; A a; } }",
        true,
    ),
    ("void f(int n) { static int (*p)[n]; }", true),
    ("void f(int n) { static int (*p)[n] = 0; }", true),
    ("void f(int n) { int (*p)[n] = (int (*)[n]){0}; }", true),
    ("void f(int n) { int (*p)[n]; int (*q)[3] = p; }", true),
    (
        "void f(int n) { int a[n]; unsigned long x = sizeof(a); }",
        true,
    ),
    ("void f(int n) { unsigned long x = sizeof(int[n]); }", true),
    (
        "void f(int n) { typedef int A[n]; unsigned long x = sizeof(A[2]); }",
        true,
    ),
    ("void f(int n) { int a[sizeof(int[n])]; }", true),
    (
        "void f(int n) { int a[n]; enum { A = _Alignof(int[n]), P = sizeof(&a) }; }",
        true,
    ),
    (
        "void f(int n) { int a[1 || n]; unsigned long x = sizeof(a); }",
        true,
    ),
    ("int n; int x = _Alignof(int[n]);", true),
    ("int n; int x = sizeof(int (*)[n]);", true),
    // Undefined runtime bounds are not integer constant expressions. These
    // declarations are type-correct; executing their bounds is undefined.
    ("void f(void) { int a[1 / 0]; }", true),
    ("void f(int n) { for (int a[n]; n; --n) a[0] = n; }", true),
    ("void f(int n) { goto L; { int a[n]; } L:; }", true),
    ("void f(int n) { L:; int a[n]; if (n) goto L; }", true),
    ("void f(int n) { int a[n]; goto L; L:; }", true),
    ("void f(int n) { { int a[n]; goto L; } L:; }", true),
    (
        "void f(int n) { int a[n]; switch(n) { case 0:; default:; } }",
        true,
    ),
    (
        "void f(int n) { switch(n) { case 0:; int a[n]; a[0] = 1; } }",
        true,
    ),
    (
        "void f(int n) { switch(n) { case 0: { int a[n]; switch(n) { case 1:; } } } }",
        true,
    ),
    ("void f(int n) { int a[*]; }", false),
    ("int a[*];", false),
    ("void f(int n, int a[*]) {}", false),
    ("void f(int n, int a[n][*]) {}", false),
    ("int n; int a[n];", false),
    ("int n; extern int a[n];", false),
    ("int n; static int (*p)[n];", false),
    ("int n; typedef int A[n];", false),
    ("void f(int n) { static int a[n]; }", false),
    ("void f(int n) { extern int a[n]; }", false),
    ("void f(int n) { extern int (*p)[n]; }", false),
    ("void f(int n) { extern int (*g(void))[n]; }", false),
    (
        "void f(int n) { typedef int A[n]; typedef int A[n]; }",
        false,
    ),
    ("void f(int n) { struct S { int a[n]; }; }", false),
    ("void f(int n) { struct S { int (*p)[n]; }; }", false),
    ("void f(int n) { int a[n] = {0}; }", false),
    ("void f(int n) { int a[2][n] = {{0}}; }", false),
    ("void f(int n) { int *p = (int[n]){0}; }", false),
    (
        "void f(int n) { int x = _Generic(0, int (*)[n]: 1, default: 0); }",
        false,
    ),
    (
        "void f(int n) { int x = _Generic(0, int[n]: 1, default: 0); }",
        false,
    ),
    ("void f(float n) { int a[n]; }", false),
    ("void f(int *n) { int a[n]; }", false),
    ("void f(int n) { void a[n]; }", false),
    ("void f(int n) { struct S; struct S a[n]; }", false),
    ("void f(int n) { int a[n]; enum { X = sizeof(a) }; }", false),
    (
        "void f(int n) { int a[n]; _Static_assert(sizeof(a), \"\"); }",
        false,
    ),
    (
        "void f(int n) { int a[1 || n]; _Static_assert(sizeof(a) == sizeof(int), \"\"); }",
        false,
    ),
    (
        "void f(int n) { int a[n]; static unsigned long x = sizeof(a); }",
        false,
    ),
    ("void f(int n) { int a[n]; static int *p = a; }", false),
    (
        "void f(int n) { int a[n][6][n]; int (*p)[4][n+1] = a; }",
        false,
    ),
    ("void f(int n) { goto L; int a[n]; L:; }", false),
    ("void f(int n) { goto L; typedef int A[n]; L:; }", false),
    ("void f(int n) { goto L; int (*p)[n]; L:; }", false),
    ("void f(int n) { goto L; { int a[n]; L:; } }", false),
    (
        "void f(int n) { { int a[n]; goto L; } { int b[n]; L:; } }",
        false,
    ),
    ("void f(int n) { switch(n) { int a[n]; case 0:; } }", false),
    (
        "void f(int n) { switch(n) { int (*p)[n]; case 0:; } }",
        false,
    ),
    (
        "void f(int n) { switch(n) { typedef int A[n]; default:; } }",
        false,
    ),
    (
        "void f(int n) { switch(n) { case 0:; int a[n]; case 1:; } }",
        false,
    ),
];

#[test]
fn variable_array_constraints() {
    for target in [Target::X86_64UnknownLinuxGnu, Target::X86_64AppleDarwin] {
        for &(source, accepted) in CASES {
            let result = analyze(source, target);
            assert_eq!(result.is_ok(), accepted, "{target:?}: {source}: {result:?}");
        }
    }
}

#[test]
fn runtime_arrays_remain_distinct_from_incomplete_arrays() {
    let unit = analyze(
        "void f(int n, int (*a)[n], int (*b)[], int c[restrict n][n]);",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let TypeKind::Function(function) = &unit.declarations[0].ty.kind else {
        panic!("function")
    };
    let TypeKind::Pointer(array) = &function.parameters[1].ty.kind else {
        panic!("pointer")
    };
    assert!(matches!(array.kind, TypeKind::VariableArray { .. }));
    assert!(unit.is_variable_length_array(array).unwrap());
    assert!(
        unit.is_variably_modified(&function.parameters[1].ty)
            .unwrap()
    );
    assert!(
        !unit
            .is_variable_length_array(&function.parameters[1].ty)
            .unwrap()
    );
    assert!(!unit.is_variably_modified(&unit.declarations[0].ty).unwrap());
    assert_eq!(unit.alignment(array).unwrap(), 4);
    assert!(
        unit.layout(array)
            .unwrap_err()
            .message
            .contains("runtime bound")
    );
    assert_eq!(
        unit.layout(&function.parameters[1].ty)
            .unwrap()
            .size_bytes(),
        8
    );
    let TypeKind::Pointer(incomplete) = &function.parameters[2].ty.kind else {
        panic!("pointer")
    };
    assert!(matches!(
        incomplete.kind,
        TypeKind::Array { length: None, .. }
    ));
    assert!(!unit.is_variably_modified(incomplete).unwrap());
    assert!(function.parameters[3].ty.qualifiers.is_restrict);
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn variable_array_constraints_match_c_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        "clang".into(),
    ] {
        for &(source, accepted) in CASES {
            let mut child = Command::new(&compiler)
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
            writeln!(child.stdin.take().unwrap(), "{source}").unwrap();
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                output.status.success(),
                accepted,
                "{compiler}: {source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                analyze(source, Target::X86_64UnknownLinuxGnu).is_ok(),
                accepted,
                "{compiler}: {source}"
            );
        }
    }
}

#[test]
fn many_vm_declarations_and_labels_use_compact_scope_snapshots() {
    use std::fmt::Write;
    let mut source = String::from("void f(int n) {");
    for index in 0..5000 {
        write!(source, "int a{index}[n]; L{index}:; if (n) goto L{index};").unwrap();
    }
    source.push('}');
    analyze(&source, Target::X86_64UnknownLinuxGnu).unwrap();
}

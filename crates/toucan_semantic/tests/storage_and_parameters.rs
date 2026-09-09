use toucan_semantic::{DeclarationKind, TypeKind, analyze};
use toucan_target::Target;

const CASES: &[(&str, bool)] = &[
    ("static extern int x;", false),
    ("typedef static int T;", false),
    ("static static int x;", false),
    ("extern extern int x;", false),
    ("auto int x;", false),
    ("register int x;", false),
    ("register int f(void);", false),
    ("typedef _Thread_local int T;", false),
    ("_Thread_local _Thread_local int x;", false),
    ("_Thread_local void f(void);", false),
    ("extern int x; static int x;", false),
    ("static int x; int x;", false),
    ("int f(void); static int f(void);", false),
    ("int x = 1; int x = 2;", false),
    ("int x = 1; int x; int x = 2;", false),
    ("int f(void) { return 1; } int f(void) { return 2; }", false),
    (
        "int f(void); int f(void) { return 1; } int f(void) { return 2; }",
        false,
    ),
    ("int f(void) = 0;", false),
    ("typedef int T = 0;", false),
    ("enum { A }; int A;", false),
    ("enum { A }; typedef int A;", false),
    ("int A; enum { A };", false),
    ("int f(static int x);", false),
    ("int f(extern int x);", false),
    ("int f(auto int x);", false),
    ("int f(typedef int x);", false),
    ("int f(_Thread_local int x);", false),
    ("int f(register register int x);", false),
    ("int f(_Alignas(8) int x);", false),
    ("int f(const void);", false),
    ("int f(volatile void);", false),
    ("int a[static 2];", false),
    ("int a[const 2];", false),
    ("typedef int A[restrict 2];", false),
    ("struct S { int a[const 2]; };", false),
    ("int f(int (*a)[const 2]);", false),
    ("int f(int a[2][const 3]);", false),
    ("int f(int a[2][static 3]);", false),
    ("int f(int (*a[2])[const 3]);", false),
    ("struct S; struct S x;", false),
    ("struct S; static struct S x;", false),
    ("enum E; enum E x;", false),
    (
        "int x[]; _Static_assert(sizeof(x) == sizeof(int), \"premature completion\");",
        false,
    ),
    ("int f(void); int f(void);", true),
    ("static int f(void); int f(void);", true),
    ("static int f(void); extern int f(void);", true),
    ("static int x; extern int x;", true),
    ("static int x; static int x;", true),
    ("extern int x; int x; extern int x;", true),
    ("int x; int x;", true),
    ("extern int x; int x = 1;", true),
    ("int x = 1; extern int x; int x;", true),
    ("static int x; extern int x = 1;", true),
    (
        "int f(int old); int f(int current) { return current; }",
        true,
    ),
    ("int f(void) { return 1; } int f(void);", true),
    ("int f(register int x);", true),
    ("int f(register int);", true),
    ("int f(void);", true),
    ("int f(int a[const 2]);", true),
    ("int f(const int a[restrict static 2]);", true),
    ("int f(int a[static const 2]);", true),
    ("int f(int (a[static 2]));", true),
    ("int f(int (a)[const 2]);", true),
    ("int f(int a[const 2][3]);", true),
    ("int f(int (*a[const 2])[3]);", true),
    ("int f(int *a[restrict const 2]);", true),
    ("int f(int (*callback)(int a[const 2]));", true),
    ("int f(int a[const 2]); int f(int *a);", true),
    ("int f(int a[const 2]); int f(int a[volatile 3]);", true),
    ("extern struct S x;", true),
    ("struct S; struct S x; struct S { int member; };", true),
    ("int x[];", true),
    ("static int x[];", true),
    ("typedef int A[]; A x;", true),
    ("int x[]; extern int x[3];", true),
];

const TLS_CASES: &[&str] = &[
    "_Thread_local int x;",
    "static _Thread_local int x;",
    "_Thread_local extern int x;",
];

#[test]
fn storage_linkage_and_parameter_constraints() {
    for target in [Target::X86_64UnknownLinuxGnu, Target::X86_64AppleDarwin] {
        for (source, accepted) in CASES {
            let result = analyze(source, target);
            assert_eq!(
                result.is_ok(),
                *accepted,
                "{target:?}: {source}: {result:?}"
            );
        }
        // Clang permits this spelling; GCC diagnoses the storage on void.
        assert_eq!(
            analyze("int f(register void);", target).is_ok(),
            target == Target::X86_64AppleDarwin
        );
        for source in TLS_CASES {
            let error = analyze(source, target).unwrap_err();
            assert!(
                error.message.contains("unsupported Rust TLS"),
                "{source}: {error}"
            );
        }
    }
}

#[test]
fn redeclarations_retain_definition_and_linkage_metadata() {
    let unit = analyze(
        "extern int x; int x = 1; extern int x; static int f(int old); int f(int current) { return current; } extern int f(int later);",
        Target::X86_64UnknownLinuxGnu,
    ).unwrap();
    assert_eq!(unit.declarations.len(), 2);
    let variable = &unit.declarations[0];
    assert_eq!(variable.kind, DeclarationKind::Variable);
    assert!(variable.is_definition && !variable.is_static);
    let function = &unit.declarations[1];
    assert!(function.is_definition && function.is_static);
    let TypeKind::Function(function) = &function.ty.kind else {
        panic!("expected function")
    };
    assert_eq!(function.parameters[0].name.as_deref(), Some("current"));
}

#[test]
fn tentative_definitions_complete_at_the_end_of_the_translation_unit() {
    let unit = analyze(
        "typedef int A[]; A x; static int y[]; int z[]; extern int z[3]; extern int incomplete[];",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    assert!(matches!(
        unit.typedefs["A"].kind,
        TypeKind::Array { length: None, .. }
    ));
    for (name, length) in [
        ("x", Some(1)),
        ("y", Some(1)),
        ("z", Some(3)),
        ("incomplete", None),
    ] {
        let declaration = unit
            .declarations
            .iter()
            .find(|item| item.name == name)
            .unwrap();
        let TypeKind::Array { length: actual, .. } = unit.resolve(&declaration.ty).unwrap().kind
        else {
            panic!("expected array")
        };
        assert_eq!(actual, length, "{name}");
        assert_eq!(declaration.is_definition, name != "incomplete");
    }
}

#[test]
fn parameter_array_qualifiers_apply_to_the_adjusted_pointer() {
    let unit = analyze(
        "void f(const int a[restrict static 2], int b[const 2][3], int (*c[volatile 2])[3]);",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let TypeKind::Function(function) = &unit.declarations[0].ty.kind else {
        panic!("expected function")
    };
    let [a, b, c] = function.parameters.as_slice() else {
        panic!("expected three parameters")
    };
    assert!(a.ty.qualifiers.is_restrict && !a.ty.qualifiers.is_const);
    let TypeKind::Pointer(element) = &a.ty.kind else {
        panic!("expected adjusted pointer")
    };
    assert!(element.qualifiers.is_const && !element.qualifiers.is_restrict);
    assert!(b.ty.qualifiers.is_const && !b.ty.qualifiers.is_volatile);
    let TypeKind::Pointer(element) = &b.ty.kind else {
        panic!("expected adjusted pointer")
    };
    assert!(!element.qualifiers.is_const);
    assert!(matches!(
        element.kind,
        TypeKind::Array {
            length: Some(3),
            ..
        }
    ));
    assert!(c.ty.qualifiers.is_volatile);
    let TypeKind::Pointer(element) = &c.ty.kind else {
        panic!("expected adjusted pointer")
    };
    assert!(!element.qualifiers.is_volatile);
    assert!(matches!(element.kind, TypeKind::Pointer(_)));
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn storage_and_parameter_constraints_match_c_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    for (compiler, target) in [
        ("gcc", Target::X86_64UnknownLinuxGnu),
        ("clang", Target::X86_64AppleDarwin),
    ] {
        let cases = CASES
            .iter()
            .copied()
            .chain([("int f(register void);", compiler == "clang")])
            .map(|(source, accepted)| (source, accepted, false))
            .chain(TLS_CASES.iter().map(|source| (*source, true, true)));
        for (source, accepted, unsupported) in cases {
            let mut child = Command::new(compiler)
                // Clang diagnoses repeated storage classes as a warning by default.
                .args([
                    "-std=c11",
                    "-Werror=duplicate-decl-specifier",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
                ])
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
                output.status.success(),
                accepted,
                "{compiler}: {source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let result = analyze(source, target);
            if unsupported {
                assert!(result.unwrap_err().message.contains("unsupported Rust TLS"));
            } else {
                assert_eq!(
                    result.is_ok(),
                    output.status.success(),
                    "{compiler}: {source}"
                );
            }
        }
    }
}

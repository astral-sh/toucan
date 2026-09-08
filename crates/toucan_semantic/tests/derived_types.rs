use toucan_semantic::{Type, TypeKind, analyze, evaluate_integer};
use toucan_target::Target;

// C11 6.7.2.1, 6.7.3, and 6.7.6.2 constrain record members, restrict
// qualification, and array elements independently of whether a layout is used.
const CASES: &[(&str, bool)] = &[
    ("struct S; struct S a[1];", false),
    ("struct S; extern struct S a[];", false),
    ("struct S; typedef struct S A[2];", false),
    ("struct S; int f(struct S a[1]);", false),
    ("struct S { struct S a[1]; };", false),
    ("struct S { struct S value; };", false),
    ("enum E; enum E a[2];", false),
    ("typedef int A[]; A a[2];", false),
    ("int a[2][];", false),
    ("void a[2];", false),
    ("typedef int F(void); F a[2];", false),
    ("struct S { void value; };", false),
    ("struct S { int function(void); };", false),
    ("typedef void V; struct S { V value; };", false),
    ("enum E; struct S { enum E value; };", false),
    ("struct S { int : 2; int values[]; };", false),
    ("struct S { int values[]; int count; };", false),
    ("union U { int count; int values[]; };", false),
    ("restrict int value;", false),
    ("restrict int *pointer;", false),
    ("int (*restrict callback)(void);", false),
    ("typedef int F(void); restrict F *callback;", false),
    ("typedef int A[3]; restrict A values;", false),
    ("typedef int (*F)(void); restrict F callback;", false),
    ("struct S { struct { int x; }; int x; };", false),
    ("struct S { int x; union { int x; }; };", false),
    ("struct S { struct { int x; }; union { int x; }; };", false),
    ("struct S { struct I { int x; }; int values[]; };", false),
    ("struct S; extern struct S value;", true),
    ("struct S; struct S *pointers[2];", true),
    ("struct S; typedef struct S *P; P pointers[2];", true),
    ("struct S { struct S *next; }; struct S values[2];", true),
    ("typedef int A[]; extern A values;", true),
    ("typedef int A[2]; A values[3];", true),
    ("int values[][2];", true),
    ("int (*pointer)[2];", true),
    ("struct S { void *value; int (*callback)(void); };", true),
    ("struct S { int count; int values[]; };", true),
    ("struct S { int count : 2; int values[]; };", true),
    ("struct S { struct { int count; }; int values[]; };", true),
    ("struct S { union { int count; }; int values[]; };", true),
    ("int *restrict pointer;", true),
    ("void *restrict pointer;", true),
    ("struct S; struct S *restrict pointer;", true),
    ("typedef int *P; restrict P pointer;", true),
    ("int *restrict *pointer;", true),
    ("int (*restrict pointer)[2];", true),
    ("int f(int *restrict pointer);", true),
    ("struct S { struct { int x; } nested; int x; };", true),
    (
        "struct S { struct I { int x; }; int y; }; _Static_assert(sizeof(struct S) == sizeof(int), \"tag is not a member\");",
        true,
    ),
    (
        "typedef struct { int x; } T; struct S { T; int y; }; _Static_assert(sizeof(struct S) == sizeof(int), \"typedef is not a member\");",
        true,
    ),
    (
        "struct S { enum E { A = 7 }; int y; }; _Static_assert(sizeof(struct S) == sizeof(int) && A == 7, \"tag is not a member\");",
        true,
    ),
    (
        "struct S { int; int y; }; _Static_assert(sizeof(struct S) == sizeof(int), \"empty declaration\");",
        true,
    ),
];

// GCC accepts these extensions; Clang rejects them in the same C11 mode.
const GNU_CASES: &[&str] = &[
    "typedef int *A[3]; restrict A pointers;",
    "struct S { int : 2; struct { int : 2; }; int values[]; };",
];

#[test]
fn derived_types_obey_object_and_qualifier_constraints() {
    for target in [Target::X86_64UnknownLinuxGnu, Target::X86_64AppleDarwin] {
        for (source, accepted) in CASES {
            let result = analyze(source, target);
            assert_eq!(
                result.is_ok(),
                *accepted,
                "{target:?}: {source}: {result:?}"
            );
        }
        for source in GNU_CASES {
            let result = analyze(source, target);
            assert_eq!(
                result.is_ok(),
                target == Target::X86_64UnknownLinuxGnu,
                "{target:?}: {source}: {result:?}"
            );
        }
    }
}

#[test]
fn array_completeness_rejects_invalid_public_tag_identities() {
    for kind in [TypeKind::Record(usize::MAX), TypeKind::Enum(usize::MAX)] {
        let mut unit = analyze("", Target::X86_64UnknownLinuxGnu).unwrap();
        unit.typedefs.insert("T".into(), Type::new(kind));
        assert!(evaluate_integer(&unit, "sizeof(T[2])").is_err());
    }
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn derived_type_constraints_match_c_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    for (compiler, target) in [
        ("gcc", Target::X86_64UnknownLinuxGnu),
        ("clang", Target::X86_64AppleDarwin),
    ] {
        // macOS's `gcc` driver is Apple Clang. GNU-only behavior is checked
        // against GCC on Linux; native Clang still checks this matrix on macOS.
        if compiler == "gcc" && !cfg!(target_os = "linux") {
            continue;
        }
        let cases = CASES
            .iter()
            .copied()
            .chain(GNU_CASES.iter().map(|source| (*source, compiler == "gcc")));
        for (source, accepted) in cases {
            let mut command = Command::new(compiler);
            command.args(["-std=c11", "-fsyntax-only", "-x", "c", "-"]);
            if !accepted {
                // Newer Clang accepts some constraint violations as extensions.
                // A required diagnostic is the oracle for these invalid cases.
                command.arg("-pedantic-errors");
            }
            let mut child = command
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
                Ok(accepted),
                "{compiler}: {source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                analyze(source, target).is_ok(),
                output.status.success(),
                "{compiler}: {source}"
            );
        }
    }
}

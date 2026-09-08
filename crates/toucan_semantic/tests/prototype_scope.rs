use toucan_semantic::{Scope, TranslationUnit, Type, TypeKind, analyze, evaluate_integer};
use toucan_target::Target;

const TARGET: Target = Target::X86_64UnknownLinuxGnu;

const CASES: &[(&str, bool)] = &[
    ("int f(enum { A = 1 } x, int A);", false),
    ("int f(int A, enum { A = 1 } x);", false),
    ("enum { A = 1 }; int f(int A, enum { B = A } b);", false),
    (
        "enum { A = 1 }; int f(int A, int values[sizeof(A)]); _Static_assert(A == 1, \"restored\");",
        true,
    ),
    ("int (*f(enum { N = 4 } x))[N];", false),
    ("int (*f(enum { N = 4 } x))(int values[N]);", false),
    ("int (*f(struct S *))(struct S *);", true),
    (
        "int f(struct S *); struct S { int x; }; int f(struct S *);",
        false,
    ),
    (
        "struct S; int f(struct S *); struct S { int x; }; int f(struct S *);",
        true,
    ),
    ("int f(struct S *); int f(struct S *);", false),
    ("int f(struct S *, struct S *); int g(struct S *);", true),
    (
        "int f(enum E { A = 3, B = A + 1 } value, int values[B]); enum { A = 7 };",
        true,
    ),
    (
        "int f(enum E { A = 3 } value); _Static_assert(A == 3, \"escaped\");",
        false,
    ),
    (
        "enum E { A = 3 }; int f(enum E { A = 7 } value, int values[A]); _Static_assert(A == 3, \"restored\");",
        true,
    ),
    (
        "struct S { int x; }; int f(struct S { char x; } *); _Static_assert(sizeof(struct S) == sizeof(int), \"restored\");",
        true,
    ),
    (
        "enum S { A }; int f(struct S { int x; } *); enum S value;",
        true,
    ),
    ("int f(struct S *, struct S { int x; } *);", true),
    ("int f(int (*callback)(struct S *), struct S *);", true),
    (
        "int f(struct S *, int (*callback)(struct S *), struct S *);",
        true,
    ),
    (
        "struct S; int f(int (*callback)(struct S { char x; } *), struct S *);",
        true,
    ),
    (
        "int f(enum E { A = 1 } value); int g(enum E { A = 2 } value);",
        true,
    ),
    ("int f(enum E { A = 1 } x, enum F { A = 2 } y);", false),
    (
        "int f(struct S *value) { return 0; } struct S { int x; }; int f(struct S *);",
        false,
    ),
    (
        "int f(struct S *value) { return 0; } struct S { int x; };",
        true,
    ),
];

fn parameters<'a>(unit: &'a TranslationUnit, name: &str) -> Vec<&'a Type> {
    let declaration = unit
        .declarations
        .iter()
        .find(|declaration| declaration.name == name)
        .unwrap();
    let TypeKind::Function(function) = &declaration.ty.kind else {
        panic!("function expected")
    };
    function
        .parameters
        .iter()
        .map(|parameter| &parameter.ty)
        .collect()
}

fn record_id(ty: &Type) -> usize {
    let TypeKind::Pointer(pointee) = &ty.kind else {
        panic!("pointer expected")
    };
    let TypeKind::Record(id) = pointee.kind else {
        panic!("record expected")
    };
    id
}

#[test]
fn prototype_declarations_have_lexical_scope() {
    for (source, accepted) in CASES {
        let result = analyze(source, TARGET);
        assert_eq!(result.is_ok(), *accepted, "{source}: {result:?}");
    }
}

#[test]
fn prototype_tags_keep_distinct_identities() {
    let unit = analyze(
        "int f(struct S *, struct S *); int g(struct S *); struct S { int x; }; int h(struct S *);",
        TARGET,
    )
    .unwrap();
    let f = parameters(&unit, "f");
    let g = parameters(&unit, "g");
    let h = parameters(&unit, "h");
    assert_eq!(record_id(f[0]), record_id(f[1]));
    assert_ne!(record_id(f[0]), record_id(g[0]));
    assert_ne!(record_id(g[0]), record_id(h[0]));
    for id in [record_id(f[0]), record_id(g[0])] {
        assert_eq!(unit.records[id].scope, Scope::Prototype);
        assert_eq!(unit.records[id].name.as_deref(), Some("S"));
        assert!(unit.records[id].fields.is_none());
    }
    assert_eq!(unit.records[record_id(h[0])].scope, Scope::File);
    assert!(unit.records[record_id(h[0])].fields.is_some());
}

#[test]
fn nested_prototypes_reuse_only_visible_tags() {
    for (source, shared) in [
        ("int f(int (*callback)(struct S *), struct S *);", false),
        (
            "int f(struct S *, int (*callback)(struct S *), struct S *);",
            true,
        ),
    ] {
        let unit = analyze(source, TARGET).unwrap();
        let params = parameters(&unit, "f");
        let callback = params[usize::from(shared)];
        let TypeKind::Pointer(callback) = &callback.kind else {
            panic!("callback pointer expected")
        };
        let TypeKind::Function(callback) = &callback.kind else {
            panic!("callback expected")
        };
        assert_eq!(
            record_id(params.last().unwrap()) == record_id(&callback.parameters[0].ty),
            shared
        );
    }
}

#[test]
fn prototype_definitions_restore_file_scope_for_constant_evaluation() {
    let unit = analyze("struct S { int x; }; enum E { A = 3 }; int f(struct S { char x; } *, enum E { A = 7 } value);", TARGET).unwrap();
    assert_eq!(
        evaluate_integer(&unit, "sizeof(struct S)")
            .unwrap()
            .as_u64()
            .unwrap(),
        4
    );
    assert_eq!(evaluate_integer(&unit, "A").unwrap().as_u64().unwrap(), 3);
    assert_eq!(
        unit.enums
            .iter()
            .filter(|value| value.scope == Scope::File)
            .count(),
        1
    );
    assert_eq!(
        unit.enums
            .iter()
            .filter(|value| value.scope == Scope::Prototype)
            .count(),
        1
    );
    let local = analyze(
        "int f(struct S { char x; } *, enum E { A = 7 } value);",
        TARGET,
    )
    .unwrap();
    assert!(evaluate_integer(&local, "sizeof(struct S)").is_err());
    assert!(evaluate_integer(&local, "A").is_err());
    assert!(local.constants.is_empty());
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn prototype_scope_matches_gcc_and_clang() {
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
                .unwrap();
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

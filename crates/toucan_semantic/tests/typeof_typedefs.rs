use toucan_semantic::analyze;
use toucan_target::Target;

const CASES: &[(&str, bool)] = &[
    (
        "typedef long T; int (*f(int T))(int) { __typeof__(T) a; _Static_assert(sizeof a == sizeof(int), \"parameter\"); return 0; }",
        true,
    ),
    (
        "typedef long T; int f(int (*callback)(int T)) { __typeof__(T) a; _Static_assert(sizeof a == sizeof(long), \"callback\"); return 0; }",
        true,
    ),
    (
        "typedef long T; int f(enum { T = 1 } x) { __typeof__(T) a; _Static_assert(sizeof a == sizeof(int), \"enumerator\"); return 0; }",
        true,
    ),
    (
        "typedef long T; __typeof__(int (*)(int T)) callback; T value; _Static_assert(sizeof value == sizeof(long), \"prototype\");",
        true,
    ),
    ("typedef int T; __typeof__(T) a = 1;", true),
    ("typedef int T; __typeof__(const T) a = 1;", true),
    ("typedef int T; __typeof__(T *) a = 0;", true),
    (
        "typedef int T; __typeof__(T[3]) a = {1, 2, 3}; _Static_assert(sizeof a == 3*sizeof(int), \"array\");",
        true,
    ),
    (
        "typedef int T; __typeof__(T (void)) callback; int f(void) { return callback(); }",
        true,
    ),
    (
        "typedef int T; __typeof__(T (*)(void)) callback; int f(void) { return callback(); }",
        true,
    ),
    ("typedef int T; __typeof__(__typeof__(T) *) pointer;", true),
    ("typedef int T; __typeof__((T){1}) a = 2;", true),
    (
        "int f(int n) { typedef int A[n]; __typeof__(A) a; return sizeof a; }",
        true,
    ),
    (
        "typedef long T; int f(int T) { __typeof__(T) a; _Static_assert(sizeof a == sizeof(int), \"parameter\"); return 0; }",
        true,
    ),
    (
        "typedef long T; int f(void) { int T; __typeof__(T) a; _Static_assert(sizeof a == sizeof(int), \"object\"); return 0; }",
        true,
    ),
    (
        "typedef long T; int f(void) { enum { T }; __typeof__(T) a; _Static_assert(sizeof a == sizeof(int), \"enumerator\"); return 0; }",
        true,
    ),
    (
        "typedef long T; int f(void) { { int T; __typeof__(T) a; _Static_assert(sizeof a == sizeof(int), \"inner\"); } __typeof__(T) b; _Static_assert(sizeof b == sizeof(long), \"outer\"); return 0; }",
        true,
    ),
    ("typedef int T; __typeof__((T)) a;", false),
    ("typedef int T; __typeof__(T + 1) a;", false),
    ("typedef int T; __typeof__(T[3]) a = {1, 2, 3, 4};", false),
    (
        "typedef int T; __typeof__(T (void)) callback; int f(void) { return callback(1); }",
        false,
    ),
    (
        "int f(int n) { typedef int A[n]; __typeof__(A) a = {1}; return 0; }",
        false,
    ),
    (
        "int f(int n) { typedef int A[n]; goto L; __typeof__(A) a; L: return 0; }",
        false,
    ),
];

const BOUND_EFFECTS: &str = r#"
    int main(void) {
        int n = 3;
        typedef int A[n++];
        __typeof__(A) a;
        __typeof__(A[2]) b;
        __typeof__(A *) p = &a;
        if (n != 4 || sizeof a != 3*sizeof(int) || sizeof b != 6*sizeof(int)
            || sizeof *p != 3*sizeof(int)) return 1;
        __typeof__(A[n++]) c;
        if (n != 5 || sizeof c != 12*sizeof(int)) return 2;
        return 0;
    }
"#;

#[test]
fn typedef_operands_obey_types_scopes_and_vla_constraints() {
    for target in Target::ALL {
        for &(source, accepted) in CASES {
            let result = analyze(source, target);
            assert_eq!(result.is_ok(), accepted, "{target}: {source}: {result:?}");
        }
        analyze(BOUND_EFFECTS, target).unwrap();
    }
}

#[test]
fn typeof_errors_preserve_original_offsets_after_source_adapters() {
    let source =
        "struct S { int x; }; struct S s = (struct S){}; typedef int T; __typeof__((T)) a;";
    let error = analyze(source, Target::X86_64UnknownLinuxGnu).unwrap_err();
    assert!(
        error.message.contains("typedef name is not an expression"),
        "{error}"
    );
    assert_eq!(error.offset, source.rfind("(T))").unwrap());
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn typeof_typedef_constraints_and_bound_effects_match_c_compilers() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("typeof.c");
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        "clang".into(),
    ] {
        for &(source, accepted) in CASES {
            std::fs::write(&input, format!("{source}\n")).unwrap();
            let output = std::process::Command::new(&compiler)
                .args(["-std=c11", "-pedantic-errors", "-fsyntax-only"])
                .arg(&input)
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                accepted,
                "{compiler}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::fs::write(&input, BOUND_EFFECTS).unwrap();
        for optimization in ["-O0", "-O2"] {
            let executable = directory.path().join("typeof");
            let output = std::process::Command::new(&compiler)
                .args(["-std=c11", "-pedantic-errors", optimization])
                .arg(&input)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                std::process::Command::new(&executable)
                    .status()
                    .unwrap()
                    .success(),
                "{compiler} {optimization}: VLA bound was reused or evaluated incorrectly"
            );
        }
    }
}

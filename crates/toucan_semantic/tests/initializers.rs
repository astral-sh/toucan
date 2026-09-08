use toucan_semantic::{analyze, evaluate_integer};
use toucan_target::Target;

const VALID: &[&str] = &[
    "struct S { int n; int a[]; }; struct S s = {1, {2}};",
    "int x = 1;",
    "const int x = 1;",
    "double x = 1 + 2.5;",
    "float x = -1.5f;",
    "int x = {1};",
    "int a[] = {1, 2, 3}; _Static_assert(sizeof a == 3 * sizeof(int), \"array bound\");",
    "int a[5] = {[3] = 1, 2};",
    "int a[] = {[4] = 1, [2] = 2}; _Static_assert(sizeof a == 5 * sizeof(int), \"highest index\");",
    "int a[][2] = {1, 2, 3}; _Static_assert(sizeof a == 4 * sizeof(int), \"elided braces\");",
    "int a[][2] = {{1}, {2}};",
    "int a[2][2] = {[0][1] = 1, 2, 3};",
    "int a[2][2] = {[0] = {1, 2}, [1][0] = 3};",
    "int a[3] = {[1] = 2, [1] = 3};",
    "char s[] = \"abc\"; _Static_assert(sizeof s == 4, \"string bound\");",
    "char s[3] = \"abc\";",
    "char s[] = {\"abc\"};",
    "signed char s[] = \"abc\";",
    "unsigned char s[] = \"abc\";",
    "char s[2][4] = {\"abc\", \"def\"};",
    "typedef char A[]; A a = \"abc\"; A b = \"x\"; _Static_assert(sizeof a == 4 && sizeof b == 2, \"independent bounds\");",
    "typedef const int A[]; A a = {1, 2}; _Static_assert(sizeof a == 2 * sizeof(int), \"qualified bound\");",
    "struct S { int x, y; }; struct S s = {1, 2};",
    "struct S { int x, y; }; struct S s = {.y = 2};",
    "struct S { int x, y; }; struct S a[] = {1, 2, 3, 4};",
    "struct S { int x[2], y; }; struct S s = {1, 2, 3};",
    "struct S { int x[2], y; }; struct S s = {.x[1] = 1, 2};",
    "struct S { int x[2], y; }; struct S s = {.x = {1}, 2};",
    "struct S { struct { int x, y; }; int z; }; struct S s = {.y = 1, 2};",
    "struct S { int : 2; int x; }; struct S s = {1};",
    "struct S { unsigned x : 2, y : 3; }; struct S s = {.y = 1};",
    "union U { int x; double y; }; union U u = {1};",
    "union U { int x; double y; }; union U u = {.y = 1.5};",
    "union U { int a[2]; double y; }; union U u = {1, 2};",
    "union U { int x; double y; }; union U u = {.x = 1, .y = 2};",
    "struct S { int n; int a[]; }; struct S s = {1};",
    "extern int a[3]; int a[] = {1, 2, 3};",
    "int f(int); extern int (*p)(int); int (*p)() = f;",
    "extern int a[5]; int a[] = {1, 2}; _Static_assert(sizeof a == 5 * sizeof(int), \"prior bound\");",
    "extern int a[5]; int a[] = {[2] = 1}; _Static_assert(sizeof a == 5 * sizeof(int), \"prior designated bound\");",
    "extern int a[2][3]; int a[][3] = {{1}}; _Static_assert(sizeof a == 6 * sizeof(int), \"prior nested bound\");",
    "extern char a[5]; char a[] = \"hi\"; _Static_assert(sizeof a == 5, \"prior string bound\");",
    "extern unsigned short a[2]; unsigned short a[] = u\"\\U0001F600\"; _Static_assert(sizeof a == 2 * sizeof(unsigned short), \"omitted terminator\");",
    "int x, *p = &x;",
    "int *p = 0;",
    "int *p = (void *)0;",
    "int *p = (int *)4;",
    "int a[3]; int *p = a + 2;",
    "int a[3]; int *p = &a[1];",
    "int a[3]; int *p = &1[a];",
    "int a[3]; int *p = 1 + a;",
    "const char *p = \"abc\" + 1;",
    "struct S { int x; }; struct S s; int *p = &s.x;",
    "void f(int); void (*p)(int) = f;",
    "int *p = &*(int *)0;",
    "void *p = &p;",
    "int f(void); int a = 0 && f();",
    "int f(void); int a = 1 ? 2 : f();",
    "int a = sizeof((int[]){1, 2});",
    "int *p = (int[]){1, 2};",
    "struct S { int x; }; struct S *p = &(struct S){1};",
    "struct S { int x; }; struct S s = (struct S){1};",
    "int a = _Generic(1, int: 2, default: 3);",
];

const INVALID: &[&str] = &[
    "int a = unknown;",
    "struct S { int x; }; struct S s = 42;",
    "struct S; struct S s = {1};",
    "int f(void) = 1;",
    "int a[2] = 1;",
    "int a[2] = {1, 2, 3};",
    "int a[2] = {[2] = 1};",
    "int a[2] = {[-1] = 1};",
    "int a[2] = {.x = 1};",
    "int a = {.x = 1};",
    "int a = {1, 2};",
    "int a[1][1] = {{1, 2}};",
    "char s[2] = \"abc\";",
    "int s[] = \"abc\";",
    "struct S { int x; }; struct S s = {.y = 1};",
    "struct S { int x; }; struct S s = {[0] = 1};",
    "struct S { int x; }; struct S s = {1, 2};",
    "union U { int x, y; }; union U u = {1, 2};",
    "extern int a[3]; int a[] = {1, 2, 3, 4};",
    "int f(double); extern int (*p)(int); int (*p)() = f;",
    "extern int a[2]; int a[] = {[2] = 1};",
    "extern char a[2]; char a[] = \"abc\";",
    "const int x = 1; int *p = &x;",
    "int x; double *p = &x;",
    "int *p = 1;",
    "int x = (void *)0;",
    "int f(void); int x = f();",
    "int x; int y = x;",
    "int x; int *p = &x; int *q = p;",
    "int x = 1 / 0;",
    "int a = (1, 2);",
    "int a = sizeof((int[1]){1, 2});",
];

#[test]
fn initializers_check_types_and_complete_arrays() {
    for source in VALID {
        assert!(
            analyze(source, Target::X86_64UnknownLinuxGnu).is_ok(),
            "{source}: {:?}",
            analyze(source, Target::X86_64UnknownLinuxGnu)
        );
    }
    for source in INVALID {
        assert!(
            analyze(source, Target::X86_64UnknownLinuxGnu).is_err(),
            "accepted: {source}"
        );
    }
}

#[test]
fn sparse_designators_do_not_expand_implicit_elements() {
    let unit = analyze(
        "unsigned char a[] = {[1099511627776] = 1};",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    assert_eq!(
        evaluate_integer(&unit, "sizeof a").unwrap().value,
        1099511627777
    );
    let unit = analyze(
        "int a[] = {[0 ... 1099511627776] = 1};",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    assert_eq!(
        evaluate_integer(&unit, "sizeof a").unwrap().value,
        4 * 1099511627777
    );
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn initializer_constraints_match_c_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    for compiler in ["gcc", "clang"] {
        for source in VALID {
            let mut child = Command::new(compiler)
                .args(["-std=c11", "-fsyntax-only", "-x", "c", "-"])
                .stdin(Stdio::piped())
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
            assert!(
                output.status.success(),
                "{compiler}: {source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        // These are C constraint violations. Reference compilers sometimes emit
        // only warnings by default; request diagnostics as errors for the oracle.
        for source in INVALID {
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
            assert!(
                !output.status.success(),
                "{compiler} accepted: {source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

const PRIOR_BOUND_SOURCE: &str = r#"
    extern int values[5]; int values[] = {1, 2};
    extern int sparse[5]; int sparse[] = {[2] = 7};
    extern int grid[2][3]; int grid[][3] = {{9}};
    extern char text[5]; char text[] = "hi";
    extern unsigned short emoji[2]; unsigned short emoji[] = u"\U0001F600";
    _Static_assert(sizeof values == 5 * sizeof(int), "prior bound");
    _Static_assert(sizeof sparse == 5 * sizeof(int), "prior designated bound");
    _Static_assert(sizeof grid == 6 * sizeof(int), "prior nested bound");
    _Static_assert(sizeof text == 5, "prior string bound");
    _Static_assert(sizeof emoji == 2 * sizeof(unsigned short), "omitted terminator");
    int main(void) {
        return values[0] != 1 || values[1] != 2 || values[4] != 0
            || sparse[0] != 0 || sparse[2] != 7 || sparse[4] != 0
            || grid[0][0] != 9 || grid[1][2] != 0
            || text[0] != 'h' || text[2] != 0 || text[4] != 0
            || emoji[0] != 0xd83d || emoji[1] != 0xde00;
    }
"#;

#[test]
fn prior_declarations_supply_initializer_bounds_on_every_target() {
    for target in Target::ALL {
        analyze(PRIOR_BOUND_SOURCE, target).unwrap();
    }
}

#[test]
#[ignore = "requires native GCC and Clang plus all Clang target backends; run with --include-ignored"]
fn prior_bounds_and_zero_fill_match_c_compilers() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("bounds.c");
    std::fs::write(&input, PRIOR_BOUND_SOURCE).unwrap();
    for compiler in ["gcc", "clang"] {
        let executable = directory.path().join(format!("{compiler}.exe"));
        let output = std::process::Command::new(compiler)
            .args(["-std=c11", "-pedantic-errors"])
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
            std::process::Command::new(executable)
                .status()
                .unwrap()
                .success(),
            "{compiler}"
        );
    }
    for target in Target::ALL {
        let output = std::process::Command::new("clang")
            .args([
                "-std=c11",
                "-pedantic-errors",
                "-fsyntax-only",
                "-target",
                target.triple(),
            ])
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

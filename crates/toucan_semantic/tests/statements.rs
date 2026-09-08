use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::{Scope, analyze};
use toucan_target::Target;

const TARGET: Target = Target::X86_64UnknownLinuxGnu;

#[test]
#[ignore = "requires a C compiler (CC or cc); run with --include-ignored"]
fn function_body_constraints_match_c11_compiler() {
    let valid = [
        "int f(int x) { return x + 1; }",
        "int f(void) { register int a[2]; return sizeof(a); }",
        "int f(int previous); int f(int actual) { return actual; }",
        "typedef int T; int f(void) { extern int T; return T; }",
        "int f(void) { extern int a[]; extern int a[2]; return sizeof(a); }",
        "int function(void) { _Static_assert(sizeof(__func__) == 9, \"name\"); return __func__[0]; }",
        "int f(int x) { int values[] = { x, 2 }; return values[0]; }",
        "int f(int x) { struct S { int value; }; struct S s = (struct S){ x }; return s.value; }",
        "int global; int f(void) { static int *p = &global; static int x = 1; return *p + x; }",
        "int f(void) { static int values[2]; static int *p = values; return *p; }",
        "int f(int x) { if (sizeof(struct S {int x;})) { struct S s; return sizeof(s); } return x; }",
        "void f(void) { return; }",
        "int f(int x) { int y = x; y += 2; return y; }",
        "int x; int f(void) { int x = 1; { double x = 2; x++; } return x; }",
        "enum { X = 3 }; int f(void) { int X = 4; { enum { X = 5 }; _Static_assert(X == 5, \"inner\"); } return X; } _Static_assert(X == 3, \"outer\");",
        "typedef int T; T g; int f(T x) { typedef double T; _Static_assert(sizeof(x) == sizeof(int), \"parameter\"); _Static_assert(sizeof(g) == sizeof(int), \"global\"); T y = 1; return y; }",
        "typedef int T; int f(void) { typedef double T; { typedef char T; T x = 1; } T y = 1; return y; } T g;",
        "struct S { int x; }; int f(void) { struct S { double x; } s; return sizeof(s); }",
        "struct S { int x; }; int f(void) { struct S; struct S *s; return sizeof(s); }",
        "int f(struct S { int x; } value) { struct S copy = value; return copy.x; }",
        "int f(enum E { A = 1 } value) { return value + A; }",
        "int f(int x) { if (x) return 1; else return 2; }",
        "int f(int x) { while (x) { x--; if (x) continue; break; } return x; }",
        "int f(int x) { do { --x; } while (x); return x; }",
        "int f(void) { int total = 0; for (int i = 0; i < 4; i++) total += i; return total; }",
        "int f(int x) { switch (x) { case 1: return 2; case 2: break; default: return 3; } return 0; }",
        "int f(int x) { switch (x) { case 0: switch(x) { case 0: break; } break; } return 0; }",
        "int f(int x) { goto done; again: x++; done: if (!x) goto again; return x; }",
        "int g(int); int f(int x) { int g(int); return g(x); }",
        "int x; int f(void) { extern int x; extern int x; return x; }",
        "int f(void) { register int x = 1; return x; }",
        "int f(register int x) { return x; }",
        "enum { RESULT = 1 ? sizeof(struct S { int x; }) : 0 };",
        "enum { RESULT = 1 ? (enum E { A = 0 })0 : 0 };",
    ];
    let invalid = [
        "int f(void) { return missing; }",
        "int v(int, ...); int f(void) { register int a[2]; return v(0, a); }",
        "int f(void) { register int a[2]; return sizeof(&*a); }",
        "int f(void) { register int a[2]; return a[0]; }",
        "int f(void) { register int a[2]; return _Generic(a, int*:1, default:0); }",
        "int f(void) { register int a[2]; return (a,0); }",
        "int f(void) { register int a[2]; if(a) return 1; return 0; }",
        "int f(void) { register int a[2]; int *p = a; return *p; }",
        "int f(void) { int x; extern int x; return x; }",
        "int f(void) { static int x; extern int x; return x; }",
        "int f(int x) { static int y = x; return y; }",
        "int f(void) { int x; static int *p = &x; return *p; }",
        "int f(void) { int values[2]; static int *p = values; return *p; }",
        "int f(void) { static int *p = (int[]){1}; return *p; }",
        "int f(void) { static int *p = &(int){1}; return *p; }",
        "int f(void) { int values[1] = {1, 2}; return values[0]; }",
        "int f(void) { int values[1] = {[2] = 1}; return values[0]; }",
        "int f(void) { register struct S {int value;} s; return *(&s.value); }",
        "int f(void) { if (sizeof(struct S {int x;})) {} struct S s; return sizeof(s); }",
        "void f(void) { return 1; }",
        "int f(void) { return; }",
        "int *f(void) { return 1; }",
        "int *f(const int *p) { return p; }",
        "int f(int x) { int x; return x; }",
        "int f(void) { int x; double x; return 0; }",
        "int f(void) { int x; enum { x = 1 }; return x; }",
        "int f(void) { enum { x = 1 }; int x; return x; }",
        "int f(void) { int x; typedef int x; return 0; }",
        "int f(void) { typedef int T; typedef double T; return 0; }",
        "int f(void) { { int x = 1; } return x; }",
        "int f(void) { for (int i = 0; i < 1; i++) {} return i; }",
        "struct S {int x;}; int f(struct S s) { if (s) return 1; return 0; }",
        "struct S {int x;}; int f(struct S s) { while (s) {} return 0; }",
        "int f(double x) { switch (x) { default: break; } return 0; }",
        "int f(int x) { switch (x) { case 1: break; case 1: break; } return 0; }",
        "int f(unsigned x) { switch (x) { case -1: break; case 4294967295U: break; } return 0; }",
        "int f(int x) { switch (x) { default: break; default: break; } return 0; }",
        "int f(void) { case 1: return 0; }",
        "int f(void) { default: return 0; }",
        "int f(void) { break; return 0; }",
        "int f(void) { continue; return 0; }",
        "int f(int x) { switch (x) { default: continue; } return 0; }",
        "int f(void) { goto missing; return 0; }",
        "int f(void) { label: ; label: return 0; }",
        "int g(int *); int f(void) { return g(1); }",
        "int f(void) { struct S; struct S s; return 0; }",
        "int f(void) { void x; return 0; }",
        "int f(void) { const int x = 1; x = 2; return x; }",
        "int f(void) { register int x; int *p = &x; return 0; }",
        "int f(register int x) { return *(&x); }",
        "int f(void) { static int g(void); return 0; }",
        "int x; int f(void) { extern double x; return 0; }",
        "int f(void) { extern double x; return 0; } int x;",
        "int f(void) { extern int x = 1; return x; }",
        "int f(void) { for (static int i = 0; i < 1; i++) {} return 0; }",
    ];
    for (expected, cases) in [(true, valid.as_slice()), (false, invalid.as_slice())] {
        for source in cases {
            let source = format!("{source}\n");
            let compiler = compile(&source);
            assert_eq!(compiler, expected, "unexpected reference result: {source}");
            let actual = analyze(&source, TARGET);
            assert_eq!(actual.is_ok(), compiler, "{source}: {actual:?}");
        }
    }
}

#[test]
fn local_bindings_do_not_leak_into_the_public_api() {
    let unit = analyze("typedef int T; enum { E = 1 }; int f(int p) { typedef double T; enum { E = 2 }; struct S { T member; }; T local = p; return local + E; }", TARGET).unwrap();
    assert_eq!(unit.constants["E"].value, 1);
    assert!(
        !unit
            .typedefs
            .keys()
            .any(|name| name.starts_with("__toucan_block_"))
    );
    assert_eq!(
        unit.declarations
            .iter()
            .map(|declaration| declaration.name.as_str())
            .collect::<Vec<_>>(),
        ["T", "f"]
    );
    assert_eq!(unit.records.last().unwrap().scope, Scope::Block);
    assert_eq!(unit.enums.last().unwrap().scope, Scope::Block);
    assert!(
        unit.layout(&unit.records.last().unwrap().fields.as_ref().unwrap()[0].ty)
            .is_ok()
    );
}

#[test]
#[ignore = "requires a GNU-compatible C compiler; run with --include-ignored"]
fn gnu_case_ranges_and_static_compound_copies_match_compiler() {
    for (expected, source) in [
        (
            true,
            "int f(int x) { switch(x) { case -10 ... -1: return 0; case 0 ... 2147483647: return 1; } return 2; }",
        ),
        (
            true,
            "int f(unsigned x) { switch(x) { case 0 ... 4294967295U: return 1; } return 0; }",
        ),
        (
            true,
            "int f(void) { static struct S { int x; } s = (struct S){1}; return s.x; }",
        ),
        (
            false,
            "int f(int x) { switch(x) { case 0 ... 100: break; case 50 ... 150: break; } return 0; }",
        ),
    ] {
        let source = format!("{source}\n");
        assert_eq!(compile_with_mode(&source, true), expected, "{source}");
        let actual = analyze(&source, TARGET);
        assert_eq!(actual.is_ok(), expected, "{source}: {actual:?}");
    }
}

#[test]
fn large_switches_and_deep_blocks_have_bounded_work() {
    let mut source = String::from("int f(int x) { switch(x) {");
    for value in 0..4096 {
        source.push_str(&format!("case {value}: break;"));
    }
    source.push_str("} return 0; }");
    analyze(&source, TARGET).unwrap();
    let source = format!(
        "int f(void) {{ {} return 0; {} }}",
        "{".repeat(160),
        "}".repeat(160)
    );
    assert!(analyze(&source, TARGET).is_err());
}

fn compile(source: &str) -> bool {
    compile_with_mode(source, false)
}

fn compile_with_mode(source: &str, gnu: bool) -> bool {
    let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()));
    command.args([
        "-x",
        "c",
        if gnu { "-std=gnu11" } else { "-std=c11" },
        "-fsyntax-only",
        "-",
    ]);
    if !gnu {
        command.arg("-pedantic-errors");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the differential test requires a C compiler (set CC)");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap().status.success()
}

#[test]
fn block_aliases_cannot_conflict_with_later_source_names() {
    let source = "int f(void) { typedef const int T; T x = 1; return x; } typedef double __toucan_block_0_T; __toucan_block_0_T x;";
    let unit = analyze(source, TARGET).unwrap();
    assert_eq!(
        toucan_semantic::evaluate_integer(&unit, "sizeof(x)")
            .unwrap()
            .value,
        8
    );
    assert!(
        analyze(
            "void f(void) { typedef const int T; T x = 1; x = 2; }",
            TARGET
        )
        .is_err()
    );
}

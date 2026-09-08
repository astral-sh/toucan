use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::analyze;
use toucan_target::Target;

const GNU: Target = Target::X86_64UnknownLinuxGnu;
const CLANG: Target = Target::X86_64AppleDarwin;

const VALID: &[&str] = &[
    "int f(int x) { return ({ int y = x + 1; y * 2; }); }",
    "int f(int x) { return ({ typedef int T; T x = 3; x; }) + x; }",
    "int f(void) { int x = ({ int x = 2; x; }); return x; }",
    "int f(void) { ({ }); return 0; }",
    "int f(int x) { return ({ x; ; }); }",
    "int f(int x) { return ({ label: x; }); }",
    "int f(int x) { return ({ if (x) return 1; x; }); }",
    "int f(int x) { while (x) { ({ if (x) break; continue; }); } return x; }",
    "int f(void) { return ({ switch (1) { case 1: break; default: break; } 1; }); }",
    "int f(void) { int x = ({ goto out; 1; }); out: return 0; }",
    "int f(void) { return ({ ({ goto out; 1; }); out: 2; }); }",
    "int f(void) { return ({ int x = 2; x + ({ int y = 3; y; }); }); }",
    "int f(int n) { return ({ int a[n]; sizeof a > 0; }); }",
    "int f(int n) { return ({ int a[n]; goto done; done: sizeof a > 0; }); }",
    "int f(int n) { int a[n]; return ({ goto out; 1; }); out: return sizeof a > 0; }",
    "int f(void) { _Static_assert(sizeof(({ label: 1; })) == sizeof(int), \"size\"); return 0; }",
    "int f(void) { _Static_assert(_Generic(({ int x; x; }), int: 1, default: 0), \"type\"); return 0; }",
    "int f(void) { int a[3]; _Static_assert(sizeof(({a;})) == sizeof(int*), \"decay\"); return 0; }",
    "int g(void); int f(void) { int (*p)(void) = ({g;}); return p(); }",
    "struct S { int x; }; int f(void) { struct S s = ({ struct S t = {1}; t; }); return s.x; }",
    "int f(void) { ({struct S {int x;}; struct S s = {0}; s;}); struct S {double y;}; return sizeof(struct S); }",
    "int f(void) { int x = 1; (1 ? x : ({ goto done; })); done: return x; }",
    "int f(int n) { switch (n) { case 1: return ({ 2; }); } return 0; }",
];

const INVALID: &[&str] = &[
    "int x = ({1;});",
    "int x = sizeof(({1;}));",
    "int f(void) { return ({ int x = 1; }); }",
    "int f(void) { return ({ 1; if (1) {} }); }",
    "int f(void) { ({ int x = 1; x; }); return x; }",
    "int f(void) { return ({ missing; 1; }); }",
    "int f(void) { return ({ int x = 1; int x = 2; x; }); }",
    "int f(void) { return ({ const int x = 1; x = 2; x; }); }",
    "int f(void) { return ({ break; 1; }); }",
    "int f(void) { return ({ continue; 1; }); }",
    "int f(void) { return ({ return; 1; }); }",
    "int f(void) { return ({ goto missing; 1; }); }",
    "int f(void) { goto in; return ({ in: 1; }); }",
    "int f(void) { ({ goto in; 1; }); return ({ in: 1; }); }",
    "int f(void) { return ({ label: 1; label: 2; }); }",
    "int f(void) { switch(1) { ({ case 1: ; }); } return 0; }",
    "int f(void) { switch(1) { ({ default: ; }); } return 0; }",
    "int f(int n) { return ({ goto in; int a[n]; in: 1; }); }",
    "int f(int n) { return ({ switch(1) { int a[n]; case 1: ; } 1; }); }",
    "int g(void); int f(void) { static int x = ({g();}); return x; }",
    "int f(int x) { enum { E = ({x;}) }; return E; }",
    "int f(int x) { ({ int y; x; }) = 1; return x; }",
    "int f(int x) { ({ 1; x; }) = 1; return x; }",
    "int f(int x) { return *&({const int y=x; y;}); }",
    "int f(void) { register int a[3]; return ({ a; })[0]; }",
];

// Target-specific behavior follows each profile's compiler, not the host compiler.
const GNU_LVALUES: &[&str] = &[
    "int f(int x) { ({x;}) = 1; return x; }",
    "int f(int x) { ({;x;;}) = 1; return x; }",
    "int f(int x) { return *&({x;}); }",
    "int f(int x) { ({_Static_assert(1,\"ok\");x;}) = 1; return x; }",
    "int f(int x) { return ({x;_Static_assert(1,\"ok\");}); }",
];

const CLANG_LOOPS: &[&str] = &[
    "int f(void) { while (({break;1;})) {} return 0; }",
    "int f(void) { do {} while (({continue;1;})); return 0; }",
    "int f(void) { for (;({break;1;});) {} return 0; }",
    "int f(void) { for (;;({continue;})) {} return 0; }",
];

#[test]
fn statement_expressions_check_types_scopes_and_jumps() {
    for target in [GNU, CLANG] {
        for source in VALID {
            analyze(source, target).unwrap_or_else(|error| panic!("{target}: {source}: {error}"));
        }
        for source in INVALID {
            assert!(analyze(source, target).is_err(), "{target}: {source}");
        }
        for source in GNU_LVALUES {
            assert_eq!(
                analyze(source, target).is_ok(),
                target == GNU,
                "{target}: {source}"
            );
        }
        for source in CLANG_LOOPS {
            assert_eq!(
                analyze(source, target).is_ok(),
                target == CLANG,
                "{target}: {source}"
            );
        }
    }
    let source = "struct S { unsigned b: 3; }; int f(struct S s) { return ({s.b;}); }";
    assert!(
        analyze(source, GNU)
            .unwrap_err()
            .message
            .contains("bit-field result types")
    );
    analyze(source, CLANG).unwrap();
    // Compilers fold some statement expressions as an extension; evaluation of
    // their enclosed statements is a separate capability from body checking.
    for target in [GNU, CLANG] {
        let error =
            analyze("int f(void) { static int x = ({1;}); return x; }", target).unwrap_err();
        assert!(
            error
                .message
                .contains("constant evaluation of statement expressions is unsupported")
        );
    }
}

#[test]
fn statement_expression_work_is_bounded_and_metadata_does_not_leak() {
    let mut expression = "1".to_owned();
    for _ in 0..150 {
        expression = format!("({{{expression};}})");
    }
    assert!(analyze(&format!("int f(void) {{ return {expression}; }}"), GNU).is_err());
    let source = "int f(void) {return ({int x = 1; x;});} int g(void) {return ({int x = 2; x;});}";
    let unit = analyze(source, GNU).unwrap();
    assert_eq!(unit.declarations.len(), 2);
    assert!(
        unit.declarations
            .iter()
            .all(|declaration| declaration.name != "x")
    );
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn statement_expression_constraints_match_native_compilers() {
    for compiler in ["gcc", "clang"] {
        let target = if is_gnu_compiler(compiler) {
            GNU
        } else {
            CLANG
        };
        for (cases, accepted) in [
            (VALID, true),
            (INVALID, false),
            (GNU_LVALUES, target == GNU),
            (CLANG_LOOPS, target == CLANG),
        ] {
            for source in cases {
                let mut child = Command::new(compiler)
                    .args([
                        "-std=gnu11",
                        "-Werror=incompatible-pointer-types",
                        "-Werror=int-conversion",
                        "-Werror=return-type",
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
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&output),
                    Ok(accepted),
                    "{compiler}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

/// macOS installs Apple Clang under both `clang` and `gcc` command names.
fn is_gnu_compiler(compiler: &str) -> bool {
    let output = Command::new(compiler)
        .arg("--version")
        .output()
        .expect("required C compiler");
    assert!(output.status.success(), "{compiler} --version failed");
    let version = String::from_utf8(output.stdout).unwrap();
    let gnu = version.contains("Free Software Foundation");
    assert!(
        gnu || version.to_ascii_lowercase().contains("clang"),
        "unknown compiler: {version}"
    );
    gnu
}

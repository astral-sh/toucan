use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::analyze;
use toucan_target::Target;

const VALID: &[&str] = &[
    "void f(const void *p) { int (*function)(int) = p; }",
    "void f(volatile void *p) { int (*function)(int) = p; }",
    "int f(void); void *p = f;",
    "int f(void); const void *p = &f;",
    "int f(void); void *f2(void) { return f; }",
    "void call(void *); int f(void); void g(void) { call(f); }",
    "void f(void *p) { int (*function)(int) = p; function(1); }",
    "void f(void *p, int (*function)(int)) { function = p; p = function; }",
    "int f(void *p, int (*function)(int)) { return p == function || function != p; }",
    "void *f(int c, void *p, int (*function)(int)) { return c ? p : function; }",
    "void *f(int c, void *p, int (*function)(int)) { return c ? function : p; }",
    "int f(int c, void *p, int (*function)(int)) { return _Generic(c ? function : p, void *: 1, default: 0); }",
];
const INVALID: &[&str] = &[
    "void f(int *p) { int (*function)(int) = p; }",
    "int f(void); int *p = f;",
    "void f(int *p, int (*function)(int)) { p = function; }",
    "void f(void **p) { int (**function)(int) = p; }",
];

#[test]
fn void_function_pointer_conversions_preserve_qualifiers() {
    for target in Target::ALL {
        for source in VALID {
            analyze(source, target).unwrap_or_else(|error| panic!("{target}: {source}: {error}"));
        }
        for source in INVALID {
            assert!(analyze(source, target).is_err(), "{target}: {source}");
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn gnu_pointer_conversions_match_compilers() {
    for compiler in ["gcc", "clang"] {
        let output = Command::new(compiler).arg("--version").output().unwrap();
        assert!(output.status.success());
        let gnu = String::from_utf8(output.stdout)
            .unwrap()
            .contains("Free Software Foundation");
        let profile_source = format!(
            "void f(int c, const void *p, int (*function)(int)) {{ _Static_assert(_Generic(c ? function : p, const void *: 1, void *: 0) == {}, \"qualifiers\"); }}",
            u8::from(gnu)
        );
        let valid: Vec<_> = VALID
            .iter()
            .copied()
            .chain([profile_source.as_str()])
            .collect();
        for (cases, accepted) in [(valid.as_slice(), true), (INVALID, false)] {
            for source in cases {
                // Clang warns for a function/void conditional even in GNU mode.
                // Valid extension cases permit that diagnostic; invalid cases
                // promote constraint warnings to errors on older GCC releases.
                let mut command = Command::new(compiler);
                if !accepted {
                    command.arg("-Werror");
                }
                let mut child = command
                    .args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"])
                    .stdin(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                writeln!(child.stdin.take().unwrap(), "{source}").unwrap();
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

#[test]
fn conditional_pointer_qualifiers_follow_the_compiler_profile() {
    for target in Target::ALL {
        let gnu = matches!(
            target,
            Target::I686UnknownLinuxGnu
                | Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        );
        let source = format!(
            "void f(int c, const void *p, int (*function)(int)) {{ _Static_assert(_Generic(c ? function : p, const void *: 1, void *: 0) == {}, \"qualifiers\"); }}",
            u8::from(gnu)
        );
        analyze(&source, target).unwrap();
        let source =
            "void f(int c, const void *p, int (*function)(int)) { void *q = c ? p : function; }";
        assert_eq!(analyze(source, target).is_ok(), !gnu);
    }
}

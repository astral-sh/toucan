use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::{analyze, evaluate_integer};
use toucan_target::Target;

const VALID: &[&str] = &[
    "int f(int n, ...) { __builtin_va_list a; __builtin_va_start(a, n); int x = __builtin_va_arg(a, int); __builtin_va_end(a); return x; }",
    "void f(int n, ...) { __builtin_va_list a, b; __builtin_va_start(a, n); __builtin_va_copy(b, a); __builtin_va_end(a); __builtin_va_end(b); }",
    "int f(__builtin_va_list a) { return __builtin_va_arg(a, int); }",
    "double f(__builtin_va_list a) { return __builtin_va_arg(a, double); }",
    "struct S { int x; }; struct S f(__builtin_va_list a) { return __builtin_va_arg(a, struct S); }",
    "int *f(__builtin_va_list a) { return __builtin_va_arg(a, int *); }",
    "void f(__builtin_va_list *a) { __builtin_va_end(*a); }",
    "void f(int different, ...); void f(int actual, ...) { __builtin_va_list a; __builtin_va_start(a, actual); __builtin_va_end(a); }",
    "long f(int x, long y) { return __builtin_expect(x, y); }",
    "long f(double x) { return __builtin_expect(x, 0); }",
    "int f(void) { __builtin_unreachable(); }",
    "void f(void) { __builtin_trap(); }",
    "enum { A = __builtin_expect(17, 0), B = __builtin_expect(-2.5, 1) }; _Static_assert(A == 17 && B == -2, \"long conversion\");",
    "int x[__builtin_expect(3, 1)]; _Static_assert(sizeof(x) == 3 * sizeof(int), \"constant bound\");",
    "_Static_assert(_Generic(__builtin_expect(1, 1), long: 1, default: 0), \"result type\");",
    "void f(__builtin_va_list a) { _Static_assert(_Generic(__builtin_va_arg(a, double), double: 1, default: 0), \"argument type\"); }",
];

const INVALID: &[&str] = &[
    "void f(void) { __builtin_va_list a; __builtin_va_start(a, 0); }",
    "void f(int n) { __builtin_va_list a; __builtin_va_start(a, n); }",
    "void f(int n, ...) { int a; __builtin_va_start(a, n); }",
    "void f(int n, ...) { __builtin_va_list a; __builtin_va_start(a); }",
    "void f(__builtin_va_list a) { __builtin_va_end(a, a); }",
    "void f(__builtin_va_list a) { __builtin_va_copy(a); }",
    "int f(int a) { return __builtin_va_arg(a, int); }",
    "void f(__builtin_va_list a) { __builtin_va_arg(a, void); }",
    "struct S; void f(__builtin_va_list a) { __builtin_va_arg(a, struct S); }",
    "void f(__builtin_va_list a) { __builtin_va_arg(a, int(void)); }",
    "void f(__builtin_va_list a) { __builtin_va_arg(a, int) = 1; }",
    "long f(void) { return __builtin_expect(1); }",
    "long f(void) { return __builtin_expect(1, 2, 3); }",
    "long f(void) { return __builtin_expect((void)0, 1); }",
    "struct S { int x; }; long f(struct S s) { return __builtin_expect(s, 1); }",
    "void f(void) { __builtin_unreachable(1); }",
    "void f(void) { __builtin_trap(1); }",
];

#[test]
fn compiler_intrinsics_check_types_without_exporting_symbols() {
    for target in Target::ALL {
        for source in VALID {
            let unit = analyze(source, target)
                .unwrap_or_else(|error| panic!("{target}: {source}: {error}"));
            assert!(
                !unit
                    .declarations
                    .iter()
                    .any(|d| d.name.starts_with("__builtin_"))
            );
        }
        for source in INVALID {
            assert!(
                analyze(source, target).is_err(),
                "{target}: accepted {source}"
            );
        }
        let unit = analyze("", target).unwrap();
        let value = evaluate_integer(&unit, "__builtin_expect(0xffffffffUL, 0)").unwrap();
        assert_eq!(value.bits, target.long_width() as u8);
        assert!(value.signed);
        assert_eq!(
            value.signed_value(),
            if target.long_width() == 32 {
                -1
            } else {
                0xffff_ffff
            }
        );
        assert!(evaluate_integer(&unit, "__builtin_expect(1, missing)").is_err());
    }
}

#[test]
fn va_start_identifies_the_visible_definition_parameter() {
    for source in [
        "void f(int first, int last, ...) { __builtin_va_list a; __builtin_va_start(a, first); }",
        "void f(int n, ...) { __builtin_va_list a; { int n = 1; __builtin_va_start(a, n); } }",
        "void f(int n, ...) { __builtin_va_list a; __builtin_va_start(a, n + 0); }",
    ] {
        for target in Target::ALL {
            let error = analyze(source, target).unwrap_err();
            assert!(
                error.message.contains("last named parameter"),
                "{source}: {error}"
            );
        }
    }
    for source in [
        "void f(const __builtin_va_list a) { __builtin_va_end(a); }",
        "void f(const __builtin_va_list a) { __builtin_va_arg(a, int); }",
        "void f(__builtin_va_list a, const __builtin_va_list b) { __builtin_va_copy(a, b); }",
    ] {
        for target in Target::ALL {
            assert!(
                analyze(source, target).is_err(),
                "{target}: accepted {source}"
            );
        }
    }
    for target in [
        Target::Aarch64UnknownLinuxGnu,
        Target::Aarch64AppleDarwin,
        Target::X86_64PcWindowsMsvc,
    ] {
        let source = "__builtin_va_list g(void); void f(void) { __builtin_va_end(g()); }";
        assert!(
            analyze(source, target).is_err(),
            "{target}: accepted temporary va_list"
        );
    }
}

#[test]
fn ordinary_declarations_shadow_call_intrinsics() {
    let source = "void f(int (*__builtin_expect)(int)) { int x = __builtin_expect(1); }";
    analyze(source, Target::X86_64UnknownLinuxGnu).unwrap();
    let source = "void f(int __builtin_expect) { __builtin_expect(1, 2); }";
    assert!(analyze(source, Target::X86_64UnknownLinuxGnu).is_err());
}

fn compile(compiler: &str, target: Option<Target>, source: &str) -> std::process::Output {
    let directory = tempfile::tempdir().unwrap();
    let mut command = Command::new(compiler);
    if let Some(target) = target {
        command.arg(format!("--target={target}"));
    }
    let mut process = command
        .args([
            "-std=c11",
            "-pedantic-errors",
            "-Wno-unused-value",
            "-Werror=varargs",
            "-c",
            "-x",
            "c",
            "-",
            "-o",
        ])
        .arg(directory.path().join("probe.o"))
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(process.stdin.take().unwrap(), "{source}").unwrap();
    process.wait_with_output().unwrap()
}

#[test]
#[ignore = "requires GCC and Clang with cross targets; run with --include-ignored"]
fn intrinsics_match_native_gcc_and_five_clang_targets() {
    for (compiler, targets) in [
        ("gcc", vec![None]),
        ("clang", Target::ALL.iter().copied().map(Some).collect()),
    ] {
        for target in targets {
            for (cases, accepted) in [(VALID, true), (INVALID, false)] {
                for source in cases {
                    let output = compile(compiler, target, source);
                    assert_eq!(
                        toucan_test_support::compiler_acceptance(&output),
                        Ok(accepted),
                        "{compiler} {target:?}: {source}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
        }
    }
}

#[test]
fn va_start_uses_the_target_default_variadic_abi() {
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::X86_64AppleDarwin,
        Target::X86_64PcWindowsMsvc,
    ] {
        for attribute in ["sysv_abi", "ms_abi"] {
            let source = format!(
                "void __attribute__(({attribute})) f(int n, ...) {{ __builtin_va_list a; __builtin_va_start(a, n); __builtin_va_end(a); }}"
            );
            let default = (attribute == "ms_abi") == (target == Target::X86_64PcWindowsMsvc);
            let result = analyze(&source, target);
            assert_eq!(result.is_ok(), default, "{target}: {source}: {result:?}");
            if let Err(error) = result {
                assert!(error.message.contains("nondefault variadic ABI"), "{error}");
            }
        }
    }
}

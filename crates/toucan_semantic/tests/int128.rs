use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::{analyze, evaluate_integer};
use toucan_target::Target;

const VALID: &[&str] = &[
    "typedef int I __attribute__((mode(TI))); _Static_assert(sizeof(I) == sizeof(__int128), \"machine mode\");",
    "__int128 x; signed __int128 y; __int128 unsigned z;",
    "typedef __int128 I; typedef unsigned __int128 U; I f(I x) { return x + 1; }",
    "_Static_assert(sizeof(__int128) == 16, \"size\"); _Static_assert(_Alignof(unsigned __int128) == 16, \"alignment\");",
    "struct S { char a; __int128 b; }; _Static_assert(sizeof(struct S) == 32, \"record\");",
    "_Static_assert(((__int128)1 << 100) > (unsigned long long)-1, \"rank\");",
    "_Static_assert(_Generic((__int128)0 + (unsigned long long)0, __int128: 1, default: 0), \"conversion\");",
    "_Static_assert((unsigned __int128)-1 >> 127 == 1, \"unsigned\");",
    "void f(void) { __int128 a[2] = {1, 2}; __int128 *p = a; *p += 3; }",
    "void f(__int128 (*callback)(__int128));",
    "struct S { int x; }; struct S s = (struct S){}; __int128 x; _Static_assert(sizeof(x) == 16, \"inserted offsets\");",
    "typedef __int128 (*F)(void); F f; __int128 g(void) { return ((__int128 (__attribute__((unused)) *)(void))f)(); }",
    "const char text[] = \"__int128\"; int __int128_suffix; /* __int128 */",
];

const INVALID: &[&str] = &[
    "__int128 int x;",
    "__int128 long x;",
    "__int128 short x;",
    "__int128 float x;",
    "__int128 double x;",
    "__int128 char x;",
    "__int128 _Float16 x;",
    "__int128 __int128 x;",
    "__int128 signed unsigned x;",
    "typedef int __int128;",
    "struct __int128 { int a; };",
    "int f(void) { return __int128; }",
    "void f(void) { __int128 *p; long long *q; p = q; }",
];

#[test]
fn int128_types_use_integer_semantics_in_every_context() {
    for target in Target::ALL {
        for source in VALID {
            analyze(source, target).unwrap_or_else(|error| panic!("{target}: {source}: {error}"));
        }
        for source in INVALID {
            assert!(
                analyze(source, target).is_err(),
                "{target} accepted {source}"
            );
        }
        let unit = analyze("typedef __int128 I;", target).unwrap();
        let value = evaluate_integer(&unit, "((unsigned __int128)1 << 100) + 7").unwrap();
        assert_eq!(value.value, (1 << 100) + 7);
        assert_eq!(value.bits, 128);
        assert!(!value.signed);
        assert_eq!(evaluate_integer(&unit, "sizeof(I)").unwrap().value, 16);
        // The grammar adapter must never change genuine extended float types.
        assert!(
            analyze("unsigned _Float16 x;", target).is_err(),
            "accepted unsigned floating type"
        );
    }
    let source = "void f(void) {} __int128 x = unknown;";
    let error = analyze(source, Target::X86_64UnknownLinuxGnu).unwrap_err();
    assert_eq!(error.offset, source.find("unknown").unwrap());
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn int128_types_match_compiler_acceptance_on_all_targets() {
    for (compiler, target) in std::iter::once(("gcc", None)).chain(
        Target::ALL
            .into_iter()
            .map(|target| ("clang", Some(target))),
    ) {
        for (sources, accepted) in [(VALID, true), (INVALID, false)] {
            for source in sources {
                let mut command = Command::new(compiler);
                if let Some(target) = target {
                    command.arg(format!("--target={}", target.triple()));
                }
                let mut child = command
                    .args([
                        "-std=gnu11",
                        "-Werror=incompatible-pointer-types",
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
                    output.status.success(),
                    accepted,
                    "{compiler} {target:?}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

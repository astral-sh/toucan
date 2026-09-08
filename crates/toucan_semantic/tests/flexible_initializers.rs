use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::{analyze, evaluate_integer};
use toucan_target::Target;

const GNU: Target = Target::X86_64UnknownLinuxGnu;
const CLANG: Target = Target::X86_64AppleDarwin;

const VALID: &[&str] = &[
    "typedef int A[]; struct F {int n; A a;}; struct F f = {2, {1, 2}};",
    "#pragma pack(push, 1)\nstruct F {char n; int a[];}; struct F f = {2, {1, 2}};\n#pragma pack(pop)\n",
    "struct F {int n; int a[];}; struct F f = {2, {1, 2}};",
    "struct F {int n; int a[];}; struct F f = {2, 1, 2};",
    "struct F {int n; int a[];}; struct F f = {.a = {[3] = 1}};",
    "struct F {int n; int a[];}; struct F f = {.a = {[2 ... 5] = 1}};",
    "struct F {int n; int a[];}; static const struct F f = {.a = {1, 2}};",
    "struct F {int n; int a[];}; void g(void) {static struct F f = {2, {1, 2}};}",
    "struct F {int n; int a[];}; struct F f = {2, {}};",
    "struct F {int n; int a[];}; struct F f[1] = {{2, {}}};",
    "struct F {int n; int a[];}; struct O {struct F f;}; struct O o = {{2, {}}};",
    "struct F {int n; char a[];}; struct F f = {3, \"abc\"};",
    "struct F {int n; char a[];}; struct F f = {.a = \"abc\"};",
    "struct F {int n; unsigned short a[];}; struct F f = {2, u\"😀\"};",
    "struct E {int x, y;}; struct F {int n; struct E a[];}; struct F f = {2, {1, 2, 3, 4}};",
    "struct E {int x, y;}; struct F {int n; struct E a[];}; struct F f = {2, 1, 2, 3, 4};",
    "struct F {int n; int a[];}; struct F f = {.a = {[3] = 1}, .a = {1}};",
    "struct F {int n; int a[];}; struct F f = {.a = {1}, .a = {[3] = 1}};",
    "struct F {int n; int a[];}; struct F f = {.a = {[3] = 1}, .a = {}};",
];

const INVALID: &[&str] = &[
    "struct F {int n; int a[];}; void g(void) {struct F f = {2, {1, 2}};}",
    "struct F {int n; int a[];}; struct F f[1] = {{2, {1, 2}}};",
    "struct F {int n; int a[];}; struct O {struct F f;}; struct O o = {{2, {1}}};",
    "struct F {int n; int a[];}; struct F f = {.a = {{.x = 1}}};",
    "int g(void); struct F {int n; int a[];}; struct F f = {1, {g()}};",
    "struct F {int n; int a[];}; struct F f = {.a = {[-1] = 1}};",
    "struct F {int n; int a[];}; struct F f = {.a = {[3 ... 2] = 1}};",
    "struct F {int n; int a[];}; struct F f = {.a = \"abc\"};",
];

const GNU_DESIGNATORS: &[&str] = &[
    "struct F {int n; int a[];}; struct F f = {.a[3] = 1};",
    "struct F {int n; int a[];}; struct F f = {.a[3] = 1, .a[1] = 2};",
    "struct F {int n; int a[];}; struct F f = {.a = 1};",
];

const CLANG_EMPTY: &[&str] = &[
    "struct F {int n; int a[];}; void g(void) {struct F f = {0, {}};}",
    "struct F {int n; int a[];}; void g(void) {struct F f[1] = {{0, {}}};}",
    "struct F {int n; int a[];}; struct O {struct F f;}; void g(void) {struct O o = {{0, {}}};}",
    "struct F {int n; int a[];}; void g(void) {struct F *p = &(struct F){0, {}};}",
];

#[test]
fn flexible_initializers_follow_storage_and_nesting_constraints() {
    for target in [GNU, CLANG] {
        for source in VALID {
            analyze(source, target).unwrap_or_else(|error| panic!("{target}: {source}: {error}"));
        }
        for source in INVALID {
            assert!(analyze(source, target).is_err(), "{target}: {source}");
        }
        for source in GNU_DESIGNATORS {
            assert_eq!(
                analyze(source, target).is_ok(),
                target == GNU,
                "{target}: {source}"
            );
        }
        for source in CLANG_EMPTY {
            assert_eq!(
                analyze(source, target).is_ok(),
                target == CLANG,
                "{target}: {source}"
            );
        }
    }
    let compound = "struct F {int n; int a[];}; struct F *p = &(struct F){1, {2}};";
    assert!(
        analyze(compound, GNU)
            .unwrap_err()
            .message
            .contains("compound literal")
    );
}

fn extent(source: &str, target: Target) -> (u64, u64) {
    let unit =
        analyze(source, target).unwrap_or_else(|error| panic!("{target}: {source}: {error}"));
    let object = unit
        .declarations
        .iter()
        .find(|declaration| declaration.name == "f")
        .unwrap();
    let storage = object.flexible_array_storage.as_ref().unwrap();
    assert_eq!(
        evaluate_integer(&unit, "sizeof f").unwrap().value,
        u128::from(unit.layout(&object.ty).unwrap().size_bits / 8)
    );
    assert!(evaluate_integer(&unit, "sizeof f.a").is_err());
    (storage.elements, storage.size_bits / 8)
}

#[test]
fn allocation_extent_does_not_complete_the_declared_flexible_type() {
    for target in Target::ALL {
        let gnu = matches!(
            target,
            Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
        );
        assert_eq!(
            extent(
                "struct F {int n; int a[];}; struct F f = {2, {1, 2}};",
                target
            ),
            (2, 12)
        );
        assert_eq!(
            extent(
                "struct F {long long n; char a[];}; struct F f = {0, {1}};",
                target
            ),
            (1, 9)
        );
        assert_eq!(
            extent(
                "struct F {int n; char c; char a[];}; struct F f = {0, 0, \"ab\"};",
                target
            ),
            (3, if gnu { 11 } else { 8 })
        );
        assert_eq!(
            extent(
                "struct F {int n; int a[];}; struct F f = {.a = {[3] = 1}, .a = {1}};",
                target
            ),
            (1, 8)
        );
        assert_eq!(
            extent(
                "struct F {int n; int a[];}; struct F f = {.a = {[3] = 1}, .a = {}};",
                target
            ),
            if gnu { (4, 20) } else { (0, 4) }
        );
    }
    let unit = analyze("typedef int Tail[]; struct F {int n; Tail a;}; struct F short_tail = {1, {2}}, long_tail = {3, {4, 5, 6}};", GNU).unwrap();
    let short = unit
        .declarations
        .iter()
        .find(|declaration| declaration.name == "short_tail")
        .unwrap();
    let long = unit
        .declarations
        .iter()
        .find(|declaration| declaration.name == "long_tail")
        .unwrap();
    assert_eq!(short.ty, long.ty);
    assert_eq!(short.flexible_array_storage.as_ref().unwrap().elements, 1);
    assert_eq!(long.flexible_array_storage.as_ref().unwrap().elements, 3);
    assert!(matches!(
        unit.resolve(&unit.typedefs["Tail"]).unwrap().kind,
        toucan_semantic::TypeKind::Array { length: None, .. }
    ));
    assert_eq!(
        extent(
            "struct F {int n; unsigned char a[];}; struct F f = {.a = {[1099511627776] = 1}};",
            GNU
        ),
        (1099511627777, 1099511627781)
    );
    let source = "struct F {int n; int a[];}; struct F f = {.a = {[18446744073709551614ULL] = 1}};";
    assert!(
        analyze(source, GNU)
            .unwrap_err()
            .message
            .contains("overflows")
    );
}

fn c_compile(compiler: &str, source: &str, arguments: &[&str]) -> std::process::Output {
    let mut child = Command::new(compiler)
        .args([
            "-std=gnu11",
            "-Werror=incompatible-pointer-types",
            "-Werror=int-conversion",
            "-x",
            "c",
            "-",
        ])
        .args(arguments)
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
    child.wait_with_output().unwrap()
}

#[test]
#[ignore = "requires native GCC and Clang; run with --include-ignored"]
fn flexible_initializer_constraints_match_compilers() {
    for compiler in ["gcc", "clang"] {
        let version = Command::new(compiler).arg("--version").output().unwrap();
        assert!(version.status.success());
        let version = String::from_utf8(version.stdout).unwrap();
        let gnu = version.contains("Free Software Foundation");
        assert!(gnu || version.to_ascii_lowercase().contains("clang"));
        for (cases, accepted) in [
            (VALID, true),
            (INVALID, false),
            (GNU_DESIGNATORS, gnu),
            (CLANG_EMPTY, !gnu),
        ] {
            for source in cases {
                let output = c_compile(compiler, source, &["-fsyntax-only"]);
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
#[ignore = "requires GCC and Clang ELF assembly output; run with --include-ignored"]
fn flexible_allocations_match_c_symbol_sizes() {
    let sources = [
        "struct F {int n; int a[];}; struct F f = {2, {1, 2}};",
        "struct F {long long n; char a[];}; struct F f = {0, {1}};",
        "struct F {int n; char c; char a[];}; struct F f = {0, 0, \"ab\"};",
        "struct F {int n; int a[];}; struct F f = {.a = {[3] = 1}, .a = {1}};",
        "struct F {int n; int a[];}; struct F f = {.a = {[3] = 1}, .a = {}};",
    ];
    for compiler in ["gcc", "clang"] {
        let version = Command::new(compiler).arg("--version").output().unwrap();
        assert!(version.status.success());
        let gnu = String::from_utf8(version.stdout)
            .unwrap()
            .contains("Free Software Foundation");
        // GNU GCC's native ELF symbols are checked on Linux. Clang's ELF
        // backend checks symbol extents even when the test host uses Mach-O.
        if gnu && !cfg!(target_os = "linux") {
            continue;
        }
        let target = if gnu { GNU } else { CLANG };
        for source in sources {
            let arguments = if gnu {
                vec!["-S", "-o", "-"]
            } else {
                vec!["-target", "x86_64-unknown-linux-gnu", "-S", "-o", "-"]
            };
            let output = c_compile(compiler, source, &arguments);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let assembly = String::from_utf8(output.stdout).unwrap();
            let size: u64 = assembly
                .lines()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix(".size")
                        .and_then(|rest| rest.trim().strip_prefix("f,"))
                })
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            assert_eq!(extent(source, target).1, size, "{compiler}: {source}");
        }
    }
}

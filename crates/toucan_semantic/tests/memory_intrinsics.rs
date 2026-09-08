use toucan_semantic::{analyze, evaluate_integer};
use toucan_target::Target;

const VALID: &[&str] = &[
    "void *f(void *p, unsigned long n) { return __builtin_memset(p, 0, n); }",
    "void *f(void *p, const void *q, unsigned long n) { return __builtin_memcpy(p, q, n); }",
    "void *f(void *p, const void *q, unsigned long n) { return __builtin_memmove(p, q, n); }",
    "int f(const void *p, const void *q, unsigned long n) { return __builtin_memcmp(p, q, n); }",
    "void f(void) { char destination[4]; const char source[4] = {1,2,3,4}; __builtin_memcpy(destination, source, sizeof source); }",
    "void f(char *p, double byte, float n) { __builtin_memset(p, byte, n); }",
    "struct S { unsigned byte:3; }; void f(char *p, struct S s) { __builtin_memset(p, s.byte, s.byte); }",
    "void f(char *p) { _Static_assert(_Generic(__builtin_memset(p,0,1), void *:1, default:0), \"memset result\"); _Static_assert(_Generic(__builtin_memcmp(p,p,1), int:1, default:0), \"memcmp result\"); }",
    "void f(void *p) { sizeof(__builtin_memset(p, 0, 4)); }",
    "void f(int (*__builtin_memset)(int)) { int result = __builtin_memset(1); }",
    "void f(int (*__builtin_memcpy)(int)) { int result = __builtin_memcpy(1); }",
];

const INVALID: &[&str] = &[
    "void f(void *p) { __builtin_memset(p, 0); }",
    "void f(void *p) { __builtin_memcpy(p, p, 1, 2); }",
    "void f(void *p) { __builtin_memmove(p); }",
    "void f(void *p) { __builtin_memcmp(p, p); }",
    "void f(const void *p) { __builtin_memset(p, 0, 1); }",
    "void f(volatile void *p) { __builtin_memset(p, 0, 1); }",
    "void f(const void *p) { __builtin_memcpy(p, p, 1); }",
    "void f(const void *p) { __builtin_memmove(p, p, 1); }",
    "void f(void *p, volatile void *q) { __builtin_memcpy(p, q, 1); }",
    "void f(const void *p, volatile void *q) { __builtin_memcmp(p, q, 1); }",
    "void f(int p) { __builtin_memset(p, 0, 1); }",
    "void f(void *p, int q) { __builtin_memcpy(p, q, 1); }",
    "void f(void *p) { __builtin_memset(p, p, 1); }",
    "void f(void *p) { __builtin_memset(p, 0, p); }",
    "struct S { int x; }; void f(void *p, struct S n) { __builtin_memmove(p, p, n); }",
    "void f(void *p) { __builtin_memcmp(p, p, (void)0); }",
    "void f(void *p) { __builtin_memset(p, 0, 1) = p; }",
    "void f(int __builtin_memcpy) { __builtin_memcpy(0, 0, 0); }",
];

const MEMORY_EFFECTS: &str = r#"
    int main(void) {
        unsigned char source[4] = {1,2,3,4};
        unsigned char destination[8];
        if (__builtin_memset(destination, 0x1ff, sizeof destination) != destination) return 1;
        if (destination[0] != 255 || destination[7] != 255) return 2;
        if (__builtin_memcpy(destination, source, sizeof source) != destination) return 3;
        if (__builtin_memcmp(destination, source, sizeof source) != 0) return 4;
        if (__builtin_memmove(destination+1, destination, 4) != destination+1) return 5;
        if (destination[1] != 1 || destination[4] != 4 || destination[7] != 255) return 6;
        return 0;
    }
"#;

#[test]
fn memory_intrinsics_check_the_target_prototypes_and_shadowing() {
    for target in Target::ALL {
        for source in VALID.iter().copied().chain([MEMORY_EFFECTS]) {
            let unit = analyze(source, target).unwrap_or_else(|error| panic!("{source}: {error}"));
            assert!(
                !unit
                    .declarations
                    .iter()
                    .any(|declaration| declaration.name.starts_with("__builtin_mem"))
            );
        }
        for source in INVALID {
            assert!(
                analyze(source, target).is_err(),
                "{target}: accepted {source}"
            );
        }
        let unit = analyze("", target).unwrap();
        assert_eq!(
            evaluate_integer(&unit, "sizeof(__builtin_memcpy(0,0,0))")
                .unwrap()
                .value,
            8
        );
    }
}

#[test]
#[ignore = "requires GCC and Clang with cross targets; run with --include-ignored"]
fn memory_intrinsics_match_compiler_constraints_and_runtime_behavior() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("memory.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for (compiler, targets) in [
        (gcc.as_str(), vec![None]),
        ("clang", Target::ALL.iter().copied().map(Some).collect()),
    ] {
        for target in targets {
            for (cases, accepted) in [(VALID, true), (INVALID, false)] {
                for source in cases {
                    std::fs::write(&input, format!("{source}\n")).unwrap();
                    let mut command = std::process::Command::new(compiler);
                    command.args([
                        "-std=c11",
                        "-pedantic-errors",
                        "-Werror",
                        "-Wno-unused-value",
                        "-fsyntax-only",
                    ]);
                    if let Some(target) = target {
                        command.arg(format!("--target={target}"));
                    }
                    let output = command.arg(&input).output().unwrap();
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
    std::fs::write(&input, MEMORY_EFFECTS).unwrap();
    for compiler in [gcc.as_str(), "clang"] {
        for optimization in ["-O0", "-O2"] {
            let executable = directory.path().join("memory");
            let output = std::process::Command::new(compiler)
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
                "{compiler}: memory builtin effects differ"
            );
        }
    }
}

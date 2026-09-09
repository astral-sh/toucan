use toucan_semantic::{analyze, evaluate_integer};
use toucan_target::Target;

const VALUES: &[(&str, u128, u8)] = &[
    ("__builtin_bswap16(0x1234)", 0x3412, 16),
    ("__builtin_bswap16(0x12345)", 0x4523, 16),
    ("__builtin_bswap16(-2)", 0xfeff, 16),
    ("__builtin_bswap16(1.9)", 0x100, 16),
    ("__builtin_bswap32(0x12345678UL)", 0x78563412, 32),
    ("__builtin_bswap32(0x123456789ULL)", 0x89674523, 32),
    (
        "__builtin_bswap64(0x0123456789abcdefULL)",
        0xefcdab8967452301,
        64,
    ),
    ("__builtin_bswap64(-2)", 0xfeffffffffffffff, 64),
    (
        "__builtin_bswap64(__builtin_bswap64(0x0123456789abcdefULL))",
        0x0123456789abcdef,
        64,
    ),
];

const VALID: &[&str] = &[
    "unsigned short f(short x) { return __builtin_bswap16(x); }",
    "unsigned int f(double x) { return __builtin_bswap32(x); }",
    "unsigned long long f(unsigned long long x) { return __builtin_bswap64(x); }",
    "void f(unsigned *x) { __builtin_bswap32(++*x); }",
    "void f(int (*__builtin_bswap16)(int)) { _Static_assert(_Generic(__builtin_bswap16(1), int:1, default:0), \"shadow\"); }",
    "_Static_assert(sizeof(__builtin_bswap16(1)) == 2 && sizeof(__builtin_bswap32(1)) == 4 && sizeof(__builtin_bswap64(1)) == 8, \"widths\");",
];

const INVALID: &[&str] = &[
    "void f(void) { __builtin_bswap16(); }",
    "void f(void) { __builtin_bswap32(1, 2); }",
    "void f(void) { __builtin_bswap64(); }",
    "void f(void *p) { __builtin_bswap16(p); }",
    "struct S { int x; }; void f(struct S x) { __builtin_bswap32(x); }",
    "void f(void) { __builtin_bswap64((void)0); }",
    "void f(void) { __builtin_bswap16(1) = 1; }",
    "void f(int __builtin_bswap32) { __builtin_bswap32(1); }",
];

fn result_types(target: Target) -> String {
    let wide = if matches!(
        target,
        Target::X86_64UnknownLinuxGnu
            | Target::X86_64UnknownLinuxMusl
            | Target::Aarch64UnknownLinuxGnu
            | Target::Aarch64UnknownLinuxMusl
    ) {
        "unsigned long"
    } else {
        "unsigned long long"
    };
    format!(
        "_Static_assert(_Generic(__builtin_bswap16(0), unsigned short:1, default:0), \"16-bit type\"); _Static_assert(_Generic(__builtin_bswap32(0), unsigned int:1, default:0), \"32-bit type\"); _Static_assert(_Generic(__builtin_bswap64(0), {wide}:1, default:0), \"64-bit type\");"
    )
}

#[test]
fn byte_swaps_convert_before_swapping_with_target_result_types() {
    for target in Target::ALL {
        let unit = analyze("", target).unwrap();
        for &(expression, expected, bits) in VALUES {
            let result = evaluate_integer(&unit, expression)
                .unwrap_or_else(|error| panic!("{expression}: {error}"));
            assert_eq!(result.value, expected, "{target}: {expression}");
            assert_eq!(result.bits, bits);
            assert!(!result.signed);
            analyze(
                &format!("_Static_assert({expression} == {expected}ULL, \"value\");"),
                target,
            )
            .unwrap();
        }
        analyze(&result_types(target), target).unwrap();
        for source in VALID {
            analyze(source, target).unwrap();
        }
        for source in INVALID {
            assert!(analyze(source, target).is_err(), "{target}: {source}");
        }
        let unit = analyze("int runtime(void);", target).unwrap();
        assert!(evaluate_integer(&unit, "__builtin_bswap16(runtime())").is_err());
    }
}

#[test]
#[ignore = "requires GCC and Clang with cross targets; run with --include-ignored"]
fn byte_swap_values_and_types_match_native_gcc_and_clang_targets() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("byte-swaps.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let constants = VALUES
        .iter()
        .map(|(expression, value, _)| {
            format!("_Static_assert({expression} == {value}ULL, \"value\");\n")
        })
        .collect::<String>();
    for (compiler, targets) in [
        (gcc.as_str(), vec![None]),
        ("clang", Target::ALL.iter().copied().map(Some).collect()),
    ] {
        for target in targets {
            let types = target.map_or_else(|| "_Static_assert(_Generic(__builtin_bswap64(0), __UINT64_TYPE__:1, default:0), \"native64\");".to_owned(), result_types);
            for (source, accepted) in VALID
                .iter()
                .map(|s| ((*s).to_owned(), true))
                .chain(INVALID.iter().map(|s| ((*s).to_owned(), false)))
                .chain([(constants.clone() + &types, true)])
            {
                std::fs::write(&input, format!("{source}\n")).unwrap();
                let mut command = std::process::Command::new(compiler);
                command.args([
                    "-std=c11",
                    "-pedantic-errors",
                    "-Werror=int-conversion",
                    "-Wno-unused-value",
                    "-fsyntax-only",
                ]);
                if let Some(target) = target {
                    command.arg(format!("--target={target}"));
                }
                let output = command.arg(&input).output().unwrap();
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

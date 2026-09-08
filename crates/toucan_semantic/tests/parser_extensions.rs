use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::{analyze, evaluate_integer};
use toucan_target::Target;

const TARGET: Target = Target::X86_64UnknownLinuxGnu;
const VALID: &[&str] = &[
    "struct S { int x; }; struct S s = (struct S){};",
    "struct S { int x; }; struct S s = (struct S){ /* empty */ };",
    "struct E {}; struct E e = (struct E){};",
    "struct E {}; struct S { int x; struct E e; }; struct S s = {1, (struct E){}};",
    "int (*p)[2] = &(int[2]){};",
    "int a = sizeof((int[]){}); _Static_assert(sizeof((int[]) {}) == 0, \"inferred bound\");",
    "void f(void) {} int g(int x) { if (x) {} else {} while (x) {} for (;x;) {} return 1; }",
    "int f(void); int g(void) { return ((int (__attribute__((noinline)) *)(void))f)(); }",
    "int f(void); int g(void) { return ((int (__attribute__((noinline)) __attribute__((unused)) *)(void))f)(); }",
    "int (__attribute__((noinline)) *f)(void);",
    "int (__attribute__((stdcall)) *f)(void);",
    "int (__attribute__((ms_abi)) *f)(void);",
    "int (__attribute__((noinline)) * const *f)(void);",
    "void f(int (__attribute__((noinline)) *)(void));",
    "void f(int (__attribute__ (( __noinline__ )) *)(void));",
    "void f(int (__attribute__((deprecated(\"é ( )\"))) *)(void));",
];

const INVALID: &[&str] = &[
    "struct S; struct S *p = &(struct S){};",
    "int *p = &(int(void)){};",
    "struct S { const int x; }; void f(void) { (struct S){}.x = 1; }",
    "struct S { int x; }; int *p = &(struct S){};",
    "int f(void); void g(void) { ((int (__attribute__((noinline)) *)(void))f)(1); }",
    "int f(void); void g(void) { ((int (__attribute__((noinline)) *)(void))f)() = 1; }",
    "int f(void); int g(void) { return ((int (__attribute__((noinline)) *)(void))f)(; }",
    "int f(void) { return (1){}; }",
];

#[test]
fn empty_compound_literals_and_nested_pointer_attributes_are_checked() {
    for source in VALID {
        analyze(source, TARGET).unwrap_or_else(|error| panic!("{source}: {error}"));
    }
    for source in INVALID {
        assert!(analyze(source, TARGET).is_err(), "accepted {source}");
    }
    for attribute in [
        "vectorcall",
        "aligned(16)",
        "packed",
        "mode(DI)",
        "unrecognized",
    ] {
        let source = format!("int (__attribute__(({attribute})) *f)(void);");
        let error = analyze(&source, TARGET).unwrap_err();
        assert!(error.message.contains("unsupported"), "{source}: {error}");
        let expected = if matches!(attribute, "aligned(16)" | "packed" | "mode(DI)") {
            source.find("__attribute__").unwrap()
        } else {
            source.find(attribute).unwrap()
        };
        assert_eq!(error.offset, expected, "{source}: {error}");
    }
}

#[test]
fn inserted_tokens_preserve_diagnostics_literals_and_pack_events() {
    let source =
        "struct S { int x; }; struct S s = (struct S){};\nint f(void) {}\nint x = unknown;";
    let error = analyze(source, TARGET).unwrap_err();
    assert_eq!(error.offset, source.find("unknown").unwrap());
    let source = "struct S { int x; }; struct S s = (struct S){};\nint x = ;";
    let error = analyze(source, TARGET).unwrap_err();
    assert_eq!(error.offset, source.rfind(';').unwrap());
    assert!(error.message.contains("line 2 column 9"), "{error}");
    let source = r#"struct S { int x; }; struct S s = (struct S){}; enum { C = U'\U0001f600' };"#;
    let unit = analyze(source, TARGET).unwrap();
    assert_eq!(evaluate_integer(&unit, "C").unwrap().value, 128512);
    assert_eq!(
        evaluate_integer(&unit, "sizeof((struct S){})")
            .unwrap()
            .value,
        4
    );
    assert_eq!(
        evaluate_integer(&unit, "sizeof((int[2]){1, 2})")
            .unwrap()
            .value,
        8
    );
    for expression in [
        "0); int y = (int[]){1}",
        "sizeof((int[1]){1}) }",
        "(int[]){1",
    ] {
        assert!(evaluate_integer(&unit, expression).is_err(), "{expression}");
    }
    let expression = "sizeof((struct S){}) + unknown";
    let error = evaluate_integer(&unit, expression).unwrap_err();
    assert_eq!(error.offset, expression.find("unknown").unwrap());
    for target in Target::ALL {
        let source = r#"
            struct S { int x; }; struct S s = (struct S){};
            #pragma pack(push, 1)
            struct P { char a; int b; };
            void f(void) {}
            #pragma pack(pop)
            struct Q { char a; int b; };
            _Static_assert(sizeof(struct P) == 5, "packed");
            _Static_assert(sizeof(struct Q) == 8, "restored");
        "#;
        analyze(source, target).unwrap_or_else(|error| panic!("{target}: {error}"));
    }
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn parser_extensions_match_gnu_compilers() {
    for compiler in ["gcc", "clang"] {
        for (cases, accepted) in [(VALID, true), (INVALID, false)] {
            for source in cases {
                let mut process = Command::new(compiler)
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
                process
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(source.as_bytes())
                    .unwrap();
                let output = process.wait_with_output().unwrap();
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
fn source_map_storage_is_bounded_before_parsing() {
    let source = "f(){}".repeat(100_001);
    let error = analyze(&source, TARGET).unwrap_err();
    assert!(error.message.contains("100000-edit limit"), "{error}");
}

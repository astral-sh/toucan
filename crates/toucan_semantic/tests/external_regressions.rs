use toucan_semantic::{analyze, evaluate_integer};
use toucan_target::Target;

#[test]
fn enum_types_are_compatible_with_their_selected_integer_types() {
    for target in Target::ALL {
        let unsigned = if target.is_windows() {
            "int"
        } else {
            "unsigned int"
        };
        let source = format!(
            "enum E {{ ZERO }}; enum E value; {unsigned} *pointer = &value; enum E function(enum E); {unsigned} function({unsigned});"
        );
        analyze(&source, target).unwrap();
        analyze(
            "enum E { NEGATIVE = -1 }; enum E value; int *pointer = &value;",
            target,
        )
        .unwrap();
        assert!(
            analyze(
                "enum A { A }; enum B { B }; enum A value; enum B *pointer = &value;",
                target
            )
            .is_err()
        );
        assert!(
            analyze(
                "enum E { NEGATIVE = -1 }; enum E value; unsigned int *pointer = &value;",
                target
            )
            .is_err()
        );
        assert!(
            analyze(
                "enum E { ZERO }; const enum E value; int *pointer = &value;",
                target
            )
            .is_err()
        );
    }
    let unit = analyze("enum E { ZERO };", Target::X86_64UnknownLinuxGnu).unwrap();
    assert_eq!(
        evaluate_integer(&unit, "_Generic((enum E)0, unsigned int: 1, default: 0)")
            .unwrap()
            .value,
        1
    );
    assert!(evaluate_integer(&unit, "_Generic(0, enum E: 1, unsigned int: 2)").is_err());
}

#[test]
fn flat_initializers_and_arguments_have_independent_expression_budgets() {
    let elements = vec!["1 + 1"; 1024].join(",");
    let source = format!("int values[] = {{{elements}}};");
    let unit = analyze(&source, Target::X86_64UnknownLinuxGnu).unwrap();
    assert_eq!(
        evaluate_integer(&unit, "sizeof values").unwrap().value,
        4096
    );
    let source = format!("int f(); int g(void) {{ return f({elements}); }}");
    analyze(&source, Target::X86_64UnknownLinuxGnu).unwrap();
    // Commas in nested call arguments must not reset the enclosing expression's
    // depth budget and allow an arbitrarily long recursive binary-expression AST.
    let chain = vec!["f(1, 2)"; 1024].join("+");
    let error = analyze(
        &format!("int f(); int g(void) {{ return {chain}; }}"),
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap_err();
    assert!(error.message.contains("parser AstDepth limit"), "{error}");
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn enum_compatibility_and_flat_initializers_match_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let source = format!(
        "enum E {{ ZERO }}; enum E value; unsigned int *pointer = &value; enum E f(enum E); unsigned int f(unsigned int); int values[] = {{{}}};\n",
        vec!["1+1"; 1024].join(",")
    );
    for compiler in ["gcc", "clang"] {
        let mut child = Command::new(compiler)
            .args([
                "-std=c11",
                "-pedantic-errors",
                "-x",
                "c",
                "-",
                "-fsyntax-only",
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
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{compiler}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

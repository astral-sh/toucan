use toucan_semantic::analyze;
use toucan_target::Target;

const TARGET: Target = Target::X86_64UnknownLinuxGnu;

#[test]
fn unbraced_control_flow_is_limited_before_recursive_parsing() {
    let labels = (0..15_000).map(|i| format!("L{i}:")).collect::<String>();
    for body in [
        labels,
        "if (1) ".repeat(15_000),
        "while (1) ".repeat(15_000),
        "if (1); else ".repeat(15_000),
    ] {
        let error = analyze(&format!("void f(void) {{{body};}}"), TARGET).unwrap_err();
        assert!(error.message.contains("1024-token limit"), "{error}");
    }
}

#[test]
fn control_budget_ignores_literals_and_comments_and_resets_between_bodies() {
    let body = format!(
        "const char *text = \"{}\"; /* {} */ {} return 0;",
        "if else : ".repeat(1500),
        "if else : ".repeat(1500),
        "if (1) ; else ;".repeat(400)
    );
    analyze(
        &format!("int f(void) {{{body}}} int g(void) {{{body}}}"),
        TARGET,
    )
    .unwrap();
    // Colons in nested regions count toward the same budget. Resetting at each
    // closing brace would allow a long if/else chain to evade the guard.
    let body = "if (1) {} else ".repeat(1500);
    let error = analyze(&format!("void f(void) {{{body};}}"), TARGET).unwrap_err();
    assert!(error.message.contains("1024-token limit"), "{error}");
}

#[test]
fn label_budget_survives_nested_braces_and_for_header_semicolons() {
    let labels = (0..900).map(|i| format!("L{i}:")).collect::<String>();
    let more_labels = (900..1800).map(|i| format!("L{i}:")).collect::<String>();
    for body in [
        format!("{labels} {{ ; {more_labels}; }}"),
        format!("{labels} for (;;) {more_labels};"),
    ] {
        let error = analyze(&format!("void f(void) {{{body}}}"), TARGET).unwrap_err();
        assert!(error.message.contains("1024-token limit"), "{error}");
    }
    let error = analyze(&"label:".repeat(1500), TARGET).unwrap_err();
    assert!(error.message.contains("1024-token limit"), "{error}");
}

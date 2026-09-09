use toucan_semantic::analyze;
use toucan_target::Target;

const TARGET: Target = Target::X86_64UnknownLinuxGnu;

#[test]
fn unbraced_control_flow_is_stopped_by_recursive_parser_budget() {
    let labels = (0..15_000).map(|i| format!("L{i}:")).collect::<String>();
    for body in [
        labels,
        "if (1) ".repeat(15_000),
        "while (1) ".repeat(15_000),
        "if (1); else ".repeat(15_000),
    ] {
        let error = analyze(&format!("void f(void) {{{body};}}"), TARGET).unwrap_err();
        assert!(error.message.contains("parser RuleDepth limit"), "{error}");
    }
}

#[test]
fn large_flat_bodies_are_distinguished_from_deep_control_flow() {
    let body = format!(
        "const char *text = \"{}\"; /* {} */ {} return 0;",
        "if else : ".repeat(1500),
        "if else : ".repeat(1500),
        "if (1) ; else ;".repeat(1500)
    );
    analyze(
        &format!("int f(void) {{{body}}} int g(void) {{{body}}}"),
        TARGET,
    )
    .unwrap();
    // A deeply nested else chain still consumes recursive parser frames.
    let body = "if (1) {} else ".repeat(1500);
    let error = analyze(&format!("void f(void) {{{body};}}"), TARGET).unwrap_err();
    assert!(error.message.contains("parser RuleDepth limit"), "{error}");
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
        assert!(error.message.contains("parser RuleDepth limit"), "{error}");
    }
    let error = analyze(&"label:".repeat(1500), TARGET).unwrap_err();
    assert!(error.message.contains("C syntax error"), "{error}");
}

#[test]
fn flat_pointer_declarators_stop_before_building_unbounded_owned_types() {
    let source = format!("int {}pointer;", "*".repeat(100_000));
    for retain_code in [false, true] {
        let options = toucan_semantic::AnalysisOptions {
            retain_code,
            ..Default::default()
        };
        let error = toucan_semantic::analyze_with_options(&source, TARGET, &options).unwrap_err();
        assert!(
            error
                .message
                .contains("type nesting exceeds the 128-level limit"),
            "{error}"
        );
    }
}

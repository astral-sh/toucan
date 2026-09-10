extern crate toucan_parser;

use toucan_parser::ast::Expression;
use toucan_parser::driver::{parse_expression, parse_expression_with_limits, Config};
use toucan_parser::limits::{ParseLimits, ResourceKind};
use toucan_parser::span::Span;

#[test]
fn expression_entrypoint_requires_complete_input() {
    for (source, offset) in [
        ("", 0),
        ("(1", 2),
        ("1 2", 2),
        ("1; int injected;", 1),
        ("1); int injected=(2", 1),
        ("# 20 \"injected\"\n1", 0),
        ("1\n#define X 2\n", 2),
    ] {
        let error = parse_expression(&Config::with_gcc(), source.into(), |_| false).unwrap_err();
        assert_eq!(error.offset, offset, "{}", error);
        assert_eq!(error.source, source);
        assert!(error.resource.is_none());
    }
}

#[test]
fn inherited_typedefs_keep_original_spans_and_can_be_shadowed() {
    let config = Config::with_gcc();
    let source = "  (T)1 + sizeof(\"Unused\")  ";
    let parsed = parse_expression(&config, source.into(), |name| {
        assert_ne!(name, "Unused");
        name == "T"
    })
    .unwrap();
    assert_eq!(parsed.source, source);
    assert_eq!(parsed.expression.span, Span::span(2, source.len() - 2));
    let Expression::BinaryOperator(binary) = parsed.expression.node else {
        panic!("binary expression")
    };
    assert!(matches!(binary.node.lhs.node, Expression::Cast(_)));
    // Postfix increment requires the local identifier; `(T)++` cannot be a cast.
    parse_expression(&config, "({ int T; (T)++; })".into(), |name| name == "T").unwrap();
    // Each query starts with a fresh environment.
    assert!(parse_expression(&config, "(T)1".into(), |_| false).is_err());
}

#[test]
fn expression_limits_cover_typedef_lookup_and_parsing() {
    let config = Config::with_gcc();
    let source = "(T)1 + sizeof(T[3])";
    let parsed = parse_expression(&config, source.into(), |name| name == "T").unwrap();
    let stats = parsed.statistics;
    let exact = ParseLimits {
        max_input_bytes: source.len(),
        max_work: stats.work,
        max_backtracking_steps: stats.maximum_backtracking_steps,
        max_rule_depth: stats.maximum_rule_depth,
        max_ast_depth: stats.maximum_ast_depth,
        max_cache_bytes: stats.cache_bytes,
        max_metadata_entries: stats.maximum_metadata_entries,
    };
    assert_eq!(
        parse_expression_with_limits(&config, source.into(), |name| name == "T", exact)
            .unwrap()
            .expression,
        parsed.expression
    );
    for (kind, limits) in [
        (
            ResourceKind::InputBytes,
            ParseLimits {
                max_input_bytes: source.len() - 1,
                ..exact
            },
        ),
        (
            ResourceKind::Work,
            ParseLimits {
                max_work: stats.work - 1,
                ..exact
            },
        ),
        (
            ResourceKind::RuleDepth,
            ParseLimits {
                max_rule_depth: stats.maximum_rule_depth - 1,
                ..exact
            },
        ),
    ] {
        let error =
            parse_expression_with_limits(&config, source.into(), |name| name == "T", limits)
                .unwrap_err();
        assert_eq!(error.resource.unwrap().kind, kind);
        assert!(source.is_char_boundary(error.offset));
    }
    parse_expression_with_limits(
        &config,
        source.into(),
        |_| panic!("lookup after budget failure"),
        ParseLimits {
            max_work: 0,
            ..exact
        },
    )
    .unwrap_err();
}

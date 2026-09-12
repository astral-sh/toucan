extern crate toucan_parser;

use std::process::Command;
use toucan_parser::arena::Arena;
use toucan_parser::ast::{
    BinaryOperatorExpression, ConditionalExpression, Identifier, Initializer,
};
use toucan_parser::driver::{parse_expression, parse_preprocessed, Config};
use toucan_parser::print::Printer;
use toucan_parser::span::Span;
use toucan_parser::view::{ConstantView, ExpressionView};
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Syntax<'ast> {
    binary: usize,
    conditional: usize,
    lists: usize,
    names: Vec<&'ast str>,
}

impl<'ast> Visit<'ast> for Syntax<'ast> {
    fn visit_binary_operator_expression(
        &mut self,
        expression: &'ast BinaryOperatorExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.binary += 1;
        visit::visit_binary_operator_expression(self, expression, span, arena);
    }

    fn visit_conditional_expression(
        &mut self,
        expression: &'ast ConditionalExpression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.conditional += 1;
        visit::visit_conditional_expression(self, expression, span, arena);
    }

    fn visit_initializer(
        &mut self,
        initializer: &'ast Initializer,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.lists += usize::from(matches!(initializer, Initializer::List(_)));
        visit::visit_initializer(self, initializer, span, arena);
    }

    fn visit_identifier(&mut self, identifier: &'ast Identifier, _: &'ast Span, _: &'ast Arena) {
        self.names.push(&identifier.name);
    }
}

#[test]
fn default_parser_visits_nested_arena_syntax_and_can_retain_references() {
    let source =
        "int f(int a) { int x[2][2] = {{1, 2}, {3, 4}}; return (a + x[0][0]) * (a ? 5 : 6); }";
    let parsed = parse_preprocessed(&Config::with_gcc(), source.into()).unwrap();
    assert_eq!(parsed.source, source);
    let mut syntax = Syntax::default();
    parsed.ast().visit(&mut syntax);
    assert_eq!(syntax.binary, 4);
    assert_eq!(syntax.conditional, 1);
    assert_eq!(syntax.lists, 3);
    assert!(syntax.names.contains(&"x"));
    assert!(syntax.names.contains(&"a"));
}

#[test]
fn cloning_a_parse_keeps_every_arena_payload_after_the_original_is_dropped() {
    let source = "struct S { int (*f)(int); }; int g(void) { struct S x; return sizeof(typeof(x)) + ({ int y = 1; y; }); }";
    let parsed = parse_preprocessed(&Config::with_gcc(), source.into()).unwrap();
    let mut expected = String::new();
    let (unit, arena) = parsed.ast().as_raw();
    Printer::new(&mut expected).visit_translation_unit(unit, arena);
    let cloned = parsed.clone();
    drop(parsed);
    let mut actual = String::new();
    let (unit, arena) = cloned.ast().as_raw();
    Printer::new(&mut actual).visit_translation_unit(unit, arena);
    assert_eq!(actual, expected);
}

#[test]
fn arena_clone_and_drop_fit_a_small_caller_stack() {
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "arena_drop_worker", "--nocapture"])
        .env("TOUCAN_ARENA_DROP_WORKER", "1")
        .status()
        .unwrap();
    assert!(status.success(), "arena drop worker: {}", status);
}

#[test]
fn arena_drop_worker() {
    if std::env::var_os("TOUCAN_ARENA_DROP_WORKER").is_none() {
        return;
    }
    std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(|| {
            let config = Config::with_gcc();
            for source in [
                format!("int f(int x) {{ return {}x; }}", "x+".repeat(120)),
                format!(
                    "int f(void) {{ {}int x;{} }}",
                    "{".repeat(126),
                    "}".repeat(126)
                ),
                format!("int {}x{};", "(".repeat(63), ")".repeat(63)),
                format!(
                    "{}int x;{};",
                    "struct S {".to_owned() + &"struct {".repeat(62),
                    "} x;".repeat(62) + "}"
                ),
                format!("int x[1] = {}1{};", "{".repeat(100), "}".repeat(100)),
            ] {
                let parsed = parse_preprocessed(&config, source).unwrap();
                let copied = parsed.ast().to_owned();
                assert!(parsed.ast().structural_eq(copied.view()));
                drop(copied);
                drop(parsed.clone());
                drop(parsed);
            }
            let parsed = parse_expression(&config, "a,".repeat(10_000) + "a", |_| false).unwrap();
            drop(parsed);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn arena_records_and_work_have_independent_per_parse_limits() {
    use toucan_parser::driver::parse_expression_with_limits;
    use toucan_parser::limits::{ParseLimits, ResourceKind};

    let config = Config::with_gcc();
    let operands = 33;
    let source = "a,".repeat(operands - 1) + "a";
    let parsed = parse_expression(&config, source.clone(), |_| false).unwrap();
    let statistics = parsed.statistics;
    // Each operand contributes both a cached expression and an arena record.
    assert!(statistics.maximum_metadata_entries >= operands * 2);
    for max_metadata_entries in [0, 1, 4, operands + 1] {
        let error = parse_expression_with_limits(
            &config,
            source.clone(),
            |_| false,
            ParseLimits {
                max_metadata_entries,
                ..ParseLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.resource.unwrap().kind, ResourceKind::MetadataEntries);
    }
    for max_work in [
        0,
        1,
        statistics.work / 4,
        statistics.work / 2,
        statistics.work - 1,
    ] {
        let error = parse_expression_with_limits(
            &config,
            source.clone(),
            |_| false,
            ParseLimits {
                max_work,
                ..ParseLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
    }
    let replay = parse_expression_with_limits(
        &config,
        source,
        |_| false,
        ParseLimits {
            max_work: statistics.work,
            max_metadata_entries: statistics.maximum_metadata_entries,
            ..ParseLimits::default()
        },
    )
    .unwrap();
    assert_eq!(replay.statistics, statistics);
    assert!(replay.ast().structural_eq(parsed.ast()));
}

#[test]
fn child_views_resolve_links_without_an_arena_argument() {
    let parsed = parse_expression(&Config::with_gcc(), "1 + 2".into(), |_| false).unwrap();
    let ExpressionView::BinaryOperator(binary) = parsed.ast().node().kind() else {
        panic!("expected binary expression");
    };
    let lhs = binary.node().lhs();
    assert_eq!(lhs.span().start, 0);
    assert_eq!(lhs.span().end, 1);
    let ExpressionView::Constant(constant) = lhs.node().kind() else {
        panic!("expected constant");
    };
    let ConstantView::Integer(integer) = constant.node().kind() else {
        panic!("expected integer");
    };
    assert_eq!(integer.number().value(), "1");
}

#[test]
fn subtree_copy_owns_descendants_and_compares_contents_across_owners() {
    let config = Config::with_gcc();
    let parsed = parse_expression(&config, "1 + (2 * 3)".into(), |_| false).unwrap();
    let ExpressionView::BinaryOperator(binary) = parsed.ast().node().kind() else {
        panic!("expected binary expression");
    };
    let subtree = binary.node().rhs();
    let copied = subtree.to_owned();
    assert!(subtree.structural_eq(copied.view()));
    drop(parsed);
    let ExpressionView::BinaryOperator(product) = copied.view().node().kind() else {
        panic!("expected copied product");
    };
    let ExpressionView::Constant(constant) = product.node().rhs().node().kind() else {
        panic!("expected copied constant");
    };
    let ConstantView::Integer(integer) = constant.node().kind() else {
        panic!("expected copied integer");
    };
    assert_eq!(integer.number().value(), "3");

    let one = parse_expression(&config, "1".into(), |_| false).unwrap();
    let another_one = parse_expression(&config, "1".into(), |_| false).unwrap();
    let two = parse_expression(&config, "2".into(), |_| false).unwrap();
    assert!(one.ast().structural_eq(another_one.ast()));
    assert!(!one.ast().structural_eq(two.ast()));
}

#[test]
fn translation_unit_views_support_lists_and_owned_extraction() {
    let parsed = parse_preprocessed(&Config::with_gcc(), "int a; int b;".into()).unwrap();
    assert_eq!(parsed.ast().inner().len(), 2);
    assert_eq!(parsed.ast().inner().iter().count(), 2);
    assert!(parsed.ast().inner().get(2).is_none());
    let owned = parsed.into_ast();
    let copied = owned.clone();
    drop(owned);
    assert_eq!(copied.view().inner().len(), 2);
    assert_eq!(::std::mem::size_of::<toucan_parser::arena::Id<u8>>(), 4);
}

#[test]
fn text_views_copy_text_instead_of_cloning_the_borrowed_handle() {
    let parsed = parse_expression(&Config::with_gcc(), "name".into(), |_| false).unwrap();
    let ExpressionView::Identifier(identifier) = parsed.ast().node().kind() else {
        panic!("expected identifier");
    };
    let name = identifier.node().name();
    assert!(name.structural_eq(name));
    let owned: String = name.to_owned();
    drop(parsed);
    assert_eq!(owned, "name");
}

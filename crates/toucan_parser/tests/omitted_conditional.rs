extern crate toucan_parser;

use toucan_parser::ast::{CallExpression, ConditionalExpression};
use toucan_parser::driver::{parse_preprocessed, Config, Flavor, Standard};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Expressions {
    calls: usize,
    omitted: usize,
    ordinary: usize,
}

impl<'ast> Visit<'ast> for Expressions {
    fn visit_call_expression(&mut self, node: &'ast CallExpression, span: &'ast Span) {
        self.calls += 1;
        visit::visit_call_expression(self, node, span);
    }
    fn visit_conditional_expression(
        &mut self,
        node: &'ast ConditionalExpression,
        span: &'ast Span,
    ) {
        if node.then_expression.is_none() {
            self.omitted += 1;
            assert_eq!(
                node.nonzero_expression().span.start,
                node.condition.span.start
            );
        } else {
            self.ordinary += 1;
        }
        visit::visit_conditional_expression(self, node, span);
    }
}

#[test]
fn omitted_operands_keep_the_written_condition_once() {
    let source = "int f(void){return first() ?: second() ?: (third()? fourth(): fifth());}";
    for standard in [Standard::C90, Standard::C99, Standard::C11, Standard::C17] {
        for flavor in [
            Flavor::GnuC11,
            Flavor::ClangC11,
            Flavor::GnuC11WithClangExtensions,
        ] {
            let config = Config {
                standard,
                flavor,
                gnu_keywords: false,
                ..Config::default()
            };
            let parsed = parse_preprocessed(&config, source.into()).unwrap();
            let mut visitor = Expressions::default();
            visitor.visit_translation_unit(&parsed.unit);
            assert_eq!(
                (visitor.calls, visitor.omitted, visitor.ordinary),
                (5, 2, 1)
            );
        }
        let core = Config {
            standard,
            flavor: Flavor::StdC11,
            ..Config::default()
        };
        assert!(parse_preprocessed(&core, source.into()).is_err());
    }
}

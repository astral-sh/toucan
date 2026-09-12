extern crate toucan_parser;

use toucan_parser::arena::Arena;

use toucan_parser::ast::{AlignOf, AlignOfOperand, Expression, SizeOfTy, SizeOfVal};
use toucan_parser::driver::{parse_preprocessed, Config, Flavor};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Operands {
    expressions: Vec<Span>,
    types: usize,
}

impl<'ast> Visit<'ast> for Operands {
    fn visit_sizeofval(&mut self, value: &'ast SizeOfVal, span: &'ast Span, arena: &'ast Arena) {
        self.expressions.push(value.0.span);
        assert!(matches!(
            value.0.node,
            Expression::CompoundLiteral(_) | Expression::Member(_) | Expression::BinaryOperator(_)
        ));
        visit::visit_sizeofval(self, value, span, arena);
    }

    fn visit_sizeofty(&mut self, value: &'ast SizeOfTy, span: &'ast Span, arena: &'ast Arena) {
        self.types += 1;
        visit::visit_sizeofty(self, value, span, arena);
    }

    fn visit_alignof(&mut self, value: &'ast AlignOf, span: &'ast Span, arena: &'ast Arena) {
        match &value.operand {
            AlignOfOperand::Expression(expression) => self.expressions.push(expression.span),
            AlignOfOperand::TypeName(_) => self.types += 1,
        }
        visit::visit_alignof(self, value, span, arena);
    }
}

#[test]
fn sizeof_and_alignof_consume_compound_literals_and_their_postfix_operators() {
    for flavor in [Flavor::StdC11, Flavor::GnuC11, Flavor::ClangC11] {
        let config = Config {
            flavor,
            ..Config::default()
        };
        let queries: &[&str] = if flavor == Flavor::StdC11 {
            &["sizeof"]
        } else {
            &["sizeof", "__alignof__", "_Alignof"]
        };
        for query in queries {
            for operand in [
                "(int){1}",
                "(char[]){1, 2, 3}",
                "(struct S){0}.field",
                "(char[]){1, 2, 3}[0]",
                "(T){1}",
            ] {
                let source = format!(
                    "typedef int T; struct S {{ char field; }}; int f(void) {{ return {query} {operand} + {query}(T); }}"
                );
                let parsed = parse_preprocessed(&config, source.clone()).unwrap();
                let mut operands = Operands::default();
                parsed.ast().visit(&mut operands);
                assert_eq!(operands.types, 1, "{source}");
                assert_eq!(operands.expressions.len(), 1, "{source}");
                let span = operands.expressions[0];
                assert_eq!(&source[span.start..span.end], operand);
            }
        }
    }
}

#[test]
fn sizeof_does_not_accept_bare_cast_operands() {
    for operand in ["(int)1", "(int){", "(int){1", "(int){1}["] {
        let source = format!("int f(void) {{ return sizeof {operand}; }}");
        assert!(parse_preprocessed(&Config::default(), source).is_err());
    }
}

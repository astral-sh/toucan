extern crate toucan_parser;

use toucan_parser::arena::Arena;

use toucan_parser::ast::Expression;
use toucan_parser::driver::{parse_preprocessed, Config};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct SizeOfOperands(Vec<bool>);

impl<'ast> Visit<'ast> for SizeOfOperands {
    fn visit_expression(
        &mut self,
        expression: &'ast Expression,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        match expression {
            Expression::SizeOfTy(_) => self.0.push(true),
            Expression::SizeOfVal(_) => self.0.push(false),
            _ => {}
        }
        visit::visit_expression(self, expression, span, arena);
    }
}

#[test]
fn names_shadow_typedefs_after_trailing_attributes() {
    for config in [Config::with_gcc(), Config::with_clang()] {
        for source in [
            "typedef int T; void f(void) { int T __attribute__((aligned(sizeof(T *)))) = sizeof(T); }",
            "typedef int T; void f(void) { int T __attribute__((aligned(sizeof(T *)))), x = sizeof(T); }",
            "typedef int T; int f(int T __attribute__((aligned(sizeof(T *)))), int a[sizeof(T)]);",
            "typedef int T; int f(int T __attribute__((aligned(sizeof(T *))))) { return sizeof(T); }",
        ] {
            let parsed = parse_preprocessed(&config, source.into()).unwrap();
            let mut operands = SizeOfOperands::default();
            parsed.ast().visit(&mut operands);
            assert_eq!(operands.0, [true, false], "{}", source);
        }
    }
}

#[test]
fn typedefs_and_inferred_names_keep_their_existing_scope_boundaries() {
    for source in [
        "typedef int T; void f(void) { typedef long T __attribute__((aligned(sizeof(T *)))); }",
        "typedef int T; void f(void) { __auto_type T = sizeof(T *); }",
    ] {
        let parsed = parse_preprocessed(&Config::with_gcc(), source.into()).unwrap();
        let mut operands = SizeOfOperands::default();
        parsed.ast().visit(&mut operands);
        assert_eq!(operands.0, [true], "{}", source);
    }
    assert!(parse_preprocessed(
        &Config::with_gcc(),
        "typedef int T; void f(void) { int T __attribute__((aligned(sizeof(T *)))) = sizeof(T *); }".into(),
    ).is_err());
}

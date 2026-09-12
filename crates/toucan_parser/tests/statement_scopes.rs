extern crate toucan_parser;

use toucan_parser::arena::Arena;

use toucan_parser::ast::{SizeOfTy, SizeOfVal};
use toucan_parser::driver::{parse_preprocessed, Config, Flavor, Standard};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct SizeOfOperands(Vec<bool>);

impl<'ast> Visit<'ast> for SizeOfOperands {
    fn visit_sizeofty(&mut self, operand: &'ast SizeOfTy, span: &'ast Span, arena: &'ast Arena) {
        self.0.push(true);
        visit::visit_sizeofty(self, operand, span, arena);
    }

    fn visit_sizeofval(&mut self, operand: &'ast SizeOfVal, span: &'ast Span, arena: &'ast Arena) {
        self.0.push(false);
        visit::visit_sizeofval(self, operand, span, arena);
    }
}

fn config(standard: Standard) -> Config {
    Config {
        flavor: Flavor::StdC11,
        standard,
        ..Config::with_gcc()
    }
}

#[test]
fn unbraced_then_body_does_not_hide_typedefs_in_else_body() {
    let source = "typedef int T; void f(void) { if (1) (void)sizeof(enum {T}); else { T x; } }";
    for standard in [Standard::C99, Standard::C11, Standard::C17] {
        parse_preprocessed(&config(standard), source.into()).unwrap();
    }
    assert!(parse_preprocessed(&config(Standard::C90), source.into()).is_err());
}

#[test]
fn unbraced_do_body_does_not_hide_typedefs_in_condition() {
    let source = "typedef int T; void f(void) { do (void)sizeof(enum {T}); while (sizeof(T[1])); }";
    for standard in [Standard::C99, Standard::C11, Standard::C17] {
        let parsed = parse_preprocessed(&config(standard), source.into()).unwrap();
        let mut operands = SizeOfOperands::default();
        parsed.ast().visit(&mut operands);
        assert_eq!(operands.0, [true, true]);
    }
}

#[test]
fn control_statement_scopes_follow_the_selected_standard() {
    for statement in [
        "if (sizeof(enum {T})) (void)0;",
        "switch (sizeof(enum {T})) { default: break; }",
        "while (sizeof(enum {T})) (void)0;",
        "do (void)sizeof(enum {T}); while (0);",
        "for ((void)sizeof(enum {T});0;) (void)0;",
    ] {
        for standard in [Standard::C90, Standard::C99, Standard::C11, Standard::C17] {
            let source = format!("typedef int T; void f(void) {{ {statement} (void)sizeof(T); }}");
            let parsed = parse_preprocessed(&config(standard), source).unwrap();
            let mut operands = SizeOfOperands::default();
            parsed.ast().visit(&mut operands);
            assert_eq!(
                operands.0,
                [true, standard != Standard::C90],
                "{standard:?}: {statement}"
            );
        }
    }
}

#[test]
fn controlling_expression_names_remain_visible_inside_the_body() {
    for statement in [
        "if (sizeof(enum {T})) (void)sizeof(T); else (void)sizeof(T);",
        "while (sizeof(enum {T})) (void)sizeof(T);",
        "for ((void)sizeof(enum {T});0;) (void)sizeof(T);",
        "switch (sizeof(enum {T})) { default: (void)sizeof(T); }",
    ] {
        let source = format!("typedef int T; void f(void) {{ {statement} }}");
        let parsed = parse_preprocessed(&config(Standard::C11), source).unwrap();
        let mut operands = SizeOfOperands::default();
        parsed.ast().visit(&mut operands);
        assert!(operands.0[0]);
        assert!(operands.0[1..].iter().all(|is_type| !is_type));
    }
}

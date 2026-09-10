extern crate toucan_parser;

use toucan_parser::ast::{
    DeclarationSpecifier, Expression, ExternalDeclaration, Initializer, Statement, TypeSpecifier,
};
use toucan_parser::driver::{parse_preprocessed, Config};

#[test]
fn int128_and_empty_compound_literals_keep_the_written_syntax() {
    let source = "__int128 value = (unsigned __int128){}; void empty(void) {}";
    for config in [Config::with_gcc(), Config::with_clang()] {
        let parsed = parse_preprocessed(&config, source.into()).unwrap();
        assert_eq!(parsed.source, source);
        let ExternalDeclaration::Declaration(declaration) = &parsed.unit.0[0].node else {
            panic!("declaration");
        };
        let DeclarationSpecifier::TypeSpecifier(specifier) = &declaration.node.specifiers[0].node
        else {
            panic!("type specifier");
        };
        assert_eq!(specifier.node, TypeSpecifier::Int128);
        assert_eq!(
            &source[specifier.span.start..specifier.span.end],
            "__int128"
        );
        let Initializer::Expression(expression) = &declaration.node.declarators[0]
            .node
            .initializer
            .as_ref()
            .unwrap()
            .node
        else {
            panic!("initializer expression");
        };
        let Expression::CompoundLiteral(literal) = &expression.node else {
            panic!("compound literal");
        };
        assert!(literal.node.initializer_list.is_empty());
        assert_eq!(
            &source[expression.span.start..expression.span.end],
            "(unsigned __int128){}"
        );
        let ExternalDeclaration::FunctionDefinition(function) = &parsed.unit.0[1].node else {
            panic!("function definition");
        };
        assert!(
            matches!(&function.node.statement.node, Statement::Compound(items) if items.is_empty())
        );
    }
}

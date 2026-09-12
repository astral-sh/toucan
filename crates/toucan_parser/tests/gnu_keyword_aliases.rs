extern crate toucan_parser;

use toucan_parser::arena::Arena;

use toucan_parser::ast::TypeQualifier;
use toucan_parser::driver::{parse_preprocessed, Config, Flavor};
use toucan_parser::span::Span;
use toucan_parser::visit::Visit;

#[derive(Default)]
struct ConstQualifiers(usize);

impl<'ast> Visit<'ast> for ConstQualifiers {
    fn visit_type_qualifier(
        &mut self,
        qualifier: &'ast TypeQualifier,
        _: &'ast Span,
        _arena: &'ast Arena,
    ) {
        if *qualifier == TypeQualifier::Const {
            self.0 += 1;
        }
    }
}

#[test]
fn double_underscore_const_is_a_qualifier_in_declarations_and_types() {
    let source = "__const__ int value = 1; struct S { __const__ int field; }; int f(int *__const__ p) { return *(__const__ int *)p; }";
    for config in [Config::with_gcc(), Config::with_clang()] {
        let parsed = parse_preprocessed(&config, source.into()).unwrap();
        let mut qualifiers = ConstQualifiers::default();
        parsed.ast().visit(&mut qualifiers);
        assert_eq!(qualifiers.0, 4);
    }
}

#[test]
fn gnu_keyword_aliases_cannot_be_redeclared_as_identifiers() {
    for name in ["__const__", "__typeof__"] {
        for config in [Config::with_gcc(), Config::with_clang()] {
            for source in [
                format!("int {name} = 1;"),
                format!("enum E {{ {name} = 1 }};"),
                format!("int f(void) {{ goto {name}; {name}: return 0; }}"),
            ] {
                assert!(parse_preprocessed(&config, source).is_err());
            }
        }
        let config = Config {
            flavor: Flavor::StdC11,
            ..Config::default()
        };
        parse_preprocessed(&config, format!("int {name};")).unwrap();
    }
}

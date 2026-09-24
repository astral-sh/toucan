extern crate toucan_parser;

use toucan_parser::ast::{
    ArrayDeclarator, ArraySize, PointerQualifier, TypeQualifier, TypeSpecifier,
};
use toucan_parser::driver::{parse_preprocessed, Config, Flavor, Standard};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Atomics {
    pointer_qualifiers: Vec<Span>,
    array_qualifiers: Vec<(Span, bool)>,
    specifiers: Vec<Span>,
}

impl<'ast> Visit<'ast> for Atomics {
    fn visit_pointer_qualifier(&mut self, qualifier: &'ast PointerQualifier, span: &'ast Span) {
        if let PointerQualifier::TypeQualifier(qualifier) = qualifier {
            if qualifier.node == TypeQualifier::Atomic {
                self.pointer_qualifiers.push(qualifier.span);
            }
        }
        visit::visit_pointer_qualifier(self, qualifier, span);
    }

    fn visit_array_declarator(&mut self, array: &'ast ArrayDeclarator, span: &'ast Span) {
        for qualifier in &array.qualifiers {
            if qualifier.node == TypeQualifier::Atomic {
                self.array_qualifiers.push((
                    qualifier.span,
                    matches!(array.size, ArraySize::StaticExpression(_)),
                ));
            }
        }
        visit::visit_array_declarator(self, array, span);
    }

    fn visit_type_specifier(&mut self, specifier: &'ast TypeSpecifier, span: &'ast Span) {
        if let TypeSpecifier::Atomic(_) = specifier {
            self.specifiers.push(*span);
        }
        visit::visit_type_specifier(self, specifier, span);
    }
}

#[test]
fn atomic_qualifiers_before_parentheses_remain_in_the_declarator() {
    let source = concat!(
        "int * _Atomic (object);\n",
        "int * const _Atomic (array[3]);\n",
        "int (* _Atomic (callback))(int);\n",
        "void pointers(int * _Atomic (parameter));\n",
        "void arrays(int first[_Atomic (3)], ",
        "int second[static _Atomic (1 + 2)], int third[_Atomic static (3)]);\n",
        "_Atomic(int) scalar;\n",
        "struct S { _Atomic(int *) pointer; };\n",
    );
    for standard in [Standard::C11, Standard::C17] {
        for flavor in [Flavor::StdC11, Flavor::GnuC11, Flavor::ClangC11] {
            let config = Config {
                standard,
                flavor,
                ..Config::default()
            };
            let parsed = parse_preprocessed(&config, source.into()).unwrap();
            let mut atomics = Atomics::default();
            atomics.visit_translation_unit(&parsed.unit);
            let text = |span: Span| &source[span.start..span.end];
            assert_eq!(
                atomics
                    .pointer_qualifiers
                    .into_iter()
                    .map(text)
                    .collect::<Vec<_>>(),
                ["_Atomic"; 4]
            );
            assert_eq!(
                atomics
                    .array_qualifiers
                    .into_iter()
                    .map(|(span, is_static)| (text(span), is_static))
                    .collect::<Vec<_>>(),
                [("_Atomic", false), ("_Atomic", true), ("_Atomic", true)]
            );
            assert_eq!(
                atomics.specifiers.into_iter().map(text).collect::<Vec<_>>(),
                ["_Atomic(int)", "_Atomic(int *)"]
            );
            for invalid in ["int _Atomic (object);", "int * _Atomic (int);"] {
                assert!(
                    parse_preprocessed(&config, invalid.into()).is_err(),
                    "{}",
                    invalid
                );
            }
        }
    }
}

extern crate toucan_parser;

use toucan_parser::ast::TypeOf;
use toucan_parser::driver::{parse_preprocessed, Config, Flavor};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Operands(Vec<(bool, Span)>);

impl<'ast> Visit<'ast> for Operands {
    fn visit_type_of(&mut self, operand: &'ast TypeOf, span: &'ast Span) {
        self.0.push((matches!(operand, TypeOf::Type(_)), *span));
        visit::visit_type_of(self, operand, span);
    }
}

#[test]
fn visible_typedefs_select_types_and_shadowed_names_select_expressions() {
    let cases: &[(&str, &[bool])] = &[
        (
            "typedef long T; int (*f(int T))(int) { __typeof__(T) a; }",
            &[false],
        ),
        (
            "typedef long T; int f(int (*callback)(int T)) { __typeof__(T) a; }",
            &[true],
        ),
        (
            "typedef long T; int f(enum { T } p) { __typeof__(T) a; }",
            &[false],
        ),
        (
            "typedef long T; __typeof__(int (*)(int T)) callback; T value;",
            &[true],
        ),
        ("typedef int T; __typeof__(T) a;", &[true]),
        ("typedef int T; __typeof__(const T) a;", &[true]),
        ("typedef int T; __typeof__(T *) a;", &[true]),
        ("typedef int T; __typeof__(T[3]) a;", &[true]),
        ("typedef int T; __typeof__(T (void)) a;", &[true]),
        ("typedef int T; __typeof__(T (*)(void)) a;", &[true]),
        ("typedef int T; __typeof__((T)) a;", &[false]),
        ("typedef int T; __typeof__((T){1}) a;", &[false]),
        (
            "typedef int T; int f(int n) { typedef int A[n]; __typeof__(A) a; }",
            &[true],
        ),
        (
            "typedef long T; int f(int T) { __typeof__(T) a; }",
            &[false],
        ),
        (
            "typedef long T; int f(void) { int T; __typeof__(T) a; }",
            &[false],
        ),
        (
            "typedef long T; int f(void) { enum { T }; __typeof__(T) a; }",
            &[false],
        ),
        (
            "typedef long T; int f(void) { { int T; __typeof__(T) a; } __typeof__(T) b; }",
            &[false, true],
        ),
        (
            "typedef int T; __typeof__(__typeof__(T) *) a;",
            &[true, true],
        ),
    ];
    for flavor in [Flavor::GnuC11, Flavor::ClangC11] {
        let config = Config {
            flavor,
            ..Config::default()
        };
        for &(source, expected) in cases {
            let parsed = parse_preprocessed(&config, source.to_owned())
                .unwrap_or_else(|error| panic!("{}: {}", source, error));
            let mut operands = Operands::default();
            operands.visit_translation_unit(&parsed.unit);
            let actual: Vec<_> = operands.0.iter().map(|(is_type, _)| *is_type).collect();
            assert_eq!(actual, expected, "{source}");
            for (_, span) in operands.0 {
                assert!(span.start < span.end && span.end <= source.len());
                assert_eq!(&source[span.start - 1..span.start], "(");
                assert_eq!(&source[span.end..span.end + 1], ")");
            }
        }
    }
}

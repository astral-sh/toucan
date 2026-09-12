extern crate toucan_parser;

use toucan_parser::arena::Arena;

use toucan_parser::ast::TypeOf;
use toucan_parser::driver::{parse_preprocessed, Config, Flavor};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Operands(Vec<(bool, Span)>);

impl<'ast> Visit<'ast> for Operands {
    fn visit_type_of(&mut self, operand: &'ast TypeOf, span: &'ast Span, arena: &'ast Arena) {
        self.0.push((matches!(operand, TypeOf::Type(_)), *span));
        visit::visit_type_of(self, operand, span, arena);
    }
}

#[test]
fn visible_typedefs_select_types_and_shadowed_names_select_expressions() {
    let cases: &[(&str, &[bool])] = &[
        (
            "typedef int T; int (*f(a))(int T) int a; { typeof(T) x; }",
            &[true],
        ),
        (
            "typedef int T; int (*f(a))(int T); T value; typeof(T) after;",
            &[true],
        ),
        (
            "typedef int T; int (*f(a))(int T) int a; { typeof(T) x; } typeof(T) after;",
            &[true, true],
        ),
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
            operands.visit_translation_unit(&parsed.unit, &parsed.arena);
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

#[test]
fn inferred_names_enter_scope_after_initializer_and_leave_with_their_block() {
    for source in [
        "typedef int T; void f(void){__auto_type T=sizeof(typeof(T));typeof(T) value;} typeof(T) after;",
        "typedef int T; int f(int T); void g(void){__auto_type T=sizeof(typeof(T));typeof(T) value;} typeof(T) after;",
        "typedef int T; void g(void){for(__auto_type T=sizeof(typeof(T));T;) {typeof(T) value;}} typeof(T) after;",
    ] {
        for flavor in [Flavor::GnuC11,Flavor::ClangC11] {
            let parsed=parse_preprocessed(&Config{flavor,..Config::default()},source.to_owned()).unwrap();
            let mut operands=Operands::default();operands.visit_translation_unit(&parsed.unit, &parsed.arena);
            assert_eq!(operands.0.iter().map(|(ty,_)|*ty).collect::<Vec<_>>(),[true,false,true],"{source}");
        }
    }
}

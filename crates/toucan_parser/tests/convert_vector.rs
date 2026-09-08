extern crate toucan_parser;
use toucan_parser::ast::ConvertVectorExpression;
use toucan_parser::driver::{parse_preprocessed, Config, Flavor};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};
#[derive(Default)]
struct Conversions(Vec<Span>);
impl<'ast> Visit<'ast> for Conversions {
    fn visit_convert_vector_expression(
        &mut self,
        node: &'ast ConvertVectorExpression,
        span: &'ast Span,
    ) {
        self.0.push(node.type_name.span);
        visit::visit_convert_vector_expression(self, node, span);
    }
}
#[test]
fn destination_is_a_type_name_with_its_original_span() {
    for flavor in [Flavor::GnuC11, Flavor::ClangC11] {
        for spelling in [
            "const F",
            "float __attribute__((vector_size(16)))",
            "__typeof__((F){0})",
        ] {
            let source=format!("typedef int I __attribute__((vector_size(16)));typedef float F __attribute__((vector_size(16))); F f(I v){{return __builtin_convertvector(v,{spelling});}}");
            let parsed = parse_preprocessed(
                &Config {
                    flavor,
                    ..Config::default()
                },
                source.clone(),
            )
            .unwrap();
            let mut visitor = Conversions::default();
            visitor.visit_translation_unit(&parsed.unit);
            assert_eq!(visitor.0.len(), 1);
            let span = visitor.0[0];
            assert_eq!(&source[span.start..span.end], spelling);
        }
    }
}

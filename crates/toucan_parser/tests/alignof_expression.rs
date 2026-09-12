extern crate toucan_parser;

use toucan_parser::arena::Arena;

use toucan_parser::ast::{AlignOf, AlignOfKind, AlignOfOperand};
use toucan_parser::driver::{parse_preprocessed, parse_preprocessed_with_limits, Config, Flavor};
use toucan_parser::limits::ParseLimits;
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Queries(Vec<(AlignOfKind, bool, Span)>);
impl<'ast> Visit<'ast> for Queries {
    fn visit_alignof(&mut self, query: &'ast AlignOf, span: &'ast Span, arena: &'ast Arena) {
        self.0.push((
            query.kind,
            matches!(query.operand, AlignOfOperand::TypeName(_)),
            *span,
        ));
        visit::visit_alignof(self, query, span, arena);
    }
}

#[test]
fn query_spelling_and_typedef_ambiguity_preserve_the_operand_tree() {
    let source="typedef int T; int f(void){int x=_Alignof(T)+__alignof__ x;{int T;return __alignof__(T)+_Alignof(x);}}";
    for flavor in [Flavor::GnuC11, Flavor::ClangC11] {
        let config = Config {
            flavor,
            ..Config::default()
        };
        let parsed = parse_preprocessed(&config, source.into()).unwrap();
        let mut queries = Queries::default();
        queries.visit_translation_unit(&parsed.unit, &parsed.arena);
        assert_eq!(
            queries
                .0
                .iter()
                .map(|(kind, ty, _)| (*kind, *ty))
                .collect::<Vec<_>>(),
            [
                (AlignOfKind::C11, true),
                (AlignOfKind::Gnu, false),
                (AlignOfKind::Gnu, false),
                (AlignOfKind::C11, false)
            ]
        );
        assert!(queries
            .0
            .iter()
            .all(
                |(_, _, span)| source[span.start..span.end].contains("Alignof")
                    || source[span.start..span.end].contains("alignof")
            ));
        assert!(parse_preprocessed_with_limits(
            &config,
            source.into(),
            ParseLimits {
                max_work: parsed.statistics.work - 1,
                ..ParseLimits::default()
            }
        )
        .is_err());
    }
}

#[test]
fn nested_alignment_expressions_reach_parser_resource_limits() {
    let source = format!("int f(void){{return {}0;}}", "__alignof__ ".repeat(4096));
    let error = parse_preprocessed(&Config::default(), source).unwrap_err();
    assert!(error.to_string().contains("limit"), "{}", error);
}

extern crate toucan_parser;

use toucan_parser::arena::Arena;

use toucan_parser::ast::SpecifierQualifier;
use toucan_parser::driver::{parse_preprocessed, parse_preprocessed_with_limits, Config, Flavor};
use toucan_parser::limits::{ParseLimits, ResourceKind};
use toucan_parser::print::Printer;
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Alignments(Vec<Span>);
impl<'ast> Visit<'ast> for Alignments {
    fn visit_specifier_qualifier(
        &mut self,
        qualifier: &'ast SpecifierQualifier,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        if let SpecifierQualifier::Alignment(_) = qualifier {
            self.0.push(*span);
        }
        visit::visit_specifier_qualifier(self, qualifier, span, arena);
    }
}

#[test]
fn alignment_operands_remain_visible_to_visitors_printers_and_limits() {
    // Subject legality belongs to semantics: the parser also preserves the
    // type-name spelling so that the frontend can diagnose it precisely.
    let source = "struct S {_Alignas(16) int x; int _Alignas(double) y;}; int f(void){return sizeof(_Alignas(16) int);}";
    for flavor in [Flavor::StdC11, Flavor::GnuC11, Flavor::ClangC11] {
        let config = Config {
            flavor,
            ..Config::default()
        };
        let parsed = parse_preprocessed(&config, source.into()).unwrap();
        let mut alignments = Alignments::default();
        alignments.visit_translation_unit(&parsed.unit, &parsed.arena);
        assert_eq!(alignments.0.len(), 3);
        for span in alignments.0 {
            assert!(source[span.start..span.end].starts_with("_Alignas("));
        }
        let mut printed = String::new();
        Printer::new(&mut printed).visit_translation_unit(&parsed.unit, &parsed.arena);
        assert_eq!(printed.matches("AlignmentSpecifier").count(), 3);
        let error = parse_preprocessed_with_limits(
            &config,
            source.into(),
            ParseLimits {
                max_work: parsed.statistics.work - 1,
                ..ParseLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
    }
}

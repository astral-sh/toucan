extern crate toucan_parser;

use toucan_parser::ast::TypeSpecifier;
use toucan_parser::driver::{parse_preprocessed, parse_preprocessed_with_limits, Config};
use toucan_parser::limits::{ParseLimits, ResourceKind};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Widths(Vec<(u8, Span)>);

impl<'ast> Visit<'ast> for Widths {
    fn visit_type_specifier(&mut self, value: &'ast TypeSpecifier, span: &'ast Span) {
        if let TypeSpecifier::MsvcInteger(width) = value {
            self.0.push((*width, *span));
        }
        visit::visit_type_specifier(self, value, span);
    }
}

#[test]
fn microsoft_widths_preserve_spans_and_resource_limits() {
    let source = "typedef __int8 B; _int16 f(__int32 x){return (_int64)x;}";
    let config = Config {
        extensions_msvc: true,
        ..Config::with_clang()
    };
    let parsed = parse_preprocessed(&config, source.into()).unwrap();
    let mut widths = Widths::default();
    widths.visit_translation_unit(&parsed.unit);
    assert_eq!(
        widths
            .0
            .iter()
            .map(|(width, span)| (*width, &source[span.start..span.end]))
            .collect::<Vec<_>>(),
        [
            (8, "__int8"),
            (16, "_int16"),
            (32, "__int32"),
            (64, "_int64")
        ]
    );
    let limit = ParseLimits {
        max_work: parsed.statistics.work - 1,
        ..Default::default()
    };
    let error = parse_preprocessed_with_limits(&config, source.into(), limit).unwrap_err();
    assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
    assert_eq!(
        parse_preprocessed(&config, source.into())
            .unwrap()
            .statistics,
        parsed.statistics
    );
    assert!(parse_preprocessed(&Config::with_clang(), source.into()).is_err());
}

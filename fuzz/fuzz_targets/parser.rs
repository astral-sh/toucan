#![no_main]

use libfuzzer_sys::fuzz_target;
use toucan_parser::arena::Arena;
use toucan_parser::ast::*;
use toucan_parser::driver::{
    Config, Flavor, Standard, parse_preprocessed_with_limits, with_parser_stack,
};
use toucan_parser::limits::ParseLimits;
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

struct Spans<'a>(&'a str);

impl Spans<'_> {
    fn check(&self, span: &Span) {
        assert!(span.start <= span.end);
        assert!(span.end <= self.0.len());
        assert!(self.0.is_char_boundary(span.start));
        assert!(self.0.is_char_boundary(span.end));
    }
}

macro_rules! visit_spans {
    ($($method:ident: $ty:ty),* $(,)?) => {
        impl<'ast> Visit<'ast> for Spans<'_> {
            $(fn $method(&mut self, value: &'ast $ty, span: &'ast Span, arena: &'ast Arena) {
                self.check(span);
                visit::$method(self, value, span, arena);
            })*
        }
    };
}

visit_spans! {
    visit_identifier: Identifier,
    visit_expression: Expression,
    visit_declaration: Declaration,
    visit_declarator: Declarator,
    visit_statement: Statement,
    visit_type_name: TypeName,
    visit_initializer: Initializer,
    visit_static_assert: StaticAssert,
}

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > 16_384 {
        return;
    }
    let Ok(source) = std::str::from_utf8(bytes) else {
        return;
    };
    let selector = bytes
        .iter()
        .fold(0usize, |sum, &byte| sum.wrapping_add(usize::from(byte)));
    let config = Config {
        flavor: [
            Flavor::StdC11,
            Flavor::GnuC11,
            Flavor::ClangC11,
            Flavor::GnuC11WithClangExtensions,
        ][selector & 3],
        standard: [Standard::C90, Standard::C99, Standard::C11, Standard::C17][(selector >> 2) & 3],
        gnu_keywords: selector & 16 != 0,
        extensions_msvc: selector & 32 != 0,
        ..Config::default()
    };
    let limits = ParseLimits {
        max_input_bytes: 16_384,
        max_work: 10_000_000,
        max_cache_bytes: 1024 * 1024,
        ..ParseLimits::default()
    };
    with_parser_stack(
        || match parse_preprocessed_with_limits(&config, source.into(), limits) {
            Ok(parsed) => {
                Spans(source).visit_translation_unit(&parsed.unit, &parsed.arena);
                let exact = ParseLimits {
                    max_work: parsed.statistics.work,
                    ..limits
                };
                let repeated =
                    parse_preprocessed_with_limits(&config, source.into(), exact).unwrap();
                assert_eq!(parsed.unit, repeated.unit);
                assert_eq!(parsed.arena, repeated.arena);
                assert_eq!(parsed.statistics, repeated.statistics);
            }
            Err(error) => {
                assert!(source.is_char_boundary(error.offset));
                assert!(error.offset <= source.len());
                assert!(error.statistics.cache_bytes <= limits.max_cache_bytes);
                let _ = error.to_string();
            }
        },
    )
    .unwrap();
});

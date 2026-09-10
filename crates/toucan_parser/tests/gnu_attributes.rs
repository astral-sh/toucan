extern crate toucan_parser;

use toucan_parser::ast::Extension;
use toucan_parser::driver::{parse_preprocessed, Config};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Attributes(Vec<Span>);

impl<'ast> Visit<'ast> for Attributes {
    fn visit_extension(&mut self, extension: &'ast Extension, span: &'ast Span) {
        if let Extension::Attribute(_) = extension {
            self.0.push(*span);
        }
        visit::visit_extension(self, extension, span);
    }
}

#[test]
fn empty_attribute_entries_are_ignored_without_changing_written_attributes() {
    for config in [Config::with_gcc(), Config::with_clang()] {
        for (source, expected) in [
            ("int x __attribute__((,aligned(16)));", &["aligned(16)"][..]),
            (
                "int x __attribute__((aligned(16),,));",
                &["aligned(16)"][..],
            ),
            (
                "int x __attribute__((,,aligned(16),,unused,,));",
                &["aligned(16)", "unused"][..],
            ),
            (
                "struct __attribute__((,aligned(16),,)) S { int x; };",
                &["aligned(16)"][..],
            ),
            ("int f(int x __attribute__((,unused,,)));", &["unused"][..]),
            ("int x __attribute__((,));", &[][..]),
            ("int x __attribute__((,,,));", &[][..]),
        ] {
            let parsed = parse_preprocessed(&config, source.to_owned()).unwrap();
            let mut attributes = Attributes::default();
            attributes.visit_translation_unit(&parsed.unit);
            let written: Vec<_> = attributes
                .0
                .iter()
                .map(|span| &source[span.start..span.end])
                .collect();
            assert_eq!(written, expected, "{}", source);
        }
        for source in [
            "int x __attribute__((aligned(16) unused));",
            "int x __attribute__((aligned(,16)));",
        ] {
            assert!(parse_preprocessed(&config, source.to_owned()).is_err());
        }
    }
}

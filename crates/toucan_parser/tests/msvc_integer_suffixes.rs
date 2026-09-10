extern crate toucan_parser;

use toucan_parser::ast::{Integer, IntegerSize};
use toucan_parser::driver::{parse_preprocessed, Config, Flavor};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Literals(Vec<(Integer, Span)>);
impl<'ast> Visit<'ast> for Literals {
    fn visit_integer(&mut self, value: &'ast Integer, span: &'ast Span) {
        self.0.push((value.clone(), *span));
        visit::visit_integer(self, value, span);
    }
}

#[test]
fn microsoft_integer_suffixes_preserve_width_signedness_and_spans() {
    for width in [8, 16, 32, 64] {
        for prefix in ["", "u", "U"] {
            for marker in ["i", "I"] {
                let spelling = format!("0xff{prefix}{marker}{width}");
                let source = format!("int value = {spelling};");
                for flavor in [Flavor::StdC11, Flavor::GnuC11, Flavor::ClangC11] {
                    let config = Config {
                        flavor,
                        extensions_msvc: true,
                        ..Config::default()
                    };
                    let parsed = parse_preprocessed(&config, source.clone()).unwrap();
                    let mut literals = Literals::default();
                    literals.visit_translation_unit(&parsed.unit);
                    let (value, span) = &literals.0[0];
                    assert_eq!(literals.0.len(), 1);
                    assert_eq!(value.suffix.size, IntegerSize::Msvc(width));
                    assert_eq!(value.suffix.unsigned, !prefix.is_empty());
                    assert!(!value.suffix.imaginary);
                    assert_eq!(&source[span.start..span.end], spelling);
                    let config = Config {
                        extensions_msvc: false,
                        ..config
                    };
                    assert!(parse_preprocessed(&config, source.clone()).is_err());
                }
            }
        }
    }
}

#[test]
fn malformed_microsoft_integer_suffixes_are_rejected() {
    let config = Config {
        extensions_msvc: true,
        ..Config::with_clang()
    };
    for literal in [
        "1i64u", "1i8i", "1ui16u", "1i128", "1i7", "1lli64", "1.0i64",
    ] {
        assert!(parse_preprocessed(&config, format!("int value = {literal};")).is_err());
    }
}

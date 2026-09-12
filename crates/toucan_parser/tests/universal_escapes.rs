extern crate toucan_parser;

use toucan_parser::arena::Arena;

use toucan_parser::ast::Constant;
use toucan_parser::driver::{parse, parse_preprocessed, Config, Flavor, Standard};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Literals(Vec<(String, Span)>);

impl<'ast> Visit<'ast> for Literals {
    fn visit_constant(&mut self, constant: &'ast Constant, span: &'ast Span, arena: &'ast Arena) {
        if let Constant::Character(text) = constant {
            self.0.push((text.clone(), *span));
        }
        visit::visit_constant(self, constant, span, arena);
    }

    fn visit_string_literal(
        &mut self,
        strings: &'ast Vec<String>,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        self.0.push((strings.join(" "), *span));
        visit::visit_string_literal(self, strings, span, arena);
    }
}

#[test]
fn universal_escapes_preserve_literal_spellings_and_spans() {
    let source = r#"
        char *a = "\u00e9" "\U0001f426";
        void *b = L"\u00e9", *c = u"\u00e9", *d = U"\U0001f426", *e = u8"\u00e9";
        int f = '\u00e9', g = L'\u00e9', h = u'\u00e9', i = U'\U0001f426';
    "#;
    for flavor in [Flavor::StdC11, Flavor::GnuC11, Flavor::ClangC11] {
        let config = Config {
            flavor,
            ..Config::default()
        };
        let parsed = parse_preprocessed(&config, source.into()).unwrap();
        let mut literals = Literals::default();
        parsed.ast().visit(&mut literals);
        assert_eq!(literals.0.len(), 9);
        for (text, span) in literals.0 {
            assert_eq!(&source[span.start..span.end], text);
        }
    }
}

#[test]
fn malformed_universal_escapes_are_rejected() {
    for escape in [r"\u", r"\u123", r"\u123g", r"\U0001f42", r"\U0001f42g"] {
        for quote in ['\'', '"'] {
            let source = format!("int f(void) {{ return {quote}{escape}{quote}; }}");
            assert!(parse_preprocessed(&Config::default(), source).is_err());
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang"]
fn public_driver_accepts_preprocessed_universal_escapes() {
    let path = std::env::temp_dir().join(format!("toucan-ucn-{}.c", std::process::id()));
    std::fs::write(
        &path,
        concat!(
            r#"char text[] = "\u00e9\U0001f426"; int code = U'\u00e9';"#,
            "\n"
        ),
    )
    .unwrap();
    for config in [Config::with_gcc(), Config::with_clang()] {
        let status = std::process::Command::new(&config.cpp_command)
            .args(["-std=c11", "-pedantic-errors", "-fsyntax-only"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        parse(&config, &path).unwrap();
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn universal_escapes_require_c99_or_compiler_extensions() {
    let source = r#"char text[] = "\u00e9\U0001f426"; int code = L'\u00e9';"#;
    for standard in [Standard::C90, Standard::C99, Standard::C11, Standard::C17] {
        for flavor in [Flavor::StdC11, Flavor::GnuC11, Flavor::ClangC11] {
            let config = Config {
                standard,
                flavor,
                ..Config::default()
            };
            assert_eq!(
                parse_preprocessed(&config, source.into()).is_ok(),
                standard != Standard::C90 || flavor != Flavor::StdC11
            );
        }
    }
}

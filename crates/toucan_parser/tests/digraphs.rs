extern crate toucan_parser;

use toucan_parser::ast::{BinaryOperator, BinaryOperatorExpression, Statement};
use toucan_parser::driver::{parse, parse_preprocessed, Config, Flavor, Standard};
use toucan_parser::print::Printer;
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

const SOURCE: &str =
    "int values<:2:> = <%<:0:> = 1, 2%>;\nint first(void) <% return values<:0:>; %>\n";

#[test]
fn digraphs_parse_like_brackets_and_braces() {
    let ordinary = "int values[2] = {[0] = 1, 2};\nint first(void) { return values[0]; }\n";
    for flavor in [Flavor::StdC11, Flavor::GnuC11, Flavor::ClangC11] {
        for standard in [Standard::C99, Standard::C11, Standard::C17] {
            let config = Config {
                flavor,
                standard,
                ..Config::default()
            };
            let printed = [SOURCE, ordinary].map(|source| {
                let parsed = parse_preprocessed(&config, source.into()).unwrap();
                assert_eq!(parsed.source, source);
                let mut printed = String::new();
                Printer::new(&mut printed).visit_translation_unit(&parsed.unit);
                printed
            });
            assert_eq!(printed[0], printed[1]);
        }
    }
}

#[test]
fn digraph_spans_retain_the_written_delimiters() {
    #[derive(Default)]
    struct Spans {
        index: Option<Span>,
        index_operator: Option<Span>,
        compound: Option<Span>,
    }
    impl<'ast> Visit<'ast> for Spans {
        fn visit_binary_operator_expression(
            &mut self,
            expression: &'ast BinaryOperatorExpression,
            span: &'ast Span,
        ) {
            if expression.operator.node == BinaryOperator::Index {
                self.index = Some(*span);
                self.index_operator = Some(expression.operator.span);
            }
            visit::visit_binary_operator_expression(self, expression, span);
        }

        fn visit_statement(&mut self, statement: &'ast Statement, span: &'ast Span) {
            if matches!(statement, Statement::Compound(_)) {
                self.compound = Some(*span);
            }
            visit::visit_statement(self, statement, span);
        }
    }
    let parsed = parse_preprocessed(&Config::with_gcc(), SOURCE.into()).unwrap();
    let mut spans = Spans::default();
    spans.visit_translation_unit(&parsed.unit);
    let index = spans.index.unwrap();
    let index_operator = spans.index_operator.unwrap();
    let compound = spans.compound.unwrap();
    assert_eq!(&SOURCE[index.start..index.end], "values<:0:>");
    assert_eq!(&SOURCE[index_operator.start..index_operator.end], "<:0:>");
    assert_eq!(
        &SOURCE[compound.start..compound.end],
        "<% return values<:0:>; %>"
    );
}

#[test]
fn digraphs_require_adjacent_characters_and_leave_literals_unchanged() {
    for source in [
        "int values< :2:>;",
        "int values<:2: >;",
        "int first(void) < % return 0; %>",
        "int first(void) <% return 0; % >",
    ] {
        assert!(parse_preprocessed(&Config::with_gcc(), source.into()).is_err());
    }
    let parsed = parse_preprocessed(
        &Config::with_gcc(),
        "const char *text = \"<: :> <% %>\";".into(),
    )
    .unwrap();
    let mut printed = String::new();
    Printer::new(&mut printed).visit_translation_unit(&parsed.unit);
    assert!(printed.contains("<: :> <% %>"));
}

#[test]
#[ignore = "requires native GCC/Clang; run with --include-ignored"]
fn native_preprocessors_preserve_digraphs_for_the_parser() {
    let mut gcc = Config::with_gcc();
    gcc.cpp_command = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let path =
        std::env::temp_dir().join(format!("toucan-parser-digraphs-{}.c", std::process::id()));
    std::fs::write(&path, SOURCE).unwrap();
    let results = [gcc, Config::with_clang()].map(|mut config| {
        config
            .cpp_options
            .extend(["-std=c11".into(), "-pedantic-errors".into()]);
        let checked = std::process::Command::new(&config.cpp_command)
            .args(["-std=c11", "-pedantic-errors", "-fsyntax-only"])
            .arg(&path)
            .output()
            .unwrap();
        let parsed = parse(&config, &path);
        (config.cpp_command, checked, parsed)
    });
    std::fs::remove_file(&path).unwrap();
    for (compiler, checked, parsed) in results {
        assert!(
            checked.status.success(),
            "{}: {}",
            compiler,
            String::from_utf8_lossy(&checked.stderr)
        );
        let parsed = parsed.unwrap_or_else(|error| panic!("{}: {}", compiler, error));
        assert!(parsed.source.contains("values<:2:>"));
        assert_eq!(parsed.unit.0.len(), 2);
    }
}

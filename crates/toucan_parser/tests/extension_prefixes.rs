extern crate toucan_parser;

use toucan_parser::ast::{Declaration, StaticAssert};
use toucan_parser::driver::{parse_preprocessed, parse_preprocessed_with_limits, Config, Flavor};
use toucan_parser::limits::{ParseLimits, ResourceKind};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Declarations(Vec<Span>);

impl<'ast> Visit<'ast> for Declarations {
    fn visit_declaration(&mut self, declaration: &'ast Declaration, span: &'ast Span) {
        self.0.push(*span);
        visit::visit_declaration(self, declaration, span);
    }

    fn visit_static_assert(&mut self, assertion: &'ast StaticAssert, span: &'ast Span) {
        self.0.push(*span);
        visit::visit_static_assert(self, assertion, span);
    }
}

#[test]
fn repeated_prefixes_preserve_declaration_dispatch_and_spans() {
    let source = "__extension__ __extension__ typedef int T;
        __extension__ __extension__ _Static_assert(sizeof(T) == sizeof(int), \"T\");
        void f(void) {
            __extension__ __extension__ T x = 0;
            __extension__ __extension__ _Static_assert(sizeof(T) == sizeof(int), \"T\");
            __extension__ __extension__ ++x;
            for (__extension__ __extension__ int y = 0; y < 1; y++) {}
            for (__extension__ __extension__ _Static_assert(1, \"T\");;) { break; }
        }";
    for config in [Config::with_gcc(), Config::with_clang()] {
        let parsed = parse_preprocessed(&config, source.into()).unwrap();
        let mut declarations = Declarations::default();
        declarations.visit_translation_unit(&parsed.unit);
        assert_eq!(declarations.0.len(), 6);
        for span in declarations.0 {
            assert!(source[span.start..span.end].starts_with("__extension__ __extension__"));
        }
    }
    let core = Config {
        flavor: Flavor::StdC11,
        ..Config::default()
    };
    assert!(parse_preprocessed(&core, source.into()).is_err());
}

#[test]
fn prefix_lookahead_obeys_work_limits_and_does_not_recurse() {
    let source = format!("void f(void) {{ {}int x; }}", "__extension__ ".repeat(1024));
    let config = Config::with_gcc();
    let parsed = parse_preprocessed(&config, source.clone()).unwrap();
    assert!(parsed.statistics.maximum_rule_depth < 32);
    for max_work in [
        source.len() as u64,
        parsed.statistics.work / 2,
        parsed.statistics.work - 1,
    ] {
        let error = parse_preprocessed_with_limits(
            &config,
            source.clone(),
            ParseLimits {
                max_work,
                ..ParseLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
    }
    let exact = parse_preprocessed_with_limits(
        &config,
        source,
        ParseLimits {
            max_work: parsed.statistics.work,
            ..ParseLimits::default()
        },
    )
    .unwrap();
    assert_eq!(parsed.unit, exact.unit);
}

#[test]
fn repeated_expression_prefixes_follow_typedef_shadowing() {
    let source = "typedef int T; void f(void) { { __extension__ __extension__ int T = 1; __extension__ __extension__ T++; } T after; }";
    for config in [Config::with_gcc(), Config::with_clang()] {
        parse_preprocessed(&config, source.into()).unwrap();
    }
}

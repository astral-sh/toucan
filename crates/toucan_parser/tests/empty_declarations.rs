extern crate toucan_parser;

use toucan_parser::ast::ExternalDeclaration;
use toucan_parser::driver::{parse_preprocessed, parse_preprocessed_with_limits, Config, Flavor};
use toucan_parser::limits::{ParseLimits, ResourceKind};

#[test]
fn empty_gnu_declarations_do_not_create_ast_nodes() {
    for config in [Config::with_gcc(), Config::with_clang()] {
        for source in [";", ";;;", "__extension__ ;", "; __extension__ ; ;"] {
            let parsed = parse_preprocessed(&config, source.into()).unwrap();
            assert!(parsed.ast().inner().is_empty(), "{}", source);
        }
        let source = ";; __extension__ ; int first; ;; __extension__ ; int second;;";
        let parsed = parse_preprocessed(&config, source.into()).unwrap();
        let (unit, _) = parsed.ast().as_raw();
        assert_eq!(unit.0.len(), 2);
        let written: Vec<_> = unit
            .0
            .iter()
            .map(|declaration| {
                assert!(matches!(
                    declaration.node,
                    ExternalDeclaration::Declaration(_)
                ));
                &source[declaration.span.start..declaration.span.end]
            })
            .collect();
        // Existing trailing semicolons remain part of the preceding node's span.
        assert_eq!(written, ["int first; ;;", "int second;;"]);
    }
}

#[test]
fn empty_declarations_require_gnu_extensions_and_a_semicolon() {
    let core = Config {
        flavor: Flavor::StdC11,
        ..Config::default()
    };
    for source in [";", "; int value;", "int value;;", "__extension__ ;"] {
        assert!(parse_preprocessed(&core, source.into()).is_err());
    }
    for source in ["__extension__", "__extension__ ,"] {
        assert!(parse_preprocessed(&Config::with_gcc(), source.into()).is_err());
    }
}

#[test]
fn ignored_declarations_still_obey_the_work_budget() {
    let source = ";".repeat(4096);
    let config = Config::with_gcc();
    let parsed = parse_preprocessed(&config, source.clone()).unwrap();
    assert!(parsed.ast().inner().is_empty());
    let limits = ParseLimits {
        max_work: parsed.statistics.work - 1,
        ..ParseLimits::default()
    };
    let error = parse_preprocessed_with_limits(&config, source.clone(), limits).unwrap_err();
    assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
    let limits = ParseLimits {
        max_work: parsed.statistics.work,
        ..ParseLimits::default()
    };
    let exact = parse_preprocessed_with_limits(&config, source, limits).unwrap();
    assert!(parsed.ast().structural_eq(exact.ast()));
}

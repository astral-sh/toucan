extern crate toucan_parser;

use toucan_parser::driver::{parse_preprocessed, Config, Flavor};

#[test]
fn dollar_identifiers_require_compiler_extensions_and_keep_typedef_scopes() {
    let source = "typedef int T$; T$ $value; int fn$(T$ value) { T$ local$ = value; { int T$ = 1; local$ += T$; } return local$; }";
    for flavor in [Flavor::StdC11, Flavor::GnuC11, Flavor::ClangC11] {
        let config = Config {
            flavor,
            ..Config::default()
        };
        assert_eq!(
            parse_preprocessed(&config, source.into()).is_ok(),
            flavor != Flavor::StdC11
        );
        let microsoft = Config {
            extensions_msvc: true,
            ..config
        };
        parse_preprocessed(&microsoft, source.into()).unwrap();
    }
}

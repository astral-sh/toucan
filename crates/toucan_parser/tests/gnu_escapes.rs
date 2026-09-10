extern crate toucan_parser;

use toucan_parser::driver::{parse_preprocessed, Config, Flavor};

#[test]
fn escape_character_spelling_requires_compiler_extensions() {
    for flavor in [Flavor::StdC11, Flavor::GnuC11, Flavor::ClangC11] {
        let config = Config {
            flavor,
            ..Config::default()
        };
        for escape in [r"\e", r"\E"] {
            for quote in ['\'', '"'] {
                let source = format!("int f(void) {{ return {quote}{escape}{quote}; }}");
                assert_eq!(
                    parse_preprocessed(&config, source).is_ok(),
                    flavor != Flavor::StdC11
                );
            }
        }
        for quote in ['\'', '"'] {
            let source = format!("int f(void) {{ return {quote}\\c{quote}; }}");
            assert!(parse_preprocessed(&config, source).is_err());
        }
    }
}

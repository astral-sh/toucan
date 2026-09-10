extern crate toucan_parser;

use toucan_parser::driver::{parse_preprocessed, parse_preprocessed_with_limits, Config};
use toucan_parser::limits::{ParseLimits, ResourceKind};

#[test]
fn malformed_tokens_are_rejected_without_panicking() {
    for expression in [
        "0x",
        "0b",
        "09",
        "1e+",
        "0x1p-",
        "0x1e+1",
        "1.0.0",
        "123name",
        "'\\x'",
        "''",
        "\"unfinished",
        "\"bad\\z\"",
        "a < < b",
        "a + = b",
        "a # b",
        "a\0b",
        "💥",
    ] {
        let source = format!("int f(void) {{ return {expression}; }}");
        let error = parse_preprocessed(&Config::with_gcc(), source.clone()).unwrap_err();
        assert!(source.is_char_boundary(error.offset), "{}", expression);
        assert!(error.offset <= source.len(), "{}", expression);
    }
}

#[test]
fn every_utf8_prefix_terminates_with_a_valid_diagnostic_or_tree() {
    let source = "typedef int T; int f(int x) { T a[3] = {[1] = 2}; if (x) return a[x] + sizeof(T); return \"é𝄞\"[0]; }";
    for end in (0..=source.len()).filter(|&end| source.is_char_boundary(end)) {
        let prefix = &source[..end];
        if let Err(error) = parse_preprocessed(&Config::with_gcc(), prefix.into()) {
            assert!(prefix.is_char_boundary(error.offset), "{:?}", prefix);
            assert!(error.offset <= prefix.len(), "{:?}", prefix);
        }
    }
}

#[test]
fn lexer_work_exhaustion_cannot_become_an_empty_successful_parse() {
    let source = "int value = 123;";
    for work in 0..source.len() as u64 {
        let error = parse_preprocessed_with_limits(
            &Config::with_gcc(),
            source.into(),
            ParseLimits {
                max_work: work,
                ..ParseLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
    }
}

#[test]
fn line_markers_do_not_turn_mid_expression_directives_into_trivia() {
    let config = Config::with_gcc();
    parse_preprocessed(
        &config,
        "# 9 \"a.h\"\nint value;\n# 2 \"b.h\"\nint next;".into(),
    )
    .unwrap();
    assert!(parse_preprocessed(&config, "int value = 1 # ignored\n + 2;".into()).is_err());
}

#[test]
fn extension_prefix_preserves_static_assertion_dispatch() {
    parse_preprocessed(
        &Config::with_gcc(),
        r#"
        __extension__ _Static_assert(1, "file");
        void f(void) {
            __extension__ _Static_assert(1, "block");
            for (__extension__ _Static_assert(1, "loop");; ) break;
        }
    "#
        .into(),
    )
    .unwrap();
}

#[test]
fn token_storage_and_scanning_respect_limits_before_allocating() {
    let config = Config::with_gcc();
    let empty = parse_preprocessed_with_limits(
        &config,
        String::new(),
        ParseLimits {
            max_work: 0,
            max_cache_bytes: 0,
            ..ParseLimits::default()
        },
    )
    .unwrap();
    assert_eq!(empty.statistics.cache_bytes, 0);
    for source in [";".repeat(100_000), "int value;".repeat(10_000)] {
        let error = parse_preprocessed_with_limits(
            &config,
            source,
            ParseLimits {
                max_cache_bytes: 4096,
                ..ParseLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.resource.unwrap().kind, ResourceKind::CacheBytes);
        assert!(error.statistics.cache_bytes <= 4096);
    }
    for source in [" ".repeat(100_000), format!("\"{}\"", "x".repeat(100_000))] {
        let error = parse_preprocessed_with_limits(
            &config,
            source,
            ParseLimits {
                max_work: 1,
                ..ParseLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
        assert_eq!(error.statistics.cache_bytes, 0);
        assert_eq!(error.offset, 0);
    }
}

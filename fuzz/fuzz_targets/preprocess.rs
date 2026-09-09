#![no_main]

use libfuzzer_sys::fuzz_target;
use std::sync::{Arc, LazyLock};

#[derive(Debug)]
struct Catalog;
impl toucan::FeatureQueryProvider for Catalog {
    fn query(&self, kind: toucan::FeatureQuery, _: Option<&str>, name: &str) -> u64 {
        // A small immutable test catalog exercises both query results. The
        // production frontend supplies its semantic catalog separately.
        match kind {
            toucan::FeatureQuery::CAttribute if name == "fallthrough" => 201910,
            _ => u64::from(matches!(
                name,
                "aligned" | "__aligned__" | "align" | "c_atomic" | "__builtin_bswap32"
            )),
        }
    }
}
static CATALOG: LazyLock<Arc<dyn toucan::FeatureQueryProvider>> =
    LazyLock::new(|| Arc::new(Catalog));

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > 16_384 {
        return;
    }
    let Ok(data) = std::str::from_utf8(bytes) else {
        return;
    };
    let selector = bytes
        .iter()
        .fold(0usize, |sum, byte| sum.wrapping_add(usize::from(*byte)));
    let config = toucan::PreprocessorConfig {
        allow_filesystem: false,
        record_file_origins: true,
        record_macro_definitions: selector & 4 != 0,
        documentation: match (selector >> 4) & 3 {
            0 => None,
            value => Some(toucan::DocumentationOptions {parse_all_comments: value >= 2}),
        },
        macro_redefinition_policy: if selector & 8 == 0 {
            toucan::MacroRedefinitionPolicy::Strict
        } else {
            toucan::MacroRedefinitionPolicy::RecordAndReplace
        },
        feature_queries: Some(toucan::FeatureQueries::new(
            if selector & 1 == 0 {
                toucan::QueryDialect::Gnu
            } else {
                toucan::QueryDialect::Clang
            },
            Arc::clone(&CATALOG),
        )),
        trigraphs: selector & 0x100 != 0,
        scope_punctuator: selector & 2 != 0,
        line_comments: [
            toucan::LineComments::Enabled,
            toucan::LineComments::GnuC90,
            toucan::LineComments::GnuC90Preprocessing,
            toucan::LineComments::ClangC90,
            toucan::LineComments::ClangC90Preprocessing,
        ][(selector >> 9) % 5],
        max_tokens: 4096,
        max_source_bytes: 65_536,
        max_include_depth: 8,
        max_expansion_depth: 32,
        ..Default::default()
    };
    // In-memory data is the only input; no filesystem include directories.
    let mut preprocessor = toucan::Preprocessor::new(config);
    if let Ok(output) = preprocessor.preprocess_str(std::path::Path::new("fuzz-input.h"), data) {
        let origins = output.file_origins().expect("capture requested");
        let mut previous_end = 0;
        for mapping in origins.mappings() {
            let range = mapping.generated();
            assert!(range.start >= previous_end && range.start < range.end);
            assert!(output.source.get(range.clone()).is_some());
            assert_eq!(origins.source_file(range.start), Some(mapping.path()));
            assert_eq!(origins.source_file(range.end - 1), Some(mapping.path()));
            for offset in [range.start, range.end - 1] {
                assert_eq!(
                    origins.source_is_system_include(offset),
                    Some(mapping.is_system_include())
                );
                assert_eq!(
                    origins.source_name(offset).map(std::path::Path::as_os_str),
                    Some(mapping.accessed_path().as_os_str())
                );
            }
            previous_end = range.end;
        }
        for name in output.macros.keys() {
            if let Some(location) = origins.macro_definition(name) {
                assert!(location.line > 0 && location.column > 0);
                assert!(origins.macro_definition_name(name).is_some());
            }
        }
        assert_eq!(output.macro_redefinitions().is_some(), selector & 8 != 0);
        if let Some(records) = output.macro_redefinitions() {
            assert!(records.len() <= 4096);
            for record in records {
                let location = record.location().expect("all definitions are source input");
                assert!(location.line > 0 && location.column > 0);
                assert_eq!(location.path.as_ref(), std::path::Path::new("fuzz-input.h"));
                assert_eq!(record.accessed_path(), Some(location.path.as_ref()));
                assert!(!record.name().is_empty());
            }
        }
        if let Some(docs) = output.documentation() {
            for (id, source) in docs.sources() {
                assert!(docs.source(id).is_some());
                assert_eq!(source.source_len(), data.len());
                assert_eq!(source.path(), std::path::Path::new("fuzz-input.h"));
                let mut end = 0;
                for comment in source.comments() {
                    assert!(comment.range().start >= end);
                    assert_eq!(data.get(comment.range().clone()), Some(comment.text()));
                    assert!(comment.following_barrier() >= comment.range().end);
                    assert!(comment.following_barrier() <= data.len());
                    assert!(comment.line() > 0 && comment.end_line() >= comment.line());
                    assert!(source.is_system_at(comment.range().start).is_some());
                    end = comment.range().end;
                }
            }
            let mut end = 0;
            for mapping in docs.mappings() {
                let range = mapping.generated();
                assert!(range.start >= end && range.start < range.end);
                assert!(output.source.get(range.clone()).is_some());
                let origin = docs.origin(mapping).expect("owned origin");
                assert_eq!(docs.resolve(range.start), Some(origin));
                for location in [origin.invocation(), origin.spelling()].into_iter().flatten() {
                    let source = docs.source(location.source()).expect("owned source");
                    assert!(location.offset() < source.source_len());
                    assert!(data.is_char_boundary(location.offset()));
                    assert!(location.line() > 0);
                }
                end = range.end;
            }
        }
        if let Some(definitions) = output.macro_definitions() {
            assert!(definitions.len() <= 4096);
            for definition in definitions {
                let location = definition.location();
                assert!(location.line > 0 && location.column > 0);
                assert_eq!(location.kind, toucan::OriginKind::Directive);
                assert_eq!(location.path.as_ref(), std::path::Path::new("fuzz-input.h"));
                assert_eq!(definition.accessed_path(), location.path.as_ref());
                assert!(!definition.name().is_empty());
            }
        }
    }
    let reset = preprocessor
        .preprocess_str(std::path::Path::new("reset.h"), "")
        .expect("empty source after reset");
    assert_eq!(reset.macro_redefinitions().is_some(), selector & 8 != 0);
    assert!(reset.macro_redefinitions().is_none_or(<[_]>::is_empty));
    assert!(reset.documentation().is_none());
});

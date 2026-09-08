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
});

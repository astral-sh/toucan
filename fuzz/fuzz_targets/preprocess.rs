#![no_main]

use libfuzzer_sys::fuzz_target;
use std::sync::{Arc, LazyLock};

#[derive(Debug)]
struct Catalog;
impl toucan::FeatureQueryProvider for Catalog {
    fn query(&self, _: toucan::FeatureQuery, _: Option<&str>, name: &str) -> u64 {
        // A small immutable test catalog exercises both query results. The
        // production frontend supplies its semantic catalog separately.
        u64::from(matches!(name, "aligned" | "__aligned__" | "__builtin_bswap32"))
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
    let selector = bytes.iter().fold(0usize, |sum, byte| sum.wrapping_add(usize::from(*byte)));
    let config = toucan::PreprocessorConfig {
        allow_filesystem: false,
        feature_queries: Some(toucan::FeatureQueries::new(
            if selector & 1 == 0 { toucan::QueryDialect::Gnu } else { toucan::QueryDialect::Clang },
            Arc::clone(&CATALOG),
        )),
        trigraphs: selector & 0x100 != 0,
        max_tokens: 4096,
        max_source_bytes: 65_536,
        max_include_depth: 8,
        max_expansion_depth: 32,
        ..Default::default()
    };
    // In-memory data is the only input; no filesystem include directories.
    let mut preprocessor = toucan::Preprocessor::new(config);
    let _ = preprocessor.preprocess_str(std::path::Path::new("fuzz-input.h"), data);
});

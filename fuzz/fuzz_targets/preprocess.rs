#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    if data.len() > 16_384 {
        return;
    }
    let mut config = toucan::PreprocessorConfig::default();
    config.allow_filesystem = false;
    config.max_tokens = 4096;
    config.max_source_bytes = 65_536;
    config.max_include_depth = 8;
    config.max_expansion_depth = 32;
    // In-memory data is the only input; no filesystem include directories.
    let mut preprocessor = toucan::Preprocessor::new(config);
    let _ = preprocessor.preprocess_str(std::path::Path::new("fuzz-input.h"), data);
});

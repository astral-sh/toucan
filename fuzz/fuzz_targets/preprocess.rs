#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > 16_384 {
        return;
    }
    let Ok(data) = std::str::from_utf8(bytes) else {
        return;
    };
    let config = toucan::PreprocessorConfig {
        allow_filesystem: false,
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

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > 8192 {
        return;
    }
    let Ok(data) = std::str::from_utf8(bytes) else {
        return;
    };
    let selector = bytes
        .iter()
        .fold(0usize, |sum, byte| sum.wrapping_add(usize::from(*byte)));
    let profile = toucan::CompilerProfile::ALL[selector % toucan::CompilerProfile::ALL.len()]
        .with_language_mode(if selector & 0x100 == 0 {
            toucan::LanguageMode::Gnu11
        } else {
            toucan::LanguageMode::C11
        });
    let mut config = toucan::Config::with_profile(profile);
    config.preprocessor.allow_filesystem = false;
    config.preprocessor.max_tokens = 4096;
    config.preprocessor.max_source_bytes = 65_536;
    config.preprocessor.max_include_depth = 8;
    config.preprocessor.max_expansion_depth = 32;
    if let Ok(compilation) =
        toucan::parse_source(std::path::Path::new("fuzz-input.h"), data, &config)
    {
        let _ = compilation.bindings(&toucan::BindingOptions::default());
    }
});

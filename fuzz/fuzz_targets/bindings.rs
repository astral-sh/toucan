#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    if data.len() > 8192 {
        return;
    }
    let mut config = toucan::Config::new(toucan::Target::X86_64UnknownLinuxGnu);
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

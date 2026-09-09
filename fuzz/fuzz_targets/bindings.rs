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
        .with_language_mode(
            [
                toucan::LanguageMode::Gnu11,
                toucan::LanguageMode::C11,
                toucan::LanguageMode::Gnu90,
                toucan::LanguageMode::C90,
                toucan::LanguageMode::Gnu99,
                toucan::LanguageMode::C99,
                toucan::LanguageMode::Gnu17,
                toucan::LanguageMode::C17,
            ][(selector >> 8) & 7],
        );
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
        // Preserve every input byte and the existing compiler/mode selector.
        // The next four bits vary trait requests independently of those modes.
        let derives = (selector >> 11) & 15;
        let _ = compilation.bindings(&toucan::BindingOptions {
            emit_function_definitions: selector & 1 != 0,
            exclude_inline_functions: selector & 2 != 0,
            enum_constant_style: if selector & 4 != 0 {
                toucan::EnumConstantStyle::Bindgen
            } else {
                toucan::EnumConstantStyle::Integer
            },
            prepend_enum_name: selector & 8 != 0,
            rustified_enums: true,
            derives: toucan::DeriveOptions {
                copy: derives & 1 != 0,
                debug: Some(derives & 2 != 0),
                default: derives & 4 != 0,
                eq: derives & 8 != 0,
                ..Default::default()
            },
            ..Default::default()
        });
    }
});

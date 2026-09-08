#![no_main]

use libfuzzer_sys::fuzz_target;

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
    let profile = toucan::CompilerProfile::ALL[selector % toucan::CompilerProfile::ALL.len()]
        .with_language_mode([
            toucan::LanguageMode::Gnu11,
            toucan::LanguageMode::C11,
            toucan::LanguageMode::Gnu90,
            toucan::LanguageMode::C90,
        ][(selector >> 8) & 3]);
    if let Ok(analysis) = toucan::semantic::analyze_with_profile(data, profile, &Default::default())
    {
        let unit = analysis.unit();
        for declaration in &unit.declarations {
            let _ = unit.layout(&declaration.ty);
        }
    }
});

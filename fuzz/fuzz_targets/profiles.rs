//! Profile selection and a query hook for the campaign runner.

use toucan::{CompilerProfile, LanguageMode};

const MODES: [LanguageMode; 8] = [
    LanguageMode::Gnu11,
    LanguageMode::C11,
    LanguageMode::Gnu90,
    LanguageMode::C90,
    LanguageMode::Gnu99,
    LanguageMode::C99,
    LanguageMode::Gnu17,
    LanguageMode::C17,
];

fn indices(bytes: &[u8]) -> (usize, usize) {
    let sum = bytes
        .iter()
        .fold(0usize, |sum, byte| sum.wrapping_add(usize::from(*byte)));
    (sum % CompilerProfile::ALL.len(), (sum >> 8) & 7)
}

pub fn select(bytes: &[u8]) -> CompilerProfile {
    let (profile, mode) = indices(bytes);
    CompilerProfile::ALL[profile].with_language_mode(MODES[mode])
}

pub fn initialize() {
    if std::env::args().any(|arg| arg == "--toucan-fuzz-profile") {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes).unwrap();
        let (profile, mode) = indices(&bytes);
        println!("{} {profile} {mode}", CompilerProfile::ALL.len());
        std::process::exit(0);
    }
}

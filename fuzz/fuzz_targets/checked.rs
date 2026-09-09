#![no_main]

mod checked_invariants;

use libfuzzer_sys::fuzz_target;
use toucan::semantic::{AnalysisOptions, Error, analyze, analyze_with_options};

fn retention_limit(error: &Error) -> bool {
    matches!(
        error.message.as_str(),
        "checked-code retention node limit exceeded"
            | "checked-code retention edge limit exceeded"
            | "checked-code retention payload byte limit exceeded"
            | "checked-code occurrence nesting limit exceeded"
            | "retained type nesting limit exceeded"
    )
}

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > 16_384 {
        return;
    }
    let Ok(data) = std::str::from_utf8(bytes) else {
        return;
    };
    let selector = data
        .bytes()
        .fold(0usize, |sum, byte| sum.wrapping_add(usize::from(byte)));
    let target = toucan::Target::ALL[selector % toucan::Target::ALL.len()];
    let options = AnalysisOptions {
        retain_code: true,
        ..AnalysisOptions::default()
    };
    match (
        analyze(data, target),
        analyze_with_options(data, target, &options),
    ) {
        (Ok(unit), Ok(analysis)) => {
            assert!(analysis.checked().is_some());
            assert!(
                format!("{unit:?}") == format!("{:?}", analysis.unit()),
                "declaration IR changed for {target:?}"
            );
            checked_invariants::check(&analysis, data);
            for declaration in &analysis.unit().declarations {
                let _ = analysis.unit().layout(&declaration.ty);
            }
        }
        (_, Err(error)) if retention_limit(&error) => {}
        (Err(plain), Err(retained)) => {
            assert_eq!(plain.offset, retained.offset);
            assert_eq!(plain.message, retained.message);
        }
        (plain, retained) => {
            panic!(
                "retention changed acceptance for {target:?}: plain={:?}; retained={:?}",
                plain.map(|_| "accepted"),
                retained.map(|_| "accepted")
            )
        }
    }
});

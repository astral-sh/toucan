#![no_main]

mod profiles;

mod checked_invariants;

use libfuzzer_sys::fuzz_target;
use toucan::semantic::{AnalysisOptions, Error, analyze_with_profile};

fn retention_limit(error: &Error) -> bool {
    matches!(
        error.message.as_str(),
        "checked-code retention node limit exceeded"
            | "checked-code retention edge limit exceeded"
            | "checked-code retention payload byte limit exceeded"
            | "checked-code occurrence nesting limit exceeded"
            | "retained type nesting limit exceeded"
            | "declaration-origin occurrence limit exceeded"
            | "declaration-origin source fragment limit exceeded"
            | "object-value occurrence limit exceeded"
            | "object-value metadata exceeds the 64 MiB limit"
            | "object-value type nesting exceeds the 128-level limit"
            | "object-type comparison reference limit exceeded"
            | "object-type comparison nesting limit exceeded"
            | "documentation declaration limit exceeded"
            | "parameter-type dependency reference limit exceeded"
            | "parameter-type dependency storage limit exceeded"
            | "parameter-type dependency nesting limit exceeded"
            | "parameter-type occurrence limit exceeded"
    )
}

fuzz_target!(init: profiles::initialize(), |bytes: &[u8]| {
    if bytes.len() > 16_384 {
        return;
    }
    let Ok(data) = std::str::from_utf8(bytes) else {
        return;
    };
    let profile = profiles::select(bytes);
    let options = AnalysisOptions {
        retain_code: true,
        retain_declaration_origins: true,
        retain_object_values: true,
        retain_documentation_origins: true,
        retain_parameter_type_dependencies: true,
        ..AnalysisOptions::default()
    };
    match (
        analyze_with_profile(data, profile, &AnalysisOptions::default())
            .map(|analysis| analysis.into_unit()),
        analyze_with_profile(data, profile, &options),
    ) {
        (Ok(unit), Ok(analysis)) => {
            assert!(analysis.checked().is_some());
            assert!(analysis.object_values().is_some());
            assert!(analysis.documentation_origins().is_some());
            assert!(analysis.parameter_type_dependencies().is_some());
            assert!(
                format!("{unit:?}") == format!("{:?}", analysis.unit()),
                "declaration IR changed for {profile:?}"
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
                "retention changed acceptance for {profile:?}: plain={:?}; retained={:?}",
                plain.map(|_| "accepted"),
                retained.map(|_| "accepted")
            )
        }
    }
});

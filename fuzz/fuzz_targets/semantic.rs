#![no_main]

mod profiles;

use libfuzzer_sys::fuzz_target;

fuzz_target!(init: profiles::initialize(), |bytes: &[u8]| {
    if bytes.len() > 16_384 {
        return;
    }
    let Ok(data) = std::str::from_utf8(bytes) else {
        return;
    };
    let profile = profiles::select(bytes);
    if let Ok(analysis) = toucan::semantic::analyze_with_profile(data, profile, &Default::default())
    {
        let unit = analysis.unit();
        for declaration in &unit.declarations {
            let _ = unit.layout(&declaration.ty);
        }
    }
});

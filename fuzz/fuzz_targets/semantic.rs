#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    if data.len() > 16_384 {
        return;
    }
    if let Ok(unit) = toucan::semantic::analyze(data, toucan::Target::X86_64UnknownLinuxGnu) {
        for declaration in &unit.declarations {
            let _ = unit.layout(&declaration.ty);
        }
    }
});

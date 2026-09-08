use std::collections::BTreeMap;
use toucan_bindings::{MacroType, MacroValue, Options, generate_with_macros};
use toucan_semantic::{IntegerValue, analyze};
use toucan_target::Target;

#[test]
fn malformed_boolean_metadata_cannot_be_hidden_by_an_integer_policy() {
    let unit = analyze("", Target::X86_64UnknownLinuxGnu).unwrap();
    for (bits, signed, value) in [(8, false, 2), (32, false, 1), (8, true, 1), (8, false, 256)] {
        let macros = BTreeMap::from([(
            "VALUE".into(),
            Some(MacroValue::Integer(IntegerValue {
                value,
                bits,
                signed,
                rank: 0,
            })),
        )]);
        for macro_type in [MacroType::C, MacroType::Unsigned] {
            let error = generate_with_macros(
                &unit,
                &Options {
                    macro_type,
                    ..Default::default()
                },
                &macros,
            )
            .unwrap_err();
            assert!(
                error.0.contains("invalid C _Bool macro metadata"),
                "{error}"
            );
        }
        assert!(
            generate_with_macros(
                &unit,
                &Options {
                    allowlist: vec!["OTHER".into()],
                    ..Default::default()
                },
                &macros
            )
            .is_ok()
        );
    }
}

#[test]
fn byte_constants_do_not_acquire_boolean_validity() {
    let unit = analyze("", Target::X86_64UnknownLinuxGnu).unwrap();
    let macros = BTreeMap::from([(
        "BYTE".into(),
        Some(MacroValue::Integer(IntegerValue {
            value: 1,
            bits: 8,
            signed: false,
            rank: 1,
        })),
    )]);
    let output = generate_with_macros(&unit, &Options::default(), &macros).unwrap();
    assert!(
        output
            .source
            .contains("pub const BYTE: ::core::primitive::u8 = 1;")
    );
}

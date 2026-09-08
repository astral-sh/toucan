use toucan_semantic::{IntegerValue, analyze_with_profile, evaluate_arithmetic, evaluate_integer};
use toucan_target::{CompilerProfile, LanguageMode};

#[test]
fn literal_queries_keep_the_unit_profile_and_validate_public_metadata() {
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            let analysis =
                analyze_with_profile("int object; enum { N = 3 };", profile, &Default::default())
                    .unwrap();
            assert_eq!(analysis.unit().profile().unwrap(), profile);
            if mode.is_c90() {
                let value = evaluate_integer(analysis.unit(), "9223372036854775808").unwrap();
                assert_eq!(value.bits, 64);
                assert!(!value.signed);
            }
            let mut unit = analysis.unit().clone();
            unit.declarations[0].noreturn = true;
            for expression in ["1", "1.0 + 2.0", "0 ? 3 : 4", "N", "N | 4", "N ?: 0"] {
                for error in [
                    evaluate_integer(&unit, expression).unwrap_err(),
                    evaluate_arithmetic(&unit, expression).unwrap_err(),
                ] {
                    assert!(
                        error
                            .message
                            .contains("noreturn metadata requires a function declaration")
                    );
                }
            }
            unit.declarations[0].noreturn = false;
            unit.typedefs
                .insert("not-an-identifier".into(), unit.declarations[0].ty.clone());
            assert!(
                evaluate_integer(&unit, "1")
                    .unwrap_err()
                    .message
                    .contains("invalid typedef identifier")
            );
        }
    }
}

#[test]
fn constant_queries_use_current_values_and_preserve_name_precedence() {
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            let analysis = analyze_with_profile(
                "typedef int Count; int object; enum { N = 7 }; int f(void) { enum { N = 99 }; return N; }",
                profile,
                &Default::default(),
            )
            .unwrap();
            assert_eq!(evaluate_integer(analysis.unit(), "N").unwrap().value, 7);
            let mut unit = analysis.unit().clone();
            for value in [
                IntegerValue {
                    value: u64::MAX as u128,
                    bits: 64,
                    signed: false,
                    rank: 5,
                },
                IntegerValue {
                    value: 1 << 127,
                    bits: 128,
                    signed: true,
                    rank: 6,
                },
                IntegerValue {
                    value: u128::MAX,
                    bits: 128,
                    signed: false,
                    rank: 6,
                },
            ] {
                // Public Unit values have precedence over ordinary names,
                // typedefs, and predefined functions in the existing evaluator.
                for name in ["N", "object", "Count", "__builtin_prefetch", "malloc"] {
                    unit.constants.insert(name.into(), value);
                    assert_eq!(evaluate_integer(&unit, name).unwrap(), value);
                    assert_eq!(
                        evaluate_integer(&unit, &format!("1 ? {name} : {name}")).unwrap(),
                        value
                    );
                }
            }
            unit.constants.insert(
                "unused_invalid".into(),
                IntegerValue {
                    value: 0,
                    bits: 0,
                    signed: true,
                    rank: 3,
                },
            );
            assert!(evaluate_integer(&unit, "N").is_err());
        }
    }
}

#[test]
fn literal_and_context_queries_do_not_share_local_types() {
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile(
            "typedef unsigned Count; enum { N = 7 };",
            profile,
            &Default::default(),
        )
        .unwrap();
        let unit = analysis.unit();
        assert_eq!(
            evaluate_integer(unit, "sizeof(enum Local { MEMBER = 1 })")
                .unwrap()
                .value,
            4
        );
        assert_eq!(evaluate_integer(unit, "1 + 2 * 3").unwrap().value, 7);
        assert!(evaluate_integer(unit, "sizeof(enum Local)").is_err());
        assert!(evaluate_integer(unit, "MEMBER").is_err());
        assert_eq!(
            evaluate_integer(unit, "(Count)N + sizeof(Count)")
                .unwrap()
                .value,
            11
        );
        assert_eq!(evaluate_integer(unit, "'['").unwrap().value, 91);
    }
}

#[test]
fn large_value_trees_keep_the_ordinary_evaluation_path() {
    let unit = analyze_with_profile(
        "enum { N = 1 };",
        CompilerProfile::ALL[0],
        &Default::default(),
    )
    .unwrap();
    for atom in ["1", "N"] {
        let mut expression = atom.to_owned();
        for _ in 0..12 {
            expression = format!("({expression}) + ({expression})");
        }
        assert_eq!(
            evaluate_integer(unit.unit(), &expression).unwrap().value,
            4096
        );
    }
}

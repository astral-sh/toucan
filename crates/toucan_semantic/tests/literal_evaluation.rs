use toucan_semantic::{analyze_with_profile, evaluate_arithmetic, evaluate_integer};
use toucan_target::{CompilerProfile, LanguageMode};

#[test]
fn literal_queries_keep_the_unit_profile_and_validate_public_metadata() {
    for profile in CompilerProfile::ALL {
        for mode in [LanguageMode::Gnu11, LanguageMode::C11] {
            let profile = profile.with_language_mode(mode);
            let analysis =
                analyze_with_profile("int object;", profile, &Default::default()).unwrap();
            assert_eq!(analysis.unit().profile().unwrap(), profile);
            let mut unit = analysis.unit().clone();
            unit.declarations[0].noreturn = true;
            for expression in ["1", "1.0 + 2.0", "0 ? 3 : 4"] {
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
fn large_literal_trees_keep_the_ordinary_evaluation_path() {
    let unit = analyze_with_profile("", CompilerProfile::ALL[0], &Default::default()).unwrap();
    let mut expression = "1".to_owned();
    for _ in 0..12 {
        expression = format!("({expression}) + ({expression})");
    }
    assert_eq!(
        evaluate_integer(unit.unit(), &expression).unwrap().value,
        4096
    );
}

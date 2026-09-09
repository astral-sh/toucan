use toucan_semantic::{
    AnalysisOptions, ArithmeticConstant, analyze_with_profile, evaluate_arithmetic,
    evaluate_integer,
};
use toucan_target::{CompilerProfile, LanguageMode, Target};

#[test]
fn omitted_conditionals_preserve_integer_and_floating_values() {
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            let analysis = analyze_with_profile("", profile, &Default::default()).unwrap();
            for (expression, value) in [
                ("7 ?: 9", 7),
                ("0 ?: 3", 3),
                ("-1 ?: 2u", 4294967295),
                ("4 ?: (1/0)", 4),
                ("sizeof(enum {E=3}) ?: E", 4),
            ] {
                assert_eq!(
                    evaluate_integer(analysis.unit(), expression).unwrap().value,
                    value,
                    "{profile:?}: {expression}"
                );
            }
            for (expression, value) in [
                ("1.5 ?: 2.5", 1.5f64),
                ("0.0 ?: 2.5", 2.5),
                ("1.0f ?: 2.0", 1.0),
                ("16777217.0f ?: 0.0", 16777216.0),
            ] {
                let ArithmeticConstant::Floating(actual) =
                    evaluate_arithmetic(analysis.unit(), expression).unwrap()
                else {
                    panic!("{profile:?}: {expression}")
                };
                assert_eq!(actual.to_bits(), u128::from(value.to_bits()));
            }
            let ArithmeticConstant::Complex(value) =
                evaluate_arithmetic(analysis.unit(), "1.0fi ?: 2.0").unwrap()
            else {
                panic!("expected complex conditional value")
            };
            assert_eq!(value.real().to_bits(), 0);
            assert_eq!(value.imaginary().to_bits(), u128::from(1.0f64.to_bits()));
            for expression in ["0 ?: (1/0)", "1 ?: missing"] {
                assert!(evaluate_integer(analysis.unit(), expression).is_err());
            }
        }
    }
}

#[test]
fn omitted_conditionals_match_native_admission_and_retained_results() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/omitted_conditional.json")).unwrap();
    let mut failures = Vec::new();
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            for case in cases.as_array().unwrap() {
                let source = case["source"].as_str().unwrap();
                let mut expected = case["expected"].as_bool().unwrap();
                if case["name"] == "gnu_bitfield" && profile.target().long_width() == 32 {
                    expected = false;
                }
                let plain = analyze_with_profile(source, profile, &Default::default());
                let retained = analyze_with_profile(
                    source,
                    profile,
                    &AnalysisOptions {
                        retain_code: true,
                        ..Default::default()
                    },
                );
                if plain.is_ok() != expected {
                    failures.push(format!("{profile:?} {}: {plain:?}", case["name"]));
                }
                match (&plain, &retained) {
                    (Ok(a), Ok(b)) => {
                        assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit()))
                    }
                    (Err(a), Err(b)) => assert_eq!((&a.message, a.offset), (&b.message, b.offset)),
                    _ => panic!("parity: {profile:?}: {source}: {plain:?} / {retained:?}"),
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn reused_condition_values_keep_one_load_and_their_result_conversions() {
    use toucan_semantic::checked::{Conversion, ExprKind, UseContext};
    for profile in CompilerProfile::ALL {
        let source = "unsigned f(volatile short*p,unsigned fallback){return *p ?: fallback;}int g(_Atomic(int)*p){return *p ?: 3;}int call(void);int h(void){return call() ?: 4;}";
        let analysis = analyze_with_profile(
            source,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let code = analysis.checked().unwrap();
        let mut count = 0;
        for (_, expression) in code.expressions() {
            let ExprKind::OmittedConditional {
                condition,
                then_value,
                else_value: _,
            } = expression.kind()
            else {
                continue;
            };
            count += 1;
            match count {
                1 => {
                    assert_eq!(condition.conversions().len(), 1);
                    assert_eq!(condition.conversions()[0].kind(), Conversion::Lvalue);
                    assert_eq!(
                        then_value
                            .conversions()
                            .iter()
                            .map(|step| step.kind())
                            .collect::<Vec<_>>(),
                        vec![Conversion::IntegerPromotion, Conversion::Conditional]
                    );
                }
                2 => {
                    assert_eq!(condition.conversions().len(), 1);
                    assert_eq!(condition.conversions()[0].kind(), Conversion::AtomicLoad);
                    assert!(then_value.conversions().is_empty());
                }
                3 => assert!(condition.conversions().is_empty()),
                _ => unreachable!(),
            }
            assert_eq!(condition.expression(), then_value.expression());
            assert_eq!(then_value.context(), UseContext::ReusedValue);
            assert!(then_value.conversions().iter().all(|c| !matches!(
                c.kind(),
                Conversion::Lvalue
                    | Conversion::AtomicLoad
                    | Conversion::ArrayDecay
                    | Conversion::FunctionDecay
            )));
        }
        assert_eq!(count, 3);
        assert_eq!(
            code.expressions()
                .filter(|(_, e)| matches!(e.kind(), ExprKind::Call { .. }))
                .count(),
            1
        );
    }
}

#[test]
fn static_pointer_conditions_preserve_weak_binding_and_target_width() {
    use toucan_target::Compiler;
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/conditional_addresses.json")).unwrap();
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            for case in cases.as_array().unwrap() {
                let source = case["source"].as_str().unwrap();
                let expected = case[if profile.compiler() == Compiler::Gnu {
                    "gnu"
                } else {
                    "clang"
                }]
                .as_bool()
                .unwrap()
                    && !((profile.target() == Target::I686UnknownLinuxGnu
                        || profile.target().is_armv7())
                        && source.contains("__int128"));
                let ordinary = analyze_with_profile(source, profile, &Default::default());
                let retained = analyze_with_profile(
                    source,
                    profile,
                    &AnalysisOptions {
                        retain_code: true,
                        ..Default::default()
                    },
                );
                assert_eq!(
                    ordinary.is_ok(),
                    expected,
                    "{profile:?}: {source}: {ordinary:?}"
                );
                match (ordinary, retained) {
                    (Ok(a), Ok(b)) => {
                        assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit()))
                    }
                    (Err(a), Err(b)) => {
                        if profile.target().is_armv7() && source.contains("__int128") {
                            assert!(
                                a.message
                                    .contains("__int128 is unavailable on ARMv7 GNU Linux")
                            );
                        }
                        assert_eq!((a.offset, a.message), (b.offset, b.message));
                    }
                    (a, b) => panic!("{profile:?}: {source}: {a:?} / {b:?}"),
                }
            }
        }
    }
}

#[test]
fn effect_walks_do_not_expand_saved_conditions() {
    let mut nested = "p".to_string();
    for _ in 0..24 {
        nested = format!("({nested} ?: p)");
    }
    for profile in CompilerProfile::ALL {
        let source =
            format!("unsigned long long f(int*p){{return __builtin_object_size({nested},0);}}");
        analyze_with_profile(&source, profile, &Default::default()).unwrap();
        analyze_with_profile(
            &source,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
    }
}

#[cfg(unix)]
fn native_oracles() -> Vec<(String, CompilerProfile)> {
    use toucan_target::{Compiler, Target};
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return Vec::new(),
    };
    let mut compilers = vec![(
        "clang".into(),
        CompilerProfile::new(target, Compiler::Clang).unwrap(),
    )];
    if std::env::consts::OS == "linux" {
        compilers.push((
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            CompilerProfile::new(target, Compiler::Gnu).unwrap(),
        ));
    }
    compilers
}

#[cfg(unix)]
#[test]
#[ignore = "requires native GCC/Clang syntax checks"]
fn native_omitted_conditionals_match_admission() {
    use std::process::Command;
    use toucan_target::Compiler;
    let mut cases = serde_json::from_str::<serde_json::Value>(include_str!(
        "fixtures/omitted_conditional.json"
    ))
    .unwrap()
    .as_array()
    .unwrap()
    .clone();
    cases.extend(
        serde_json::from_str::<serde_json::Value>(include_str!(
            "fixtures/conditional_addresses.json"
        ))
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .cloned(),
    );
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("conditional.c");
    for (cc, profile) in native_oracles() {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            for case in &cases {
                let source = case["source"].as_str().unwrap();
                let expected = case
                    .get("expected")
                    .unwrap_or(
                        &case[if profile.compiler() == Compiler::Gnu {
                            "gnu"
                        } else {
                            "clang"
                        }],
                    )
                    .as_bool()
                    .unwrap();
                std::fs::write(&file, source).unwrap();
                let mut command = Command::new(&cc);
                command
                    .arg(format!("-std={mode}"))
                    .arg("-fsyntax-only")
                    .arg(&file);
                let output = command.output().unwrap();
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&output).unwrap(),
                    expected,
                    "{command:?}\n{source}\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(
                    analyze_with_profile(source, profile, &Default::default()).is_ok(),
                    expected,
                    "{profile:?}: {source}"
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
#[ignore = "requires native C compilers and executable output"]
fn native_saved_values_preserve_scalar_and_vla_effects() {
    use std::process::Command;
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/omitted_conditional_runtime.json")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("conditional.c");
    let executable = directory.path().join("conditional");
    for (cc, profile) in native_oracles() {
        for mode in LanguageMode::ALL {
            for case in cases.as_array().unwrap() {
                let source = case["source"].as_str().unwrap();
                analyze_with_profile(
                    source,
                    profile.with_language_mode(mode),
                    &AnalysisOptions {
                        retain_code: true,
                        ..Default::default()
                    },
                )
                .unwrap();
                std::fs::write(&input, source).unwrap();
                for optimization in ["-O0", "-O2"] {
                    let mut command = Command::new(&cc);
                    command
                        .arg(format!("-std={mode}"))
                        .arg(optimization)
                        .arg(&input)
                        .arg("-o")
                        .arg(&executable);
                    let output = command.output().unwrap();
                    assert!(
                        output.status.success(),
                        "{command:?}\n{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let status = Command::new(&executable).status().unwrap();
                    assert!(status.success(), "{command:?}: {status}");
                }
            }
        }
    }
}

#[test]
fn saved_pointer_values_keep_vla_bounds_after_decay() {
    use toucan_semantic::checked::{BoundInput, BoundValue, ExprKind, TypeStep};
    for profile in CompilerProfile::ALL {
        let source = "int f(int n,int m,int(*q)[m]){int a[2][n];return sizeof(*(a ?: q));}";
        let analysis = analyze_with_profile(
            source,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let code = analysis.checked().unwrap();
        let expression = code
            .expressions()
            .find_map(|(_, e)| matches!(e.kind(), ExprKind::OmittedConditional { .. }).then_some(e))
            .unwrap();
        let ExprKind::OmittedConditional {
            condition,
            then_value,
            else_value,
        } = expression.kind()
        else {
            unreachable!()
        };
        let condition_extent = &code.type_use(condition.type_use()).unwrap().extents()[0];
        assert_eq!(condition_extent.path(), [TypeStep::Pointer]);
        assert_eq!(
            code.type_use(then_value.type_use()).unwrap().extents()[0].bound(),
            condition_extent.bound()
        );
        let result = &code.type_use(expression.type_use()).unwrap().extents()[0];
        assert_eq!(result.path(), [TypeStep::Pointer]);
        let BoundValue::Composite { inputs, selection } =
            code.bound(result.bound()).unwrap().value()
        else {
            panic!("composite conditional bound")
        };
        assert_eq!(*selection, Some(condition.expression()));
        assert_eq!(
            inputs,
            &vec![
                BoundInput::Runtime(condition_extent.bound()),
                BoundInput::Runtime(
                    code.type_use(else_value.type_use()).unwrap().extents()[0].bound()
                )
            ]
        );
    }
}

#[test]
fn deeply_nested_pointer_conditions_reach_the_expression_limit() {
    let mut value = "&object".to_string();
    for _ in 0..128 {
        value = format!("({value} ?: 0)");
    }
    let source = format!("int object;int*p={value};");
    for retained in [false, true] {
        let error = analyze_with_profile(
            &source,
            CompilerProfile::ALL[0],
            &AnalysisOptions {
                retain_code: retained,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.message.contains("limit"), "{error:?}");
    }
}

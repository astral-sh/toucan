use toucan_semantic::checked::{
    BoundEvaluation, Builtin, ExprKind, ObjectSizeFoldStage, ObjectSizeResult, ObjectSizeUnknown,
    QueryEvaluation, QuerySuppression, UseContext,
};
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options, evaluate_integer};
use toucan_target::Target;

const TYPES: &str =
    "struct V {char first[10];int middle;char last[10];}; union U {int number;char bytes[12];};";
const OBJECTS: &str = "char a[10], matrix[3][4]; struct V v, records[3]; union U u;";
const CASES: &[(&str, [u64; 4])] = &[
    ("a", [10, 10, 10, 10]),
    ("&a", [10, 10, 10, 10]),
    ("&a[2]", [8, 8, 8, 8]),
    ("&a[10]", [0, 0, 0, 0]),
    ("v.first", [28, 10, 28, 10]),
    ("&v.middle", [16, 4, 16, 4]),
    ("v.last", [12, 10, 12, 10]),
    ("&matrix[1]", [8, 8, 8, 8]),
    ("&matrix[1][1]", [7, 3, 7, 3]),
    ("records[1].first", [56, 10, 56, 10]),
    ("&u.number", [12, 4, 12, 4]),
    ("u.bytes", [12, 12, 12, 12]),
    ("\"abc\"", [4, 4, 4, 4]),
    ("u\"abc\"", [8, 8, 8, 8]),
];

fn options() -> AnalysisOptions {
    AnalysisOptions {
        retain_code: true,
        ..Default::default()
    }
}
fn gnu(target: Target) -> bool {
    matches!(
        target,
        Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
    )
}

#[test]
fn direct_object_extents_and_target_string_widths_are_known() {
    for target in Target::ALL {
        let unit = analyze(&format!("{TYPES}{OBJECTS}"), target).unwrap();
        for (pointer, values) in CASES {
            for (mode, expected) in values.iter().enumerate() {
                for builtin in ["__builtin_object_size", "__builtin_dynamic_object_size"] {
                    let query = format!("{builtin}({pointer},{mode})");
                    let value = evaluate_integer(&unit, &query)
                        .unwrap_or_else(|e| panic!("{target}: {query}: {e}"));
                    assert_eq!(value.value, u128::from(*expected), "{target}: {query}");
                    assert_eq!(value.bits, 64);
                    assert!(!value.signed);
                }
            }
        }
        let wide = if target == Target::X86_64PcWindowsMsvc {
            8
        } else {
            16
        };
        assert_eq!(
            evaluate_integer(&unit, "__builtin_object_size(L\"abc\",0)")
                .unwrap()
                .value,
            wide
        );
        assert_eq!(
            evaluate_integer(&unit, "__builtin_object_size(U\"abc\",0)")
                .unwrap()
                .value,
            16
        );
    }
}

#[test]
fn structural_proofs_do_not_invent_compiler_scalar_results() {
    let source = format!(
        "{TYPES} void f(void) {{{OBJECTS} __builtin_object_size(v.first+2,3); __builtin_object_size(matrix[1],1); __builtin_object_size((char*)&v.middle+1,3);}}"
    );
    for target in Target::ALL {
        let plain = analyze(&source, target).unwrap();
        let analysis = analyze_with_options(&source, target, &options()).unwrap();
        assert_eq!(format!("{plain:?}"), format!("{:?}", analysis.unit()));
        let code = analysis.checked().unwrap();
        let proofs: Vec<_> = code
            .expressions()
            .filter_map(|(_, e)| match e.kind() {
                ExprKind::BuiltinCall {
                    object_size: Some(proof),
                    ..
                } => Some(proof.as_ref()),
                _ => None,
            })
            .collect();
        assert_eq!(proofs.len(), 3);
        assert_eq!(
            (proofs[0].whole_bytes(), proofs[0].subobject_bytes()),
            (Some(26), Some(8))
        );
        if gnu(target) {
            assert_eq!(
                proofs[0].result(),
                ObjectSizeResult::Unresolved(ObjectSizeUnknown::CompilerBehavior)
            );
        } else {
            assert!(
                matches!(proofs[0].result(),ObjectSizeResult::Constant{value,stage:ObjectSizeFoldStage::Frontend,is_default:false} if value.value==8)
            );
        }
        assert_eq!(proofs[1].subobject_bytes(), Some(4));
        let ObjectSizeResult::Constant { value, .. } = proofs[1].result() else {
            panic!()
        };
        assert_eq!(value.value, if gnu(target) { 8 } else { 4 });
        assert_eq!(proofs[2].subobject_bytes(), Some(3));
        if !gnu(target) {
            assert!(
                matches!(proofs[2].result(),ObjectSizeResult::Constant{value,stage:ObjectSizeFoldStage::CodeGeneration,is_default:true} if value.value==0)
            );
        }
    }
}

#[test]
fn frontend_and_code_generation_folds_have_distinct_constant_contexts() {
    for target in Target::ALL {
        for (pointer, mode, clang_constant) in [
            ("a", 0, true),
            ("&a[2]", 3, true),
            ("\"abc\"", 0, false),
            ("\"abc\"", 1, true),
            ("&\"abc\"", 1, false),
            ("(char*)&v.middle+1", 3, false),
        ] {
            let query = format!("__builtin_object_size({pointer},{mode})");
            let accepted = !gnu(target) && clang_constant;
            for declaration in [
                format!("enum {{K={query}}};"),
                format!("unsigned long long value={query};"),
                format!("_Static_assert({query}<100,\"extent\");"),
            ] {
                let source = format!("{TYPES}{OBJECTS}{declaration}");
                for retain_code in [false, true] {
                    let result = analyze_with_options(
                        &source,
                        target,
                        &AnalysisOptions {
                            retain_code,
                            ..Default::default()
                        },
                    );
                    assert_eq!(result.is_ok(), accepted, "{target}: {source}: {result:?}");
                }
            }
        }
    }
}

#[test]
fn constant_queries_only_use_frontend_object_size_proofs() {
    for target in Target::ALL {
        let unit = analyze("char a[2];char *p;", target).unwrap();
        for (pointer, mode, clang_proven) in
            [("a", 0, true), ("\"abc\"", 0, false), ("\"abc\"", 1, true)]
        {
            let expression =
                format!("__builtin_constant_p(__builtin_object_size({pointer},{mode}))");
            let expected = u128::from(!gnu(target) && clang_proven);
            assert_eq!(
                evaluate_integer(&unit, &expression).unwrap().value,
                expected,
                "{target}: {expression}"
            );
            analyze(
                &format!("char a[2];_Static_assert({expression}=={expected},\"proof\");"),
                target,
            )
            .unwrap();
        }
        assert_eq!(
            evaluate_integer(&unit, "__builtin_constant_p(__builtin_object_size(p++,0))")
                .unwrap()
                .value,
            1
        );
    }
}

#[test]
fn known_frontend_folds_suppress_type_effects_but_later_folds_do_not() {
    for pointer in ["a", "\"abc\""] {
        let source = format!(
            "void f(int n){{char a[10]; __builtin_object_size((int (*)[n++]){pointer},0);}}"
        );
        for target in Target::ALL {
            let analysis = analyze_with_options(&source, target, &options()).unwrap();
            let code = analysis.checked().unwrap();
            let (_, expression) = code
                .expressions()
                .find(|(_, e)| {
                    matches!(
                        e.kind(),
                        ExprKind::BuiltinCall {
                            builtin: Builtin::ObjectSize,
                            ..
                        }
                    )
                })
                .unwrap();
            let ExprKind::BuiltinCall {
                object_size: Some(proof),
                arguments,
                query_evaluation,
                ..
            } = expression.kind()
            else {
                panic!()
            };
            let ObjectSizeResult::Constant {
                value,
                stage,
                is_default,
            } = proof.result()
            else {
                panic!()
            };
            if gnu(target) {
                assert_eq!(value.value, u128::from(u64::MAX));
                assert!(is_default);
            } else {
                assert_eq!(value.value, if pointer == "a" { 10 } else { 4 });
                assert!(!is_default);
            }
            let frontend = gnu(target) || pointer == "a";
            assert_eq!(
                stage,
                if frontend {
                    ObjectSizeFoldStage::Frontend
                } else {
                    ObjectSizeFoldStage::CodeGeneration
                }
            );
            assert_eq!(
                arguments[0].context(),
                if frontend {
                    UseContext::UnevaluatedValue
                } else {
                    UseContext::CompilerQuery
                }
            );
            if frontend {
                assert_eq!(
                    *query_evaluation,
                    Some(QueryEvaluation::Unevaluated(
                        QuerySuppression::ObjectSizeFrontendFold
                    ))
                );
            }
            assert_eq!(
                code.bounds().next().unwrap().1.evaluation(),
                if frontend {
                    BoundEvaluation::Unevaluated
                } else {
                    BoundEvaluation::Required
                }
            );
        }
    }
}

#[test]
fn pointer_aliases_vlas_and_default_values_are_not_object_extents() {
    let source = "void f(int n, char *p){char a[n]; __builtin_dynamic_object_size(a,0); __builtin_object_size(p,0); __builtin_object_size(p++,0);}";
    for target in Target::ALL {
        let analysis = analyze_with_options(source, target, &options()).unwrap();
        let code = analysis.checked().unwrap();
        let proofs: Vec<_> = code
            .expressions()
            .filter_map(|(_, e)| match e.kind() {
                ExprKind::BuiltinCall {
                    object_size: Some(proof),
                    ..
                } => Some(proof.as_ref()),
                _ => None,
            })
            .collect();
        assert_eq!(proofs.len(), 3);
        for proof in &proofs {
            assert_eq!(proof.whole_bytes(), None);
            assert_eq!(proof.subobject_bytes(), None);
        }
        for proof in &proofs[..2] {
            assert_eq!(
                proof.result(),
                ObjectSizeResult::Unresolved(ObjectSizeUnknown::Provenance)
            );
        }
        assert!(
            matches!(proofs[2].result(),ObjectSizeResult::Constant{value,is_default:true,..} if value.value==u128::from(u64::MAX))
        );
        assert_eq!(
            code.bounds().next().unwrap().1.evaluation(),
            BoundEvaluation::Required
        );
    }
}

#[test]
fn external_folds_do_not_relax_nested_type_or_body_constraints() {
    let unit = analyze("char a[2];", Target::X86_64UnknownLinuxGnu).unwrap();
    assert_eq!(
        evaluate_integer(&unit, "__builtin_object_size(a,0)")
            .unwrap()
            .value,
        2
    );
    for query in [
        "sizeof(struct S {int bits:__builtin_object_size(a,0);})",
        "__builtin_constant_p(({static int x=__builtin_object_size(a,0); x;}))",
        "__builtin_constant_p(({enum {X=__builtin_object_size(a,0)}; X;}))",
        "__builtin_object_size(a, __builtin_object_size(a,0))",
    ] {
        assert!(evaluate_integer(&unit, query).is_err(), "{query}");
    }
}

#[test]
fn bounded_proofs_keep_shadowing_qualifiers_and_allocation_extents() {
    let source = "char a[20]; void f(void) {const char a[7]; __builtin_object_size((char*)a+2,3); {char a[3]; __builtin_object_size(a,0);} __builtin_object_size(a,0);}";
    let analysis = analyze_with_options(source, Target::X86_64AppleDarwin, &options()).unwrap();
    let values: Vec<_> = analysis
        .checked()
        .unwrap()
        .expressions()
        .filter_map(|(_, expression)| {
            if let ExprKind::BuiltinCall {
                object_size: Some(proof),
                ..
            } = expression.kind()
                && let ObjectSizeResult::Constant { value, .. } = proof.result()
            {
                Some(value.value)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(values, [5, 3, 7]);
    let unit = analyze(
        "struct F {int n;char tail[];}; struct F f={3,{1,2,3}};",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    assert_eq!(
        evaluate_integer(&unit, "__builtin_object_size(&f,0)")
            .unwrap()
            .value,
        7
    );
    for query in [
        "__builtin_object_size((char*)0,0)",
        "__builtin_object_size((char*)1234,0)",
        "__builtin_object_size((char*)&f+0x8000000000000000ULL,0)",
    ] {
        assert!(evaluate_integer(&unit, query).is_err(), "{query}");
    }
    let mut limited = options();
    limited.limits.nodes = 16;
    assert!(
        analyze_with_options(source, Target::X86_64AppleDarwin, &limited)
            .unwrap_err()
            .message
            .contains("retention node limit")
    );
    limited.retain_code = false;
    analyze_with_options(source, Target::X86_64AppleDarwin, &limited).unwrap();
}

#[test]
fn canceled_indirection_preserves_casts_and_reinterpreted_fields_do_not_invent_storage() {
    let source = format!(
        "{TYPES} void f(void) {{char a[10];struct V records[3]; __builtin_object_size(&*(int*)a,3); __builtin_object_size(&*(int*)a+1,3); __builtin_object_size(&((struct V*)a)->middle,0); __builtin_object_size(&records[3].middle,1);}}"
    );
    let analysis = analyze_with_options(&source, Target::X86_64AppleDarwin, &options()).unwrap();
    let proofs: Vec<_> = analysis
        .checked()
        .unwrap()
        .expressions()
        .filter_map(|(_, e)| match e.kind() {
            ExprKind::BuiltinCall {
                object_size: Some(proof),
                ..
            } => Some(proof.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(proofs[0].subobject_bytes(), Some(10));
    assert!(
        matches!(proofs[0].result(), ObjectSizeResult::Constant {value,is_default:true,..} if value.value==0)
    );
    assert_eq!(proofs[1].subobject_bytes(), Some(6));
    assert!(
        matches!(proofs[1].result(), ObjectSizeResult::Constant {value,is_default:true,..} if value.value==0)
    );
    assert_eq!(
        (proofs[2].whole_bytes(), proofs[2].subobject_bytes()),
        (Some(0), None)
    );
    assert!(matches!(
        proofs[2].result(),
        ObjectSizeResult::Unresolved(_)
    ));
    assert_eq!(
        (proofs[3].whole_bytes(), proofs[3].subobject_bytes()),
        (Some(0), Some(0))
    );
    assert!(matches!(
        proofs[3].result(),
        ObjectSizeResult::Unresolved(_)
    ));
}

#[test]
#[ignore = "requires native GCC and Clang plus Clang cross targets; run with --include-ignored"]
fn native_fold_contexts_type_effects_and_target_layouts_match() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("fold.c");
    let output = directory
        .path()
        .join(format!("fold{}", std::env::consts::EXE_SUFFIX));
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc.as_str(), "clang"] {
        let version = std::process::Command::new(compiler)
            .arg("--version")
            .output()
            .unwrap();
        assert!(version.status.success());
        let clang = String::from_utf8_lossy(&version.stdout)
            .to_ascii_lowercase()
            .contains("clang");
        for (pointer, mode, clang_constant) in [
            ("a", 0, true),
            ("\"abc\"", 0, false),
            ("\"abc\"", 1, true),
            ("&\"abc\"", 1, false),
        ] {
            std::fs::write(
                &input,
                format!("char a[10];enum {{K=__builtin_object_size({pointer},{mode})}};"),
            )
            .unwrap();
            let check = std::process::Command::new(compiler)
                .args(["-std=gnu11", "-fsyntax-only"])
                .arg(&input)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&check),
                Ok(clang && clang_constant),
                "{compiler}: {pointer}, {mode}: {}",
                String::from_utf8_lossy(&check.stderr)
            );
        }
        let source = "int printf(const char*,...);int main(void){int n=2;char a[10];unsigned long long q=__builtin_object_size((int(*)[n++])a,0);printf(\"%llu %d\\n\",q,n);n=2;q=__builtin_object_size((int(*)[n++])\"abc\",0);printf(\"%llu %d\\n\",q,n);n=2;q=__builtin_object_size((int(*)[n++])\"abc\",3);printf(\"%llu %d\\n\",q,n);printf(\"%llu %llu\\n\",(unsigned long long)__builtin_object_size(&*(int*)a,3),(unsigned long long)__builtin_object_size(&*(int*)a+1,3));return 0;}";
        std::fs::write(&input, source).unwrap();
        for optimization in ["-O0", "-O2"] {
            let build = std::process::Command::new(compiler)
                .args(["-std=gnu11", optimization])
                .arg(&input)
                .arg("-o")
                .arg(&output)
                .output()
                .unwrap();
            assert!(
                build.status.success(),
                "{}",
                String::from_utf8_lossy(&build.stderr)
            );
            let run = std::process::Command::new(&output).output().unwrap();
            assert!(run.status.success());
            let expected = if clang {
                "10 2\n4 3\n4 2\n0 0\n".to_owned()
            } else {
                format!(
                    "{} 2\n{} 2\n0 2\n10 {}\n",
                    u64::MAX,
                    u64::MAX,
                    if optimization == "-O0" { 0 } else { 6 }
                )
            };
            assert_eq!(
                String::from_utf8(run.stdout).unwrap(),
                expected,
                "{compiler} {optimization}"
            );
        }
    }
    for target in Target::ALL {
        let source = "struct W {char lead;long word;long double extended;};struct W w;";
        let unit = analyze(source, target).unwrap();
        let mut assertions = source.to_owned();
        for pointer in ["&w.word", "&w.extended", "L\"abc\"", "u\"abc\""] {
            for mode in [1, 3] {
                let query = format!("__builtin_object_size({pointer},{mode})");
                let value = evaluate_integer(&unit, &query).unwrap().value;
                assertions.push_str(&format!("_Static_assert({query}=={value},\"extent\");"));
            }
        }
        std::fs::write(&input, assertions).unwrap();
        let check = std::process::Command::new("clang")
            .args(["-target", target.triple(), "-std=gnu11", "-fsyntax-only"])
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            check.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&check.stderr)
        );
    }
}

#[test]
#[ignore = "requires native GCC and Clang; run with --include-ignored"]
fn scalar_extents_and_fold_stages_match_native_compilers() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("extent.c");
    let output = directory
        .path()
        .join(format!("extent{}", std::env::consts::EXE_SUFFIX));
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let mut source = format!("int printf(const char*,...);{TYPES}int main(void){{{OBJECTS}");
    for builtin in ["__builtin_object_size", "__builtin_dynamic_object_size"] {
        for (pointer, _) in CASES {
            for mode in 0..4 {
                source.push_str(&format!(
                    "printf(\"%llu\\n\",(unsigned long long){builtin}({pointer},{mode}));"
                ));
            }
        }
    }
    source.push_str("return 0;}");
    std::fs::write(&input, &source).unwrap();
    let expected: Vec<_> = CASES
        .iter()
        .flat_map(|(_, values)| values)
        .copied()
        .collect();
    let expected = expected.repeat(2);
    for compiler in [gcc.as_str(), "clang"] {
        for optimization in ["-O0", "-O2"] {
            let build = std::process::Command::new(compiler)
                .args(["-std=gnu11", optimization])
                .arg(&input)
                .arg("-o")
                .arg(&output)
                .output()
                .unwrap();
            assert!(
                build.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&build.stderr)
            );
            let run = std::process::Command::new(&output).output().unwrap();
            assert!(run.status.success());
            let actual: Vec<u64> = String::from_utf8(run.stdout)
                .unwrap()
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            assert_eq!(actual, expected, "{compiler} {optimization}");
        }
    }
}

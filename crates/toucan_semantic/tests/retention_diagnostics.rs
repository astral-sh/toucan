use toucan_semantic::checked::{BoundEvaluation, TypeOperandEvaluation};
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

#[test]
fn constant_context_diagnostics_match_without_retention() {
    for target in Target::ALL {
        for source in [
            "enum { A = B };",
            "enum { A = 1 / 0 + B };",
            "enum { A = (int)&B };",
            "int a[B];",
            "int a[1 / 0];",
            "_Static_assert(B, \"unknown\");",
            "_Static_assert(1 / 0 + B, \"order\");",
            "struct S { unsigned value : B; };",
        ] {
            let ordinary = analyze(source, target).unwrap_err();
            let retained = analyze_with_options(
                source,
                target,
                &AnalysisOptions {
                    retain_code: true,
                    ..AnalysisOptions::default()
                },
            )
            .unwrap_err();
            assert_eq!(
                (ordinary.offset, ordinary.message),
                (retained.offset, retained.message),
                "{target:?}: {source}"
            );
        }
    }
}

#[test]
fn constant_type_queries_preserve_runtime_bound_contexts() {
    let source = "int f(int n) { enum { A=sizeof(int (*)[n++]), B=_Alignof(int[n++]), C=sizeof(typeof((int (*)[n++])0)) }; return A+B+C; }";
    let analysis = analyze_with_options(
        source,
        Target::X86_64UnknownLinuxGnu,
        &AnalysisOptions {
            retain_code: true,
            ..AnalysisOptions::default()
        },
    )
    .unwrap();
    let code = analysis.checked().unwrap();
    let evaluations: Vec<_> = code.bounds().map(|(_, bound)| bound.evaluation()).collect();
    assert_eq!(
        evaluations,
        [
            BoundEvaluation::MayBeOmitted,
            BoundEvaluation::Unevaluated,
            BoundEvaluation::MayBeOmitted
        ]
    );
    assert_eq!(
        code.type_operands().next().unwrap().1.evaluation(),
        TypeOperandEvaluation::MayBeOmitted
    );
}

#[test]
fn successful_constant_queries_reuse_checked_type_and_body_scopes() {
    for source in [
        "enum { A=sizeof(struct First {int a;}) ? sizeof(struct Second {int b;}) : sizeof(struct Third {int c;}) };",
        "int f(void) { enum { A=sizeof(({ struct S {int x;}; struct S value={0}; value; })) }; return A; }",
    ] {
        let ordinary = analyze(source, Target::X86_64UnknownLinuxGnu).unwrap();
        let retained = analyze_with_options(
            source,
            Target::X86_64UnknownLinuxGnu,
            &AnalysisOptions {
                retain_code: true,
                ..AnalysisOptions::default()
            },
        )
        .unwrap();
        assert_eq!(format!("{ordinary:?}"), format!("{:?}", retained.unit()));
        if source.contains("struct S ") {
            assert_eq!(
                retained
                    .unit()
                    .records
                    .iter()
                    .filter(|record| record.name.as_deref() == Some("S"))
                    .count(),
                1
            );
            let code = retained.checked().unwrap();
            assert_eq!(
                code.entities()
                    .filter(|(_, entity)| entity.name() == Some("value"))
                    .count(),
                1
            );
        }
    }
}

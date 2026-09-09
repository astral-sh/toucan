use toucan_semantic::{AnalysisOptions, TypeKind, analyze_with_profile};
use toucan_target::CompilerProfile;

#[test]
fn written_aliases_do_not_change_canonical_types_or_initializer_values() {
    let source = "typedef int First; typedef int Second; extern First shared; Second shared=7; extern First *pointer; Second *pointer;";
    for profile in CompilerProfile::ALL {
        let plain = analyze_with_profile(source, profile, &AnalysisOptions::default()).unwrap();
        for retained in [false, true] {
            let analysis = analyze_with_profile(
                source,
                profile,
                &AnalysisOptions {
                    retain_object_values: true,
                    retain_code: retained,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                format!("{:?}", plain.unit()),
                format!("{:?}", analysis.unit())
            );
            let occurrences = analysis.object_values().unwrap().entries();
            assert!(
                matches!(&occurrences[0].ty().kind, TypeKind::Typedef(name) if name == "First")
            );
            assert!(
                matches!(&occurrences[1].ty().kind, TypeKind::Typedef(name) if name == "Second")
            );
            assert!(
                matches!(&occurrences[3].ty().kind, TypeKind::Pointer(inner) if matches!(&inner.kind, TypeKind::Typedef(name) if name == "Second"))
            );
            assert!(occurrences[1].value().is_some());
        }
    }
}

#[test]
fn missing_bounds_and_prototypes_keep_the_completed_type() {
    for source in [
        "typedef int First[4]; typedef int Second[]; extern First shared; Second shared={1,2};",
        "typedef int First[4]; typedef int Second[]; extern First *shared; Second *shared;",
        "typedef int(*First)(int); typedef int(*Second)(); extern First shared; Second shared;",
    ] {
        let analysis = analyze_with_profile(
            source,
            CompilerProfile::ALL[0],
            &AnalysisOptions {
                retain_object_values: true,
                ..Default::default()
            },
        )
        .unwrap();
        let occurrences = analysis.object_values().unwrap().entries();
        assert_eq!(occurrences[0].ty(), occurrences[1].ty(), "{source}");
    }
    let source =
        "typedef int First[]; typedef int Second[4]; extern First shared; Second shared={1,2};";
    let analysis = analyze_with_profile(
        source,
        CompilerProfile::ALL[0],
        &AnalysisOptions {
            retain_object_values: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        matches!(&analysis.object_values().unwrap().entries()[1].ty().kind, TypeKind::Typedef(name) if name == "Second")
    );
}

#[test]
fn repeated_redeclarations_share_one_capture_comparison_budget() {
    let parameters = vec!["int"; 1024].join(",");
    let mut source = format!(
        "typedef int(*First)({parameters}); typedef int(*Second)({parameters}); extern First shared;"
    );
    for _ in 0..600 {
        source.push_str("extern Second shared;");
    }
    let profile = CompilerProfile::ALL[0];
    analyze_with_profile(&source, profile, &AnalysisOptions::default()).unwrap();
    let error = analyze_with_profile(
        &source,
        profile,
        &AnalysisOptions {
            retain_object_values: true,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        error.message,
        "object-type comparison reference limit exceeded"
    );
    assert!(error.offset > parameters.len());
}

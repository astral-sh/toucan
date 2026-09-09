use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::CompilerProfile;

const SOURCE: &str = "typedef int Array[4]; void original(Array); int first=sizeof(void(*)(Array)), second[sizeof(void(*)(Array))]; __typeof__(sizeof(__typeof__(original)*)) count; struct Holder { void(*callback)(Array); int values[sizeof(void(*)(Array))]; }; __typeof__(original) after;";

#[test]
fn expression_type_operands_do_not_become_declaration_dependencies() {
    for profile in CompilerProfile::ALL {
        let plain = analyze_with_profile(SOURCE, profile, &AnalysisOptions::default()).unwrap();
        for checked in [false, true] {
            let analysis = analyze_with_profile(
                SOURCE,
                profile,
                &AnalysisOptions {
                    retain_parameter_type_dependencies: true,
                    retain_code: checked,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                format!("{:?}", plain.unit()),
                format!("{:?}", analysis.unit())
            );
            let dependencies = analysis.parameter_type_dependencies().unwrap();
            let anchors: Vec<_> = dependencies
                .occurrences()
                .iter()
                .map(|entry| &SOURCE[entry.source().range()])
                .collect();
            assert_eq!(
                anchors,
                ["original", "callback", "after"],
                "{profile:?}, checked={checked}"
            );
            assert_eq!(dependencies.records().len(), 1);
        }
    }
}

#[test]
fn record_declarations_inside_expressions_keep_their_real_field_dependencies() {
    let source = "typedef int Array[4]; int value=sizeof(struct Holder { void(*callback)(Array); int count[sizeof(void(*)(Array))]; }); extern struct Holder *object; void after(Array);";
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile(
            source,
            profile,
            &AnalysisOptions {
                retain_parameter_type_dependencies: true,
                ..Default::default()
            },
        )
        .unwrap();
        let dependencies = analysis.parameter_type_dependencies().unwrap();
        let anchors: Vec<_> = dependencies
            .occurrences()
            .iter()
            .map(|entry| &source[entry.source().range()])
            .collect();
        assert_eq!(anchors, ["callback", "after"]);
        assert_eq!(dependencies.records().len(), 1);
        let (&record, names) = dependencies.records().iter().next().unwrap();
        assert_eq!(
            analysis.unit().records[record].name.as_deref(),
            Some("Holder")
        );
        assert_eq!(*names, ["Array".into()].into());
    }
}

#[test]
fn expression_suspension_preserves_existing_diagnostic_offsets() {
    for source in [
        "typedef int Array[4]; int object[sizeof(void(*)(Unknown))];",
        "typedef int Array[4]; int first=sizeof(void(*)(Array)), object[-1];",
        "typedef int Array[4]; int object __attribute__((aligned(3)));",
    ] {
        let profile = CompilerProfile::ALL[0];
        let plain = analyze_with_profile(source, profile, &AnalysisOptions::default()).unwrap_err();
        let retained = analyze_with_profile(
            source,
            profile,
            &AnalysisOptions {
                retain_parameter_type_dependencies: true,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(format!("{plain:?}"), format!("{retained:?}"));
    }
}

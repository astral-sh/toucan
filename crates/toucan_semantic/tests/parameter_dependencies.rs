use toucan_semantic::{AnalysisOptions, DeclarationTarget, analyze_with_profile};
use toucan_target::CompilerProfile;

fn options() -> AnalysisOptions {
    AnalysisOptions {
        retain_parameter_type_dependencies: true,
        limits: toucan_semantic::checked::Limits {
            nodes: 0,
            edges: 0,
            payload_bytes: 0,
        },
        ..Default::default()
    }
}

#[test]
fn optional_dependencies_preserve_c_types_without_checked_code() {
    let source = "typedef int Array[4]; typedef Array Chain; typedef void Callback(Chain); struct Holder { void (*callback)(Array); }; void use(Chain, Callback*, struct Holder*);";
    for profile in CompilerProfile::ALL {
        let plain = analyze_with_profile(source, profile, &AnalysisOptions::default()).unwrap();
        let captured = analyze_with_profile(source, profile, &options()).unwrap();
        assert!(plain.parameter_type_dependencies().is_none());
        assert!(captured.checked().is_none());
        assert!(captured.declaration_origins().is_none());
        assert_eq!(
            format!("{:?}", plain.unit()),
            format!("{:?}", captured.unit())
        );
        let dependencies = captured.parameter_type_dependencies().unwrap();
        assert_eq!(dependencies.typedefs()["Callback"], ["Chain".into()].into());
        let record = captured
            .unit()
            .records
            .iter()
            .position(|r| r.name.as_deref() == Some("Holder"))
            .unwrap();
        assert_eq!(dependencies.records()[&record], ["Array".into()].into());
        for occurrence in dependencies.occurrences() {
            assert!(source.is_char_boundary(occurrence.source().range().start));
            assert!(occurrence.source().range().end <= source.len());
            assert!(
                occurrence
                    .typedefs()
                    .iter()
                    .all(|name| captured.unit().typedefs.contains_key(name))
            );
            match occurrence.owner() {
                DeclarationTarget::Declaration(id) => {
                    assert!(id < captured.unit().declarations.len())
                }
                DeclarationTarget::Record(id) => assert!(id < captured.unit().records.len()),
                owner => panic!("unexpected signature owner {owner:?}"),
            }
        }
    }
}

#[test]
fn direct_types_use_first_prototypes_and_nested_cursors_keep_their_own_aliases() {
    let source = "typedef int A[4]; typedef int B[8]; void direct(A); void direct(B); void nested(void (*first)(A)); void nested(void (*second)(B)); typedef void Callback(A); typedef void Callback(B); void unprototyped(); void unprototyped(B);";
    let analysis = analyze_with_profile(source, CompilerProfile::ALL[0], &options()).unwrap();
    let dependencies = analysis.parameter_type_dependencies().unwrap();
    let rows: Vec<_> = dependencies
        .occurrences()
        .iter()
        .map(|entry| {
            (
                source[entry.source().range()].to_owned(),
                entry.typedefs().iter().cloned().collect::<Vec<_>>(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("direct".into(), vec!["A".into()]),
            ("direct".into(), vec!["A".into()]),
            ("first".into(), vec!["A".into()]),
            ("second".into(), vec!["B".into()]),
            ("Callback".into(), vec!["A".into()]),
            ("Callback".into(), vec!["A".into()]),
            ("unprototyped".into(), vec!["B".into()]),
        ]
    );
    assert_eq!(
        dependencies.occurrences()[0].owner(),
        dependencies.occurrences()[1].owner()
    );
    assert_eq!(
        dependencies.occurrences()[2].owner(),
        dependencies.occurrences()[3].owner()
    );
    assert_eq!(
        &source[dependencies.occurrences()[2].owner_source().range()],
        "nested"
    );
    assert_ne!(
        dependencies.occurrences()[2].owner_source().range(),
        dependencies.occurrences()[3].owner_source().range()
    );
}

#[test]
fn local_aliases_and_parameter_name_shadowing_do_not_capture_unrelated_globals() {
    let source = "typedef int A[4]; typedef int B[8]; void outer(int A, void (*callback)(B)); void body(void) { typedef double A[3]; void local(A); struct Local { void (*callback)(A); }; } void after(A);";
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile(source, profile, &options()).unwrap();
        let dependencies = analysis.parameter_type_dependencies().unwrap();
        assert!(dependencies.records().is_empty());
        assert!(dependencies.typedefs().is_empty());
        let rows: Vec<_> = dependencies
            .occurrences()
            .iter()
            .map(|entry| {
                (
                    source[entry.source().range()].to_owned(),
                    entry.typedefs().iter().cloned().collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("callback".into(), vec!["B".into()]),
                ("after".into(), vec!["A".into()])
            ]
        );
    }
}

#[test]
fn capture_errors_do_not_change_c_diagnostics() {
    for source in [
        "typedef int A[4]; void f(A); int f(A);",
        "typedef int A[4]; void f(int A, A other);",
        "typedef int A[4]; struct S { void (*callback)(A); int callback; };",
    ] {
        let profile = CompilerProfile::ALL[0];
        let plain = analyze_with_profile(source, profile, &AnalysisOptions::default()).unwrap_err();
        let captured = analyze_with_profile(source, profile, &options()).unwrap_err();
        assert_eq!(format!("{plain:?}"), format!("{captured:?}"));
    }
}

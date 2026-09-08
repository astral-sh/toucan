use toucan_semantic::{AnalysisOptions, DeclarationTarget, analyze_with_profile};
use toucan_target::{CompilerProfile, LanguageMode};

#[test]
fn declaration_origins_are_independent_of_checked_code_and_keep_redeclarations() {
    let source = "struct S; int f(int); typedef int Number; enum E { VALUE=2 }; struct S { int x; }; int f(int x) { struct Local {int y;}; return x; } extern int object;";
    for profile in CompilerProfile::ALL
        .into_iter()
        .flat_map(|profile| LanguageMode::ALL.map(|mode| profile.with_language_mode(mode)))
    {
        let plain = analyze_with_profile(source, profile, &AnalysisOptions::default()).unwrap();
        assert!(plain.declaration_origins().is_none());
        let options = AnalysisOptions {
            retain_declaration_origins: true,
            limits: toucan_semantic::checked::Limits {
                nodes: 0,
                edges: 0,
                payload_bytes: 0,
            },
            ..Default::default()
        };
        let analysis = analyze_with_profile(source, profile, &options).unwrap();
        assert!(analysis.checked().is_none());
        assert_eq!(
            format!("{:?}", plain.unit()),
            format!("{:?}", analysis.unit())
        );
        let entries = analysis.declaration_origins().unwrap().entries();
        assert!(
            entries
                .windows(2)
                .all(|pair| pair[0].source().range().start <= pair[1].source().range().start)
        );
        let names: Vec<_> = entries
            .iter()
            .map(|entry| &source[entry.source().range().clone()])
            .collect();
        assert_eq!(
            names,
            ["S", "f", "Number", "E", "VALUE", "S", "f", "object"],
            "{profile:?}"
        );
        assert_eq!(entries[0].target(), entries[5].target());
        assert!(!entries[0].is_definition());
        assert!(entries[5].is_definition());
        assert_eq!(entries[1].target(), entries[6].target());
        assert!(!entries[1].is_definition());
        assert!(entries[6].is_definition());
        assert!(entries[1].is_external() && entries[6].is_external());
        assert!(!entries[2].is_external());
        let DeclarationTarget::Declaration(index) = entries[1].target() else {
            panic!()
        };
        assert_eq!(analysis.unit().declarations[index].name, "f");
    }
}

#[test]
fn declaration_origins_keep_external_and_internal_occurrences_distinct() {
    let source =
        "static int hidden; extern int hidden; extern int visible; int defined(void){return 1;}";
    let analysis = analyze_with_profile(
        source,
        CompilerProfile::ALL[0],
        &AnalysisOptions {
            retain_declaration_origins: true,
            retain_code: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(analysis.checked().is_some());
    let entries = analysis.declaration_origins().unwrap().entries();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.is_external())
            .collect::<Vec<_>>(),
        [false, false, true, true]
    );
    assert_eq!(entries[0].target(), entries[1].target());
}

#[test]
fn tag_references_are_distinct_from_new_and_standalone_declarations() {
    let source = "struct S { int x; }; enum E { A }; _Static_assert(sizeof(struct S) > 0, \"\"); _Static_assert(sizeof(enum E) > 0, \"\"); struct S; enum E; struct New *object;";
    let analysis = analyze_with_profile(
        source,
        CompilerProfile::ALL[0],
        &AnalysisOptions {
            retain_declaration_origins: true,
            ..Default::default()
        },
    )
    .unwrap();
    let tags: Vec<_> = analysis
        .declaration_origins()
        .unwrap()
        .entries()
        .iter()
        .filter(|origin| {
            matches!(
                origin.target(),
                DeclarationTarget::Record(_) | DeclarationTarget::Enum(_)
            )
        })
        .collect();
    assert_eq!(
        tags.iter()
            .map(|origin| origin.is_reference())
            .collect::<Vec<_>>(),
        [false, false, true, true, false, false, false]
    );
    assert_eq!(tags[0].target(), tags[2].target());
    assert_eq!(tags[0].target(), tags[4].target());
    assert_eq!(tags[1].target(), tags[3].target());
    assert_eq!(tags[1].target(), tags[5].target());
}

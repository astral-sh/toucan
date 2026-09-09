use toucan_semantic::{AnalysisOptions, DocumentationTarget, analyze_with_options};
use toucan_target::Target;

#[test]
fn documentation_origins_keep_declaration_starts_members_and_parents() {
    let source = "typedef struct Tag { enum Mode { VALUE=1 } mode; int first, second; } Alias; struct Tag; struct Tag *object; int function(void) { struct Local { int hidden; } local; return 0; }";
    let analysis = analyze_with_options(
        source,
        Target::X86_64UnknownLinuxGnu,
        &AnalysisOptions {
            retain_documentation_origins: true,
            ..AnalysisOptions::default()
        },
    )
    .unwrap();
    assert!(analysis.checked().is_none());
    assert!(analysis.declaration_origins().is_none());
    let entries = analysis.documentation_origins().unwrap().entries();
    let alias = entries.iter().find(|entry| matches!(entry.target(), DocumentationTarget::Declaration(id) if analysis.unit().declarations[id].name == "Alias")).unwrap();
    assert_eq!(alias.begin(), 0);
    assert_eq!(alias.name(), source.find("Alias").unwrap());
    let mode = entries
        .iter()
        .find(|entry| matches!(entry.target(), DocumentationTarget::Enum(_)))
        .unwrap();
    assert_eq!(mode.parent_name(), source.find("Tag"));
    let value = entries
        .iter()
        .find(|entry| matches!(entry.target(), DocumentationTarget::Enumerator { .. }))
        .unwrap();
    assert_eq!(value.parent_name(), source.find("Mode"));
    let second = entries
        .iter()
        .find(|entry| entry.name() == source.find("second").unwrap())
        .unwrap();
    assert!(matches!(
        second.target(),
        DocumentationTarget::Field { field: 2, .. }
    ));
    assert_eq!(second.begin(), source.find("int first").unwrap());
    assert_eq!(second.parent_name(), source.find("Tag"));
    assert!(
        !entries
            .iter()
            .any(|entry| entry.name() == source.find("hidden").unwrap())
    );
    let tag = analysis
        .unit()
        .records
        .iter()
        .position(|record| record.name.as_deref() == Some("Tag"))
        .unwrap();
    let tags: Vec<_> = entries
        .iter()
        .filter(|entry| matches!(entry.target(), DocumentationTarget::Record(id) if id == tag))
        .collect();
    assert_eq!(tags.len(), 3);
    assert!(!tags[1].is_reference());
    assert!(tags[2].is_reference());
    assert!(
        analyze_with_options(
            source,
            Target::X86_64UnknownLinuxGnu,
            &AnalysisOptions::default()
        )
        .unwrap()
        .documentation_origins()
        .is_none()
    );
}

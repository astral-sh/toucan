use toucan::semantic::{AnalysisOptions, DeclarationAlignment, analyze_with_profile};
use toucan::{CompilerProfile, Target};

#[test]
fn alignment_serialization_exposes_bytes_and_preserves_explicit_zero() {
    let alignment = DeclarationAlignment::new(Some(32), Some(0)).unwrap();
    assert_eq!(
        serde_json::to_value(alignment).unwrap(),
        serde_json::json!({"gnu":32,"c11":0})
    );
    let analysis = analyze_with_profile(
        "extern int x __attribute__((aligned(1))); extern int x;",
        CompilerProfile::default_for(Target::X86_64UnknownLinuxGnu),
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    )
    .unwrap();
    let declaration = analysis
        .unit()
        .declarations
        .iter()
        .find(|declaration| declaration.name == "x")
        .unwrap();
    assert_eq!(
        serde_json::to_value(declaration.alignment).unwrap(),
        serde_json::json!({"gnu":1,"effective":4})
    );
    let plain = analyze_with_profile(
        "int x;",
        CompilerProfile::default_for(Target::X86_64UnknownLinuxGnu),
        &AnalysisOptions::default(),
    )
    .unwrap();
    let value = serde_json::to_value(plain.unit()).unwrap();
    assert!(value["declarations"][0].get("alignment").is_none());
}

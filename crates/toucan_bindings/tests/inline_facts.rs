use toucan_bindings::{Options, generate};
use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};
#[test]
fn caller_built_inline_facts_are_validated() {
    let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    let compilation = analyze_with_profile(
        "int body(void){return 1;} inline int inlined(void); int value;",
        profile,
        &AnalysisOptions::default(),
    )
    .unwrap();
    let options = Options::default();
    let mut unit = compilation.unit().clone();
    let facts = unit
        .declarations
        .iter()
        .find(|declaration| declaration.name == "inlined")
        .unwrap()
        .inline_facts
        .unwrap();
    unit.declarations
        .iter_mut()
        .find(|declaration| declaration.name == "value")
        .unwrap()
        .inline_facts = Some(facts);
    assert!(
        generate(&unit, &options)
            .unwrap_err()
            .to_string()
            .contains("inline facts")
    );
    let mut unit = compilation.unit().clone();
    let declaration = unit
        .declarations
        .iter_mut()
        .find(|declaration| declaration.name == "inlined")
        .unwrap();
    declaration
        .inline_facts
        .as_mut()
        .unwrap()
        .has_inline_definition = true;
    assert!(
        generate(&unit, &options)
            .unwrap_err()
            .to_string()
            .contains("inline facts")
    );
}

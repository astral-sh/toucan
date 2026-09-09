use std::path::Path;
use toucan::{BindingOptions, Compiler, CompilerProfile, Config, Target};

fn compilation(source: &str) -> toucan::Compilation {
    let mut config = Config::with_profile(
        CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap(),
    );
    config.analysis.retain_object_values = true;
    toucan::parse_source(Path::new("objects.h"), source, &config).unwrap()
}

#[test]
fn selected_later_object_keeps_nested_typedefs_and_qualifiers() {
    for (source, expected) in [
        (
            "typedef int First; typedef int Second; extern First shared; Second shared=7;",
            "pub const shared: Second = 7;",
        ),
        (
            "typedef int First; typedef int Second; extern const First **shared; const Second **shared;",
            "pub static mut shared: *mut *const Second;",
        ),
        (
            "typedef const int First; typedef int Second; extern First shared[4]; const Second shared[4]={1,2};",
            "pub static shared: [Second; 4];",
        ),
    ] {
        let compilation = compilation(source);
        let occurrence = compilation.object_values().unwrap().entries()[1].clone();
        let options = BindingOptions {
            allowlist: vec!["shared".into()],
            object_bindings: [("shared".into(), occurrence)].into(),
            ..Default::default()
        };
        let generated = compilation.bindings(&options).unwrap().0;
        assert!(generated.contains(expected), "{source}\n{generated}");
        assert!(!generated.contains("pub type First ="));
    }
}

#[test]
fn occurrence_validation_rejects_incompatible_nested_types_and_profiles() {
    let original = compilation("typedef int First; extern const First *shared;");
    let occurrence = original.object_values().unwrap().entries()[0].clone();
    for source in [
        "typedef int First; extern First *shared;",
        "typedef int First; extern const float *shared;",
    ] {
        let other = compilation(source);
        let options = BindingOptions {
            object_bindings: [("shared".into(), occurrence.clone())].into(),
            ..Default::default()
        };
        assert!(
            other
                .bindings(&options)
                .unwrap_err()
                .to_string()
                .contains("different declaration type")
        );
    }
    let mut config = Config::new(Target::X86_64UnknownLinuxGnu);
    config.analysis.retain_object_values = true;
    let other = toucan::parse_source(
        Path::new("objects.h"),
        "typedef int First; extern const First *shared;",
        &config,
    )
    .unwrap();
    let options = BindingOptions {
        object_bindings: [("shared".into(), occurrence)].into(),
        ..Default::default()
    };
    assert!(
        other
            .bindings(&options)
            .unwrap_err()
            .to_string()
            .contains("different compiler profile")
    );
}

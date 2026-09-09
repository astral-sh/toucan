use toucan_bindings::{BindingSelection, Options, generate};
use toucan_semantic::{AnalysisOptions, analyze_with_options};
use toucan_target::Target;

#[test]
fn extra_occurrences_are_explicit_roots_with_checked_types_and_names() {
    let analysis = analyze_with_options(
        "typedef int Number; extern Number shared; Number shared=7;",
        Target::X86_64UnknownLinuxGnu,
        &AnalysisOptions {
            retain_object_values: true,
            ..Default::default()
        },
    )
    .unwrap();
    let objects = analysis.object_values().unwrap().entries();
    let mut options = Options {
        selection: Some(Box::<BindingSelection>::default()),
        additional_objects: [
            ("external".into(), objects[0].clone()),
            ("constant".into(), objects[1].clone()),
        ]
        .into(),
        ..Default::default()
    };
    let bindings = generate(analysis.unit(), &options).unwrap();
    assert_eq!(bindings.declarations, 2);
    assert!(bindings.source.contains("pub type Number ="));
    assert!(bindings.source.contains("pub static mut external: Number;"));
    assert!(bindings.source.contains("pub const constant: Number = 7;"));
    options
        .additional_objects
        .insert("bad name".into(), objects[0].clone());
    assert!(
        generate(analysis.unit(), &options)
            .unwrap_err()
            .to_string()
            .contains("nonempty ASCII identifier")
    );
    options.additional_objects.remove("bad name");
    let other = analyze_with_options(
        "typedef int Number; extern float shared;",
        Target::X86_64UnknownLinuxGnu,
        &AnalysisOptions::default(),
    )
    .unwrap();
    assert!(
        generate(other.unit(), &options)
            .unwrap_err()
            .to_string()
            .contains("different declaration type")
    );
}

#[test]
fn extra_extern_occurrences_keep_tls_and_weak_linkage_guards() {
    for (source, expected) in [
        (
            "_Thread_local int shared=1; extern _Thread_local int shared;",
            "thread-local object",
        ),
        (
            "int shared __attribute__((weak))=1; extern int shared;",
            "weak symbol",
        ),
    ] {
        let analysis = analyze_with_options(
            source,
            Target::X86_64UnknownLinuxGnu,
            &AnalysisOptions {
                retain_object_values: true,
                ..Default::default()
            },
        )
        .unwrap();
        let objects = analysis.object_values().unwrap().entries();
        let options = Options {
            object_bindings: [("shared".into(), objects[0].clone())].into(),
            additional_objects: [("external".into(), objects[1].clone())].into(),
            ..Default::default()
        };
        assert!(
            generate(analysis.unit(), &options)
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
    }
}

#[test]
fn internal_objects_require_a_materialized_constant_projection() {
    let analysis = analyze_with_options(
        "static int scalar=7; static const char bytes[]=\"ok\"; static int hidden; static int *pointer=&scalar;",
        Target::X86_64UnknownLinuxGnu,
        &AnalysisOptions {
            retain_object_values: true,
            ..Default::default()
        },
    )
    .unwrap();
    let objects = analysis.object_values().unwrap().entries();
    for object in &objects[2..] {
        let options = Options {
            selection: Some(Box::<BindingSelection>::default()),
            additional_objects: [("external".into(), object.clone())].into(),
            ..Default::default()
        };
        assert!(
            generate(analysis.unit(), &options)
                .unwrap_err()
                .to_string()
                .contains("internal object")
        );
    }
    let options = Options {
        selection: Some(Box::<BindingSelection>::default()),
        additional_objects: [
            ("number".into(), objects[0].clone()),
            ("string".into(), objects[1].clone()),
        ]
        .into(),
        ..Default::default()
    };
    let source = generate(analysis.unit(), &options).unwrap().source;
    assert!(source.contains("pub const number: ::core::ffi::c_int = 7;"));
    assert!(source.contains("pub const string: &[::core::primitive::u8; 3]"));
    assert!(!source.contains("pub static"));
}

#[test]
fn additional_dll_imports_share_primary_symbol_library_validation() {
    let analysis = analyze_with_options(
        "__declspec(dllimport) extern int first __asm__(\"shared\"); __declspec(dllimport) extern int second __asm__(\"shared\");",
        Target::X86_64PcWindowsMsvc,
        &AnalysisOptions {
            retain_object_values: true,
            ..Default::default()
        },
    )
    .unwrap();
    let objects = analysis.object_values().unwrap().entries();
    let mut options = Options {
        selection: Some(Box::<BindingSelection>::default()),
        additional_objects: [
            ("extra_first".into(), objects[0].clone()),
            ("extra_second".into(), objects[1].clone()),
        ]
        .into(),
        ..Default::default()
    };
    assert!(
        generate(analysis.unit(), &options)
            .unwrap_err()
            .to_string()
            .contains("requires a matching DLL import library")
    );
    options.dll_import_libraries = [
        ("first".into(), "alpha".into()),
        ("second".into(), "beta".into()),
    ]
    .into();
    assert!(
        generate(analysis.unit(), &options)
            .unwrap_err()
            .to_string()
            .contains("conflicting library rules")
    );
    options.additional_objects.remove("extra_first");
    options.selection.as_mut().unwrap().declarations.insert(0);
    assert!(
        generate(analysis.unit(), &options)
            .unwrap_err()
            .to_string()
            .contains("conflicting library rules")
    );
    options
        .dll_import_libraries
        .insert("second".into(), "alpha".into());
    let source = generate(analysis.unit(), &options).unwrap().source;
    assert_eq!(source.matches("#[link(name = \"alpha\"").count(), 2);
    assert_eq!(source.matches("#[link_name = \"shared\"]").count(), 2);
}

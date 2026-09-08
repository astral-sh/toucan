use std::path::Path;
use toucan_preprocessor::{Config, Preprocessor};

#[test]
fn physical_files_and_macro_definitions_survive_diagnostic_remapping() {
    let mut config = Config {
        record_file_origins: true,
        allow_filesystem: false,
        ..Default::default()
    };
    config.virtual_headers.insert(
        "inner.h".into(),
        "#define INNER 2\n#line 400 \"diagnostic.h\"\nint inner;\n".into(),
    );
    let source = "#define OUTER 1\n#define REPLACED 2\n#define ERASED 3\n#undef REPLACED\n#define REPLACED 4\n#undef ERASED\n#include \"inner.h\"\nint outer;\n";
    let recorded = Preprocessor::new(config.clone())
        .preprocess_str(Path::new("root.h"), source)
        .unwrap();
    config.record_file_origins = false;
    let plain = Preprocessor::new(config)
        .preprocess_str(Path::new("root.h"), source)
        .unwrap();
    assert_eq!(plain.source, recorded.source);
    assert_eq!(plain.macros, recorded.macros);
    assert!(plain.file_origins().is_none());
    let origins = recorded.file_origins().unwrap();
    let inner = recorded.source.find("inner").unwrap();
    let outer = recorded.source.find("outer").unwrap();
    assert_eq!(
        origins.source_file(inner),
        Some(Path::new("<builtin>/inner.h"))
    );
    assert_eq!(origins.source_file(outer), Some(Path::new("root.h")));
    assert_eq!(
        recorded.resolve_location(inner).unwrap().path.as_ref(),
        Path::new("diagnostic.h")
    );
    assert_eq!(
        origins.macro_definition("INNER").unwrap().path.as_ref(),
        Path::new("<builtin>/inner.h")
    );
    assert_eq!(origins.macro_definition("REPLACED").unwrap().line, 5);
    assert!(origins.macro_definition("ERASED").is_none());
    assert!(origins.source_file(recorded.source.len()).is_none());
    assert!(
        origins
            .mappings()
            .windows(2)
            .all(|pair| pair[0].generated().end <= pair[1].generated().start)
    );
}

#[test]
fn file_origin_catalog_resets_and_preserves_unsupported_pragma_diagnostics() {
    let mut config = Config {
        record_file_origins: true,
        ..Default::default()
    };
    config.defines.insert("COMMAND_LINE".into(), "1".into());
    let mut processor = Preprocessor::new(config);
    let first = processor
        .preprocess_str(Path::new("first.h"), "#define ONE 1\nint first;\n")
        .unwrap();
    let second = processor
        .preprocess_str(Path::new("second.h"), "int second;\n")
        .unwrap();
    assert!(
        first
            .file_origins()
            .unwrap()
            .macro_definition("ONE")
            .is_some()
    );
    assert!(
        second
            .file_origins()
            .unwrap()
            .macro_definition("ONE")
            .is_none()
    );
    assert!(
        first
            .file_origins()
            .unwrap()
            .macro_definition("COMMAND_LINE")
            .is_none()
    );
    assert_eq!(
        second.file_origins().unwrap().source_file(0),
        Some(Path::new("second.h"))
    );
    for pragma in ["push_macro", "pop_macro"] {
        let source = format!("#pragma {pragma}(\"ONE\")\n");
        let error = processor
            .preprocess_str(Path::new("unsupported.h"), &source)
            .unwrap_err();
        assert!(error.to_string().contains("unsupported pragma"));
    }
}

#[test]
fn preserved_directives_keep_physical_origins_without_overlapping_expanded_pragmas() {
    let result = Preprocessor::new(Config {record_file_origins:true,..Default::default()})
        .preprocess_str(Path::new("packing.h"), "#line 99 \"logical.h\"\n#pragma pack(push, 1)\nstruct S { int x; };\n_Pragma(\"pack(pop)\")\n")
        .unwrap();
    let origins = result.file_origins().unwrap();
    for offset in 0..result.source.len() {
        assert_eq!(origins.source_file(offset), Some(Path::new("packing.h")));
    }
    assert!(
        origins
            .mappings()
            .windows(2)
            .all(|pair| pair[0].generated().end <= pair[1].generated().start)
    );
}

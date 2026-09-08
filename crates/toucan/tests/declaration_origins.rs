use std::path::Path;
use toucan::semantic::DeclarationTarget;
use toucan::{Config, parse_source};

#[test]
fn declaration_origins_resolve_to_physical_invocation_headers() {
    let mut config = Config::new(toucan::Target::X86_64UnknownLinuxGnu);
    config.analysis.retain_declaration_origins = true;
    config.preprocessor.record_file_origins = true;
    config.preprocessor.allow_filesystem = false;
    config.preprocessor.virtual_headers.insert(
        "definition.h".into(),
        "#define DECLARE(name) extern int name;\nstruct Shared;\n".into(),
    );
    config.preprocessor.virtual_headers.insert(
        "invocation.h".into(),
        "#line 90 \"logical.h\"\nDECLARE(exported)\nstruct Shared { int value; };\n".into(),
    );
    let source = "#include \"definition.h\"\n#include \"invocation.h\"\nextern int exported;\n";
    let compilation = parse_source(Path::new("root.h"), source, &config).unwrap();
    assert!(compilation.checked().is_none());
    let origins = compilation.declaration_origins().unwrap();
    let files = compilation.preprocessed().file_origins().unwrap();
    let exported: Vec<_> = origins.entries().iter().filter(|origin| {
        matches!(origin.target(), DeclarationTarget::Declaration(id) if compilation.unit().declarations[id].name == "exported")
    }).collect();
    assert_eq!(exported.len(), 2);
    assert_eq!(exported[0].target(), exported[1].target());
    assert_eq!(
        files.source_file(exported[0].source().range().start),
        Some(Path::new("<builtin>/invocation.h"))
    );
    assert_eq!(
        files.source_file(exported[1].source().range().start),
        Some(Path::new("root.h"))
    );
    assert_eq!(
        compilation
            .preprocessed()
            .resolve_location(exported[0].source().range().start)
            .unwrap()
            .path
            .as_ref(),
        Path::new("logical.h")
    );
    assert_eq!(
        files.macro_definition("DECLARE").unwrap().path.as_ref(),
        Path::new("<builtin>/definition.h")
    );
    for origin in origins.entries() {
        assert!(!origin.source().synthetic());
        let range = origin.source().range();
        assert!(range.end <= compilation.preprocessed().source.len());
        assert!(files.source_file(range.start).is_some());
    }
}

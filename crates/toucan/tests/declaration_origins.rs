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

#[test]
fn ordered_file_entry_preserves_each_header_origin_without_checked_graphs() {
    let path = std::env::temp_dir().join(format!("toucan-ordered-origins-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("first.h"),
        "#define LENGTH 3\ntypedef int First;\n",
    )
    .unwrap();
    std::fs::write(path.join("main.h"), "extern First values[LENGTH];\n").unwrap();
    let paths = [path.join("first.h"), path.join("main.h")];
    for profile in toucan::CompilerProfile::ALL {
        let mut config = Config::with_profile(profile);
        config.analysis.retain_declaration_origins = true;
        config.preprocessor.record_file_origins = true;
        let result = toucan::parse_files(&paths, &config).unwrap();
        assert!(result.checked().is_none());
        let origins = result.declaration_origins().unwrap();
        let files = result.preprocessed().file_origins().unwrap();
        for (name, expected) in [("First", &paths[0]), ("values", &paths[1])] {
            let declaration = result
                .unit()
                .declarations
                .iter()
                .position(|item| item.name == name)
                .unwrap();
            let origin = origins
                .entries()
                .iter()
                .find(|origin| origin.target() == DeclarationTarget::Declaration(declaration))
                .unwrap();
            assert_eq!(
                files
                    .source_name(origin.source().range().start)
                    .unwrap()
                    .as_os_str(),
                expected.as_os_str()
            );
        }
    }
    std::fs::remove_dir_all(path).unwrap();
}

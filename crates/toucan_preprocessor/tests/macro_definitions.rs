use std::path::{Path, PathBuf};

use toucan_preprocessor::{Config, ForcedInclude, OriginKind, Preprocessor};

#[test]
fn history_preserves_written_definitions_without_changing_the_final_environment() {
    let source = "#define VALUE 1\n#define ALIAS VALUE\n#undef VALUE\n#define VALUE 2\n#undef ALIAS\n#if 0\n#define SKIPPED 3\n#endif\n#define CALL(x, ...) x + __VA_ARGS__\n#define EMPTY\n#define WORD 4\n#define WORD 4\nVALUE\n";
    let mut config = Config {
        allow_filesystem: false,
        ..Config::default()
    };
    config.defines.insert("COMMANDLINE".into(), "7".into());
    let ordinary = Preprocessor::new(config.clone())
        .preprocess_str(Path::new("input.h"), source)
        .unwrap();
    assert!(ordinary.macro_definitions().is_none());
    config.record_macro_definitions = true;
    let output = Preprocessor::new(config)
        .preprocess_str(Path::new("input.h"), source)
        .unwrap();
    assert_eq!(output.source, ordinary.source);
    assert_eq!(output.macros, ordinary.macros);
    assert!(output.file_origins().is_none());
    let entries = output.macro_definitions().unwrap();
    assert_eq!(
        entries.iter().map(|entry| entry.name()).collect::<Vec<_>>(),
        ["VALUE", "ALIAS", "VALUE", "CALL", "EMPTY", "WORD", "WORD"]
    );
    assert_eq!(entries[0].definition().replacement, "1");
    assert_eq!(entries[1].definition().replacement, "VALUE");
    assert_eq!(entries[2].definition().replacement, "2");
    assert_eq!(
        entries[3].definition().parameters.as_deref(),
        Some(["x".into()].as_slice())
    );
    assert!(entries[3].definition().variadic);
    assert_eq!(
        entries[3].definition().variadic_parameter.as_deref(),
        Some("__VA_ARGS__")
    );
    assert!(entries[4].definition().replacement.is_empty());
    assert_eq!(
        output.expand_object_macro("VALUE").unwrap().as_deref(),
        Some("2")
    );
    assert!(output.expand_object_macro("ALIAS").unwrap().is_none());
    assert_eq!(
        output
            .expand_object_macro("COMMANDLINE")
            .unwrap()
            .as_deref(),
        Some("7")
    );
    assert_eq!(
        output.clone().macro_definitions().unwrap()[0]
            .definition()
            .replacement,
        "1"
    );
}

#[test]
fn forced_and_virtual_headers_keep_physical_locations_and_exact_names() {
    let mut config = Config {
        allow_filesystem: false,
        record_macro_definitions: true,
        record_file_origins: true,
        ..Config::default()
    };
    config.forced_includes.push(ForcedInclude {
        path: PathBuf::from("headers/./forced.h"),
        source: "#define FIRST 1\n".into(),
    });
    config
        .virtual_headers
        .insert("virtual.h".into(), "#define SECOND 2\n".into());
    let output = Preprocessor::new(config)
        .preprocess_str(
            Path::new("headers/./main.h"),
            "#include <virtual.h>\n#line 900 \"logical.h\"\n#define THIRD 3\n#undef FIRST\n",
        )
        .unwrap();
    let entries = output.macro_definitions().unwrap();
    assert_eq!(entries.len(), 3);
    for (entry, name, path, line) in [
        (&entries[0], "FIRST", "headers/./forced.h", 1),
        (&entries[1], "SECOND", "<builtin>/virtual.h", 1),
        (&entries[2], "THIRD", "headers/./main.h", 3),
    ] {
        assert_eq!(entry.name(), name);
        assert_eq!(
            entry.accessed_path().as_os_str(),
            Path::new(path).as_os_str()
        );
        assert_eq!(
            entry.location().path.as_os_str(),
            Path::new(path).as_os_str()
        );
        assert_eq!(entry.location().line, line);
        assert_eq!(entry.location().column, 9);
        assert_eq!(entry.location().kind, OriginKind::Directive);
    }
    assert!(
        output
            .file_origins()
            .unwrap()
            .macro_definition("FIRST")
            .is_none()
    );
    assert_eq!(
        output
            .file_origins()
            .unwrap()
            .macro_definition("THIRD")
            .unwrap()
            .line,
        3
    );
}

#[test]
fn a_new_run_resets_history_after_success_or_failure() {
    let mut preprocessor = Preprocessor::new(Config {
        allow_filesystem: false,
        record_macro_definitions: true,
        ..Config::default()
    });
    let first = preprocessor
        .preprocess_str(Path::new("first.h"), "#define FIRST 1\n")
        .unwrap();
    assert_eq!(first.macro_definitions().unwrap().len(), 1);
    assert!(
        preprocessor
            .preprocess_str(Path::new("bad.h"), "#define BAD 2\n#error stop\n")
            .is_err()
    );
    let empty = preprocessor
        .preprocess_str(Path::new("empty.h"), "")
        .unwrap();
    assert!(empty.macro_definitions().unwrap().is_empty());
    assert!(empty.macros.is_empty());
    assert_eq!(first.macro_definitions().unwrap()[0].name(), "FIRST");
}

#[test]
fn capture_has_a_checked_retained_data_budget() {
    let source = "#define VALUE 1\n";
    let mut config = Config {
        allow_filesystem: false,
        max_source_bytes: 64,
        ..Config::default()
    };
    assert!(
        Preprocessor::new(config.clone())
            .preprocess_str(Path::new("input.h"), source)
            .is_ok()
    );
    config.record_macro_definitions = true;
    let error = Preprocessor::new(config.clone())
        .preprocess_str(Path::new("input.h"), source)
        .unwrap_err();
    assert_eq!(
        error.message,
        "macro definition capture byte limit exceeded"
    );
    config.max_source_bytes = 4096;
    assert!(
        Preprocessor::new(config)
            .preprocess_str(Path::new("input.h"), source)
            .is_ok()
    );
}

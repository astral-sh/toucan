use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use toucan::{Compilation, Config, Error, OriginKind, SemanticError, Target};

fn semantic_error(result: Result<Compilation, Error>) -> SemanticError {
    match result {
        Err(Error::Semantic(error)) => error,
        Err(error) => panic!("expected a semantic diagnostic, got {error}"),
        Ok(_) => panic!("expected a semantic diagnostic"),
    }
}

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "toucan-diagnostics-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn syntax_errors_resolve_to_nested_filesystem_headers() {
    let directory = Directory::new();
    std::fs::create_dir(directory.0.join("nested")).unwrap();
    std::fs::write(directory.0.join("main.h"), "#include \"outer.h\"\n").unwrap();
    std::fs::write(directory.0.join("outer.h"), "#include \"nested/inner.h\"\n").unwrap();
    let inner = directory.0.join("nested/inner.h");
    std::fs::write(&inner, "\n\nint broken(;\n").unwrap();
    let config = Config::new(Target::X86_64UnknownLinuxGnu);
    let diagnostic = semantic_error(toucan::parse_file(&directory.0.join("main.h"), &config));
    let origin = diagnostic.origin.as_ref().unwrap();
    assert_eq!(origin.path.as_ref(), std::fs::canonicalize(inner).unwrap());
    assert_eq!(
        (origin.line, origin.column, origin.kind),
        (3, 12, OriginKind::Token)
    );
    assert!(diagnostic.error.message.contains("C syntax error"));
    assert_eq!(diagnostic.error.offset, 13);
    assert!(diagnostic.to_string().contains("inner.h:3:12:"));
    assert!(diagnostic.to_string().contains("preprocessed byte 13"));
    let source = std::error::Error::source(&diagnostic).unwrap();
    assert_eq!(
        source
            .downcast_ref::<toucan::semantic::Error>()
            .unwrap()
            .offset,
        13
    );
}

#[test]
fn macro_semantic_errors_resolve_to_the_outer_invocation() {
    let mut config = Config::new(Target::X86_64UnknownLinuxGnu);
    config.preprocessor.allow_filesystem = false;
    config.preprocessor.virtual_headers.insert(
        "macros.h".into(),
        "#define INNER(name) void name\n#define OUTER(name) INNER(name)\n".into(),
    );
    let diagnostic = semantic_error(toucan::parse_source(
        Path::new("consumer.h"),
        "#include <macros.h>\nint good;\n  OUTER(bad);\n",
        &config,
    ));
    let origin = diagnostic.origin.as_ref().unwrap();
    assert_eq!(origin.path.as_ref(), Path::new("consumer.h"));
    assert_eq!(
        (origin.line, origin.column, origin.kind),
        (3, 3, OriginKind::MacroInvocation)
    );
    assert!(diagnostic.error.message.contains("void type"));
    assert!(diagnostic.error.offset > 0);
    assert!(diagnostic.to_string().contains("macro invocation"));
    assert!(!diagnostic.to_string().contains("macros.h"));
}

#[test]
fn virtual_header_and_line_directive_names_are_preserved() {
    let mut config = Config::new(Target::X86_64UnknownLinuxGnu);
    config.preprocessor.allow_filesystem = false;
    config.preprocessor.virtual_headers.insert(
        "generated.h".into(),
        "#line 70 \"schema.h\"\nint broken(;\n".into(),
    );
    let diagnostic = semantic_error(toucan::parse_source(
        Path::new("main.h"),
        "#include <generated.h>\n",
        &config,
    ));
    let origin = diagnostic.origin.as_ref().unwrap();
    assert_eq!(
        (origin.path.as_ref(), origin.line, origin.column),
        (Path::new("schema.h"), 70, 12)
    );
}

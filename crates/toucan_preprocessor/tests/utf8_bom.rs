use std::path::Path;

use toucan_preprocessor::{
    CommandLineMacroNormalizer, Config, DocumentationOptions, ForcedInclude, PredefinedMacroMode,
    Preprocessor,
};

const SOURCE: &str = "\u{feff}#define SIZE 4\n#include \"inner.h\"\n#include \"inner.h\"\nint values[SIZE + FORCED];\nconst char *marker = \"\u{feff}\";\n";
const INNER: &str = "\u{feff}#pragma once\nint included;\n";
const FORCED: &str = "\u{feff}#define FORCED 1\n";

fn compact(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}

fn config() -> Config {
    Config {
        allow_filesystem: false,
        virtual_headers: [("inner.h".into(), INNER.into())].into(),
        forced_includes: vec![ForcedInclude {
            path: "forced.h".into(),
            source: FORCED.into(),
        }],
        ..Config::default()
    }
}

#[test]
fn each_source_accepts_one_leading_utf8_bom() {
    let result = Preprocessor::new(config())
        .preprocess_str(Path::new("input.h"), SOURCE)
        .unwrap();
    assert_eq!(
        compact(&result.source),
        "intincluded;intvalues[4+1];constchar*marker=\"\u{feff}\";"
    );
    for source in ["\u{feff}\u{feff}int value;", "\n\u{feff}int value;"] {
        assert!(
            Preprocessor::new(Config::default())
                .preprocess_str(Path::new("input.h"), source)
                .is_err()
        );
    }
    for mode in [
        PredefinedMacroMode::GnuCommandLine,
        PredefinedMacroMode::ClangCommandLine,
    ] {
        let config = Config {
            predefined_macro_mode: mode,
            ..Config::default()
        };
        let (name, _) = CommandLineMacroNormalizer::new(&config)
            .prepare("\u{feff}NAME", "/* comment */ 1")
            .unwrap();
        assert_eq!(name, "\u{feff}NAME");
    }
}

#[test]
fn bom_removal_preserves_physical_offsets_and_lines() {
    for ending in ["\n", "\r\n", "\r"] {
        let source = format!(
            "\u{feff}/** input */ int first = __LINE__;{ending}int second = __LINE__;{ending}"
        );
        let result = Preprocessor::new(Config {
            documentation: Some(DocumentationOptions::default()),
            ..Config::default()
        })
        .preprocess_str(Path::new("input.h"), &source)
        .unwrap();
        assert_eq!(compact(&result.source), "intfirst=1;intsecond=2;");
        let docs = result.documentation().unwrap();
        let (_, file) = docs.sources().next().unwrap();
        let comment = &file.comments()[0];
        assert_eq!(comment.range().start, 3);
        assert_eq!(&source[comment.range().clone()], comment.text());
        let offset = result.source.find("first").unwrap();
        let original = source.find("first").unwrap();
        assert_eq!(
            docs.resolve(offset).unwrap().invocation().unwrap().offset(),
            original
        );
        let location = result.resolve_location(offset).unwrap();
        assert_eq!((location.line, location.column), (1, original + 1));
        let second = result
            .resolve_location(result.source.find("second").unwrap())
            .unwrap();
        assert_eq!((second.line, second.column), (2, 5));
    }
}

#[test]
#[ignore = "requires a native C compiler (set CC)"]
fn native_bom_headers_match() {
    use std::process::Command;

    let directory = std::env::temp_dir().join(format!("toucan-utf8-bom-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let main = directory.join("input.h");
    std::fs::write(&main, SOURCE).unwrap();
    std::fs::write(directory.join("inner.h"), INNER).unwrap();
    let forced = directory.join("forced.h");
    std::fs::write(&forced, FORCED).unwrap();
    let output = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .args(["-E", "-P", "-std=c11", "-include"])
        .arg(&forced)
        .args(["-x", "c"])
        .arg(&main)
        .output()
        .unwrap();
    assert_eq!(
        toucan_test_support::compiler_acceptance(&output),
        Ok(true),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = Preprocessor::new(Config {
        allow_filesystem: true,
        virtual_headers: Default::default(),
        ..config()
    })
    .preprocess(&main)
    .unwrap();
    assert_eq!(
        compact(&result.source),
        compact(&String::from_utf8(output.stdout).unwrap())
    );
    std::fs::remove_dir_all(directory).unwrap();
}

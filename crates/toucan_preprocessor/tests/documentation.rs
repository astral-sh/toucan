use std::path::Path;
use toucan_preprocessor::{Config, DocumentationOptions, Preprocessor};

fn config() -> Config {
    Config {
        documentation: Some(DocumentationOptions::default()),
        ..Default::default()
    }
}

#[test]
fn raw_comments_keep_physical_groups_and_no_doc_inputs_have_no_catalog() {
    let source = "/** first */\n/** second */\ntypedef int T;\nint field; /**< trailing */\nconst char *s=\"/** literal */\";\n";
    let output = Preprocessor::new(config())
        .preprocess_str(Path::new("input.h"), source)
        .unwrap();
    let docs = output.documentation().unwrap();
    let (id, file) = docs.sources().next().unwrap();
    assert_eq!(file.comments().len(), 2);
    assert_eq!(file.comments()[0].text(), "/** first */\n/** second */");
    assert_eq!(file.comments()[1].text(), "/**< trailing */");
    assert!(file.comments()[1].is_trailing());
    assert_eq!(
        &source[file.comments()[0].range().clone()],
        file.comments()[0].text()
    );
    let offset = output.source.find("typedef").unwrap();
    let origin = docs.resolve(offset).unwrap();
    assert_eq!(origin.invocation().unwrap().source(), id);
    assert_eq!(
        origin.invocation().unwrap().offset(),
        source.find("typedef").unwrap()
    );
    assert_eq!(origin.invocation().unwrap().line(), 3);
    assert!(origin.spelling().is_none());
    assert!(
        Preprocessor::new(Config::default())
            .preprocess_str(Path::new("input.h"), source)
            .unwrap()
            .documentation()
            .is_none()
    );
    assert!(
        Preprocessor::new(config())
            .preprocess_str(Path::new("input.h"), "// ordinary\nint value;\n")
            .unwrap()
            .documentation()
            .is_none()
    );
}

#[test]
fn macro_output_keeps_invocation_and_final_replacement_coordinates() {
    let source = "#define TYPE /** TYPE */ int\n#define DECL /** DECL */ typedef TYPE Name;\n/** INVOCATION */\nDECL\n";
    let output = Preprocessor::new(config())
        .preprocess_str(Path::new("input.h"), source)
        .unwrap();
    let docs = output.documentation().unwrap();
    let int_offset = output.source.find("int").unwrap();
    let origin = docs.resolve(int_offset).unwrap();
    assert_eq!(
        origin.invocation().unwrap().offset(),
        source.rfind("DECL").unwrap()
    );
    assert_eq!(
        origin.spelling().unwrap().offset(),
        source.find(" int").unwrap() + 1
    );
    let name = docs.resolve(output.source.find("Name").unwrap()).unwrap();
    assert_eq!(name.invocation(), origin.invocation());
    assert_eq!(
        name.spelling().unwrap().offset(),
        source.find("Name").unwrap()
    );
    assert_eq!(output.macros["TYPE"].replacement, "int");
    assert!(!output.macros["DECL"].replacement.contains("/**"));
}

#[test]
fn pop_macro_restores_original_replacement_spelling() {
    let source = concat!(
        "#define X /** original */ 1\n",
        "#pragma push_macro(\"X\")\n",
        "#undef X\n#define X /** temporary */ 2\n",
        "X\n#pragma pop_macro(\"X\")\nX\n",
    );
    let output = Preprocessor::new(config())
        .preprocess_str(Path::new("macro.h"), source)
        .unwrap();
    assert_eq!(output.source, "2\n1\n");
    let docs = output.documentation().unwrap();
    assert_eq!(
        docs.resolve(output.source.find('2').unwrap())
            .unwrap()
            .spelling()
            .unwrap()
            .offset(),
        source.find(" 2\n").unwrap() + 1
    );
    assert_eq!(
        docs.resolve(output.source.find('1').unwrap())
            .unwrap()
            .spelling()
            .unwrap()
            .offset(),
        source.find(" 1\n").unwrap() + 1
    );
}

#[test]
fn physical_splices_and_line_remapping_do_not_change_comment_spelling() {
    let source = "/** physical\\\n spelling */\n#line 70 \"logical.h\"\nint value;\n";
    let output = Preprocessor::new(config())
        .preprocess_str(Path::new("physical.h"), source)
        .unwrap();
    let docs = output.documentation().unwrap();
    let (_, file) = docs.sources().next().unwrap();
    assert_eq!(file.comments()[0].text(), "/** physical\\\n spelling */");
    let offset = output.source.find("value").unwrap();
    assert_eq!(
        docs.resolve(offset).unwrap().invocation().unwrap().line(),
        4
    );
    assert_eq!(output.resolve_location(offset).unwrap().line, 70);
    assert_eq!(
        output.resolve_location(offset).unwrap().path.as_ref(),
        Path::new("logical.h")
    );
    let mut options = config();
    options.documentation.as_mut().unwrap().parse_all_comments = true;
    assert!(
        Preprocessor::new(options)
            .preprocess_str(Path::new("input.h"), "/* ordinary */ int value;")
            .unwrap()
            .documentation()
            .is_some()
    );
    for source in [
        "/\\\n** transformed */ int value;",
        "const char *s=\"/** not a comment */\";",
    ] {
        assert!(
            Preprocessor::new(config())
                .preprocess_str(Path::new("input.h"), source)
                .unwrap()
                .documentation()
                .is_none()
        );
    }
}

#[test]
fn marker_regions_and_main_pragma_exception_are_retained() {
    let source = "/** user */\nint first;\n#pragma GCC system_header\n/** still user */\nint second;\n# 10 \"system.h\" 3\n/** system */\nint third;\n# 20 \"user.h\"\n/** user again */\nint fourth;\n";
    let output = Preprocessor::new(config())
        .preprocess_str(Path::new("main.h"), source)
        .unwrap();
    let docs = output.documentation().unwrap();
    let (_, file) = docs.sources().next().unwrap();
    assert_eq!(
        file.comments()
            .iter()
            .map(|comment| file.is_system_at(comment.range().start).unwrap())
            .collect::<Vec<_>>(),
        [false, false, true, false]
    );
}

#[test]
fn capture_budget_failure_and_reuse_do_not_leak_prior_sources() {
    let mut options = config();
    options.max_source_bytes = 128;
    let error = Preprocessor::new(options)
        .preprocess_str(Path::new("small.h"), "/** docs */ int value;")
        .unwrap_err();
    assert!(error.message.contains("documentation"), "{error}");
    let mut pp = Preprocessor::new(config());
    assert!(
        pp.preprocess_str(Path::new("first.h"), "/** docs */ int value;")
            .unwrap()
            .documentation()
            .is_some()
    );
    assert!(
        pp.preprocess_str(Path::new("second.h"), "int value;")
            .unwrap()
            .documentation()
            .is_none()
    );
}

#[test]
fn physical_closing_markers_and_ordinary_trailing_groups_follow_raw_spelling() {
    for source in [
        "/** escaped close *\\\n/ int value;",
        "/??/\n** escaped open */ int value;",
    ] {
        assert!(
            Preprocessor::new(config())
                .preprocess_str(Path::new("input.h"), source)
                .unwrap()
                .documentation()
                .is_none()
        );
    }
    let mut options = config();
    options.documentation.as_mut().unwrap().parse_all_comments = true;
    let source = "int first; // first\n           // continuation\n// next\nint second;\n";
    let output = Preprocessor::new(options)
        .preprocess_str(Path::new("input.h"), source)
        .unwrap();
    let (_, file) = output.documentation().unwrap().sources().next().unwrap();
    assert_eq!(file.comments().len(), 2);
    assert_eq!(
        file.comments()[0].text(),
        "// first\n           // continuation"
    );
    assert!(file.comments()[0].is_trailing());
    assert!(!file.comments()[1].is_trailing());
}

#[test]
fn expanded_system_pragma_affects_nested_reads_and_restores_the_parent() {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    struct Files(std::path::PathBuf);
    impl Drop for Files {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let files = Files(std::env::temp_dir().join(format!(
        "toucan-doc-systems-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&files.0).unwrap();
    std::fs::write(
        files.0.join("root.h"),
        "#include \"parent.h\"\n#include \"child.h\"\n/** main */\nint root;\n",
    )
    .unwrap();
    std::fs::write(files.0.join("parent.h"),"/** before */\nint before;\n#define SYSTEM _Pragma(\"GCC system_header\")\nSYSTEM\n/** after */\nint after;\n#include \"child.h\"\n").unwrap();
    std::fs::write(files.0.join("child.h"), "/** child */\nint child;\n").unwrap();
    let output = Preprocessor::new(config())
        .preprocess(&files.0.join("root.h"))
        .unwrap();
    let docs = output.documentation().unwrap();
    let flags: Vec<_> = docs
        .sources()
        .flat_map(|(_, source)| {
            source.comments().iter().map(|comment| {
                (
                    comment.text().to_owned(),
                    source.is_system_at(comment.range().start).unwrap(),
                )
            })
        })
        .collect();
    assert_eq!(
        flags,
        [
            ("/** main */".into(), false),
            ("/** before */".into(), false),
            ("/** after */".into(), true),
            ("/** child */".into(), true),
            ("/** child */".into(), false)
        ]
    );
}

#[test]
fn configured_macro_origins_remain_distinct_without_spelling_locations() {
    let mut config = Config {
        documentation: Some(DocumentationOptions::default()),
        ..Config::default()
    };
    config
        .defines
        .insert("DECL".into(), "struct Owner { int field; };".into());
    let expanded = Preprocessor::new(config)
        .preprocess_str(Path::new("input.h"), "/** OWNER */\nDECL\n")
        .unwrap();
    let origin = expanded
        .documentation()
        .unwrap()
        .resolve(expanded.source.find("field").unwrap())
        .unwrap();
    assert!(origin.is_macro());
    assert!(origin.invocation().is_some());
    assert!(origin.spelling().is_none());
    let direct = Preprocessor::new(Config {
        documentation: Some(DocumentationOptions::default()),
        ..Config::default()
    })
    .preprocess_str(
        Path::new("input.h"),
        "/** OWNER */ struct Owner { int field; }; ",
    )
    .unwrap();
    assert!(
        !direct
            .documentation()
            .unwrap()
            .resolve(direct.source.find("field").unwrap())
            .unwrap()
            .is_macro()
    );
}

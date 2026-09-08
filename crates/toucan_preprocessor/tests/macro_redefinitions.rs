use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use toucan_preprocessor::{
    Config, ForcedInclude, MacroRedefinition, MacroRedefinitionPolicy, OriginKind, Preprocessor,
};
use toucan_test_support::compiler_acceptance;

fn compatible() -> Config {
    Config {
        allow_filesystem: false,
        macro_redefinition_policy: MacroRedefinitionPolicy::RecordAndReplace,
        ..Config::default()
    }
}

#[test]
fn default_stays_strict_and_opt_in_preserves_history_and_latest_expansion() {
    let input = "#define VALUE 1\n#define ALIAS VALUE\n#define VALUE 2\n#define VALUE 2\nALIAS\n";
    let error = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("input.h"), input)
        .unwrap_err();
    assert_eq!(error.message, "incompatible redefinition of macro `VALUE`");
    let config = Config {
        record_macro_definitions: true,
        ..compatible()
    };
    let output = Preprocessor::new(config)
        .preprocess_str(Path::new("input.h"), input)
        .unwrap();
    assert_eq!(output.source.trim(), "2");
    assert_eq!(output.macros["VALUE"].replacement, "2");
    let records = output.macro_redefinitions().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].name(), "VALUE");
    let location = records[0].location().unwrap();
    assert_eq!(location.path.as_ref(), Path::new("input.h"));
    assert_eq!((location.line, location.column), (3, 9));
    assert_eq!(location.kind, OriginKind::Directive);
    assert_eq!(records[0].accessed_path(), Some(Path::new("input.h")));
    assert_eq!(
        output
            .macro_definitions()
            .unwrap()
            .iter()
            .map(|entry| entry.definition().replacement.as_str())
            .collect::<Vec<_>>(),
        ["1", "VALUE", "2", "2"]
    );
    let strict = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("same.h"), "#define A 1\n#define A 1\nA\n")
        .unwrap();
    assert!(strict.macro_redefinitions().is_none());
}

#[test]
fn only_active_incompatible_definitions_produce_records() {
    let input = "#define F(x) ((x)+1)\n#define F(y) ((y)+1)\n#define F 7\n#undef F\n#define F 8\n#if 0\n#define F 9\n#endif\n#define A 1 + 2\n#define A 1/**/+/**/2\nF A\n";
    let output = Preprocessor::new(compatible())
        .preprocess_str(Path::new("input.h"), input)
        .unwrap();
    assert_eq!(
        output
            .macro_redefinitions()
            .unwrap()
            .iter()
            .map(MacroRedefinition::name)
            .collect::<Vec<_>>(),
        ["F", "F"]
    );
    assert_eq!(output.source.trim(), "8 1 + 2");
}

#[test]
fn current_location_stays_physical_across_forced_headers_and_line_markers() {
    let mut config = compatible();
    config.forced_includes.push(ForcedInclude {
        path: "headers/./forced.h".into(),
        source: "#define VALUE 1\n#line 90 \"logical.h\"\n#define VALUE 2\n".into(),
    });
    let output = Preprocessor::new(config)
        .preprocess_str(Path::new("input.h"), "#define VALUE 3\nVALUE\n")
        .unwrap();
    let records = output.macro_redefinitions().unwrap();
    assert_eq!(records.len(), 2);
    let location = records[0].location().unwrap();
    assert_eq!(location.path.as_ref(), Path::new("headers/./forced.h"));
    assert_eq!((location.line, location.column), (3, 9));
    assert_eq!(
        records[0].accessed_path().unwrap().as_os_str(),
        Path::new("headers/./forced.h").as_os_str()
    );
    assert_eq!(records[1].location().unwrap().line, 1);
}

#[test]
fn configured_redefinitions_have_no_invented_source_site() {
    let mut config = compatible();
    config.defines.insert("VALUE".into(), "1".into());
    config.defines.insert("VALUE(x)".into(), "(x)".into());
    let output = Preprocessor::new(config)
        .preprocess_str(Path::new("input.h"), "#define VALUE 3\nVALUE\n")
        .unwrap();
    let records = output.macro_redefinitions().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].name(), "VALUE");
    assert!(records[0].location().is_none());
    assert!(records[0].accessed_path().is_none());
    assert_eq!(
        records[1].location().unwrap().path.as_ref(),
        Path::new("input.h")
    );
}

#[test]
fn reset_clears_records_and_record_budget_after_success_or_failure() {
    let path = Path::new("input.h");
    let limit = size_of::<MacroRedefinition>() + "A".len() + 2 * path.as_os_str().len();
    let mut pp = Preprocessor::new(Config {
        max_source_bytes: limit,
        ..compatible()
    });
    let output = pp
        .preprocess_str(path, "#define A 1\n#define A 2\nA\n")
        .unwrap();
    assert_eq!(output.macro_redefinitions().unwrap().len(), 1);
    let empty = pp.preprocess_str(path, "#define A 3\nA\n").unwrap();
    assert!(empty.macro_redefinitions().unwrap().is_empty());
    let error = pp
        .preprocess_str(path, "#define A 1\n#define A 2\n#define A 3\n")
        .unwrap_err();
    assert_eq!(
        error.message,
        "macro redefinition record byte limit exceeded"
    );
    let output = pp
        .preprocess_str(path, "#define A 4\n#define A 5\nA\n")
        .unwrap();
    assert_eq!(output.source.trim(), "5");
    assert_eq!(output.macro_redefinitions().unwrap().len(), 1);
    let error = pp
        .preprocess_str(path, "#define A 1\n#define A ##\n")
        .unwrap_err();
    assert!(error.message.contains("`##` cannot begin"));
    let output = pp.preprocess_str(path, "#define A 6\nA\n").unwrap();
    assert!(output.macro_redefinitions().unwrap().is_empty());
}

#[test]
#[ignore = "requires native C preprocessors"]
fn compiler_redefinition_warnings_agree_with_records_and_expanded_values() {
    let input = Path::new("input.h");
    for source in [
        "#define A 1\n#define A 2\nA\n",
        "#define A 1\n#define A 1\nA\n",
        "#define A 1 + 2\n#define A 1/**/+/**/2\nA\n",
        "#define A 1 + 2\n#define A 1+2\nA\n",
        "#define A(x) ((x)+1)\n#define A(y) ((y)+1)\nA(3)\n",
        "#define A 1\n#define A(x) ((x)+1)\nA(3)\n",
        "#define A(x) ((x)+1)\n#define A 7\nA\n",
        "#define A 1\n#undef A\n#define A 2\nA\n",
        "#define A 1\n#if 0\n#define A 2\n#endif\nA\n",
    ] {
        let output = Preprocessor::new(compatible())
            .preprocess_str(input, source)
            .unwrap();
        for compiler in ["gcc", "clang"] {
            let mut child = Command::new(compiler)
                .args(["-E", "-P", "-x", "c", "-std=gnu11", "-"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(source.as_bytes())
                .unwrap();
            let native = child.wait_with_output().unwrap();
            assert_eq!(
                compiler_acceptance(&native),
                Ok(true),
                "{compiler}: {}",
                String::from_utf8_lossy(&native.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&native.stderr).contains("redefined"),
                !output.macro_redefinitions().unwrap().is_empty(),
                "{compiler}: {source}"
            );
            let native = String::from_utf8(native.stdout).unwrap();
            assert_eq!(
                native.split_whitespace().collect::<String>(),
                output.source.split_whitespace().collect::<String>(),
                "{compiler}: {source}"
            );
        }
    }
}

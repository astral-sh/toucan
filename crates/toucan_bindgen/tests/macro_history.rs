use std::path::Path;

use toucan_bindgen::callbacks::ParseCallbacks;
use toucan_bindgen::{Builder, Formatter, MacroTypeVariation};

fn builder(path: &Path) -> Builder {
    Builder::default()
        .header(path.to_str().unwrap())
        .clang_arg("--target=x86_64-unknown-linux-gnu")
        .formatter(Formatter::None)
        .layout_tests(false)
}

#[derive(Debug)]
struct Callbacks;
impl ParseCallbacks for Callbacks {}

#[test]
fn historical_values_use_first_output_and_latest_context() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.h");
    std::fs::write(&path, "#define A 1\n#define B A\n#undef A\n#define A 2\n#define C B\n#define D A\n#undef A\n#define E A\n#define FORWARD MISSING\n#define MISSING 9\n#define LATE MISSING\n#define CONFIGURED INPUT\n").unwrap();
    let bindings = builder(&path).clang_arg("-DINPUT=7").generate().unwrap();
    let source = bindings.to_string();
    for (name, value) in [
        ("A", 1),
        ("B", 1),
        ("C", 1),
        ("D", 2),
        ("E", 2),
        ("MISSING", 9),
        ("LATE", 9),
    ] {
        assert!(
            source.contains(&format!(
                "pub const {name}: ::core::primitive::u32 = {value};"
            )),
            "{source}"
        );
    }
    for name in ["FORWARD", "CONFIGURED", "INPUT", "_LP64"] {
        assert!(!source.contains(&format!("pub const {name}:")), "{source}");
    }
    assert_eq!(
        bindings.report().macro_evaluation,
        toucan::MacroEvaluation::Provided
    );
    assert!(bindings.report().macro_types.is_empty());
    assert!(
        bindings
            .report()
            .skipped_macros
            .iter()
            .any(|item| item.name == "CONFIGURED")
    );
}

#[test]
fn file_selection_uses_first_successful_definition_after_context_updates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("root.h");
    std::fs::write(
        directory.path().join("first.h"),
        "#define VALUE 1\n#define RECOVER unknown\n",
    )
    .unwrap();
    std::fs::write(directory.path().join("second.h"), "#line 900 \"logical.h\"\n#undef VALUE\n#define VALUE 2\n#define ALIAS VALUE\n#undef RECOVER\n#define RECOVER 3\n").unwrap();
    std::fs::write(&path, "#include \"first.h\"\n#include \"second.h\"\n").unwrap();
    let first = builder(&path)
        .allowlist_file(r".*[/\\]first\.h")
        .generate()
        .unwrap()
        .to_string();
    assert!(first.contains("pub const VALUE: ::core::primitive::u32 = 1;"));
    assert!(!first.contains("pub const ALIAS:"));
    let second = builder(&path)
        .allowlist_file(r".*[/\\]second\.h")
        .generate()
        .unwrap()
        .to_string();
    assert!(!second.contains("pub const VALUE:"));
    assert!(second.contains("pub const ALIAS: ::core::primitive::u32 = 2;"));
    assert!(second.contains("pub const RECOVER: ::core::primitive::u32 = 3;"));
    assert!(
        !builder(&path)
            .allowlist_file("logical.h")
            .generate()
            .unwrap()
            .to_string()
            .contains("pub const ")
    );
}

#[test]
fn callback_function_classification_uses_the_final_active_macro() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("function.h");
    for (tail, expected_a, expected_alias) in [
        ("", false, false),
        ("#undef A\n", true, true),
        ("#undef A\n#define A 11\n", true, true),
    ] {
        std::fs::write(
            &path,
            format!(
                "#define Known 9\n#define A 7\n#undef A\n#define A(Known)\n{tail}#define Alias A\n"
            ),
        )
        .unwrap();
        let source = builder(&path)
            .parse_callbacks(Box::new(Callbacks))
            .generate()
            .unwrap()
            .to_string();
        assert_eq!(source.contains("pub const A:"), expected_a, "{source}");
        assert_eq!(
            source.contains("pub const Alias:"),
            expected_alias,
            "{source}"
        );
        if expected_a {
            assert!(source.contains("pub const A: ::core::primitive::u32 = 7;"));
            let alias = if tail.contains("11") { 11 } else { 9 };
            assert!(source.contains(&format!(
                "pub const Alias: ::core::primitive::u32 = {alias};"
            )));
        }
    }
    std::fs::write(&path, "#define Known 9\n#define A(Known) -3\n").unwrap();
    assert!(
        builder(&path)
            .generate()
            .unwrap()
            .to_string()
            .contains("pub const A: ::core::primitive::u32 = 6;")
    );
}

#[test]
fn accepted_redefinitions_are_reported_and_unsafe_character_projection_is_omitted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("redefined.h");
    std::fs::write(&path, "#define VALUE 10\n#define VALUE 11\n#define ALIAS VALUE\n#define BYTE '\\xff'\n#define WIDE '\\x100'\n#define UNICODE L'\\u00e9'\n").unwrap();
    let bindings = builder(&path).generate().unwrap();
    let source = bindings.to_string();
    assert!(source.contains("pub const VALUE: ::core::primitive::u32 = 10;"));
    assert!(source.contains("pub const ALIAS: ::core::primitive::u32 = 11;"));
    assert!(source.contains("pub const BYTE: ::core::primitive::u8 = 255;"));
    assert_eq!(bindings.report().macro_redefinitions.len(), 1);
    for name in ["WIDE", "UNICODE"] {
        assert!(!source.contains(&format!("pub const {name}:")));
        assert!(
            bindings
                .report()
                .skipped_macros
                .iter()
                .any(|item| item.name == name && item.reason.contains("one-byte representation"))
        );
    }
}

#[test]
fn macro_type_settings_have_real_last_call_wins_behavior() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("types.h");
    std::fs::write(&path, "#define SMALL 1U\n#define NEGATIVE -1ULL\n#define LARGE 4294967295U\n#define WRAPPED 18446744073709551615ULL\n").unwrap();
    for (variation, fit, small, negative, large) in [
        (MacroTypeVariation::Unsigned, false, "u32", "i32", "u32"),
        (MacroTypeVariation::Signed, false, "i32", "i32", "i64"),
        (MacroTypeVariation::Unsigned, true, "u8", "i8", "u32"),
        (MacroTypeVariation::Signed, true, "i8", "i8", "i64"),
    ] {
        let source = builder(&path)
            .default_macro_constant_type(MacroTypeVariation::Signed)
            .default_macro_constant_type(variation)
            .fit_macro_constants(!fit)
            .fit_macro_constants(fit)
            .generate()
            .unwrap()
            .to_string();
        for (name, ty, value) in [
            ("SMALL", small, "1"),
            ("NEGATIVE", negative, "-1"),
            ("LARGE", large, "4294967295"),
            ("WRAPPED", negative, "-1"),
        ] {
            assert!(
                source.contains(&format!(
                    "pub const {name}: ::core::primitive::{ty} = {value};"
                )),
                "{source}"
            );
        }
    }
    for name in ["signed", "unsigned"] {
        assert_eq!(
            name.parse::<MacroTypeVariation>().unwrap().to_string(),
            name
        );
    }
    assert_eq!(
        "Signed".parse::<MacroTypeVariation>().unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
}

#[test]
fn resource_exhaustion_remains_an_error_for_excluded_macros() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("deep.h");
    std::fs::write(
        &path,
        format!("#define DEEP {}1{}\n", "(".repeat(256), ")".repeat(256)),
    )
    .unwrap();
    let error = builder(&path)
        .allowlist_file("absent.h")
        .generate()
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("macro evaluation resource limit"),
        "{error}"
    );
}

use std::path::Path;
use std::sync::Arc;

use toucan_preprocessor::{
    Config, FeatureQueries, FeatureQuery, FeatureQueryProvider, Preprocessor, QueryDialect,
};

#[derive(Debug)]
struct Catalog {
    namespace: bool,
}

impl FeatureQueryProvider for Catalog {
    fn query(&self, kind: FeatureQuery, namespace: Option<&str>, name: &str) -> u64 {
        if namespace.is_some_and(|name| name != "gnu" || !self.namespace) {
            return 0;
        }
        match kind {
            FeatureQuery::Builtin => u64::from(name == "__builtin_bswap32"),
            FeatureQuery::Attribute => u64::from(name == "aligned"),
            FeatureQuery::CAttribute => u64::from(namespace.is_some() && name == "aligned"),
            _ => 0,
        }
    }
}

fn config(dialect: QueryDialect, namespace: bool, scope_punctuator: bool) -> Config {
    Config {
        feature_queries: Some(FeatureQueries::new(
            dialect,
            Arc::new(Catalog { namespace }),
        )),
        scope_punctuator,
        allow_filesystem: false,
        ..Config::default()
    }
}

fn compact(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}

const ONCE_HEADER: &str = "#define ARG _Pragma(\"once\") aligned\nenum { available = __has_attribute(ARG) };\nstruct IncludedOnce { int value; };\n";
const INCLUDES: &str = "#include <once.h>\n#include <once.h>\ntypedef char SizeCheck[sizeof(struct IncludedOnce) == sizeof(int) ? 1 : -1];\n";

#[test]
fn query_pragmas_preserve_once_effects_and_reset_at_entry_points() {
    for dialect in [QueryDialect::Gnu, QueryDialect::Clang] {
        let mut config = config(dialect, true, true);
        config
            .virtual_headers
            .insert("once.h".into(), ONCE_HEADER.into());
        let mut pp = Preprocessor::new(config);
        for _ in 0..2 {
            let result = pp.preprocess_str(Path::new("main.c"), INCLUDES).unwrap();
            assert_eq!(result.source.matches("struct IncludedOnce {").count(), 1);
            assert!(compact(&result.source).contains("enum{available=1};"));
        }
    }
    let mut config = config(QueryDialect::Clang, true, true);
    config.virtual_headers.insert("once.h".into(), "#define ARG _Pragma(\"once\") aligned\n#if 0 && __has_attribute(ARG)\n#error unreachable\n#endif\nstruct IncludedOnce { int value; };\n".into());
    let result = Preprocessor::new(config)
        .preprocess_str(Path::new("main.c"), INCLUDES)
        .unwrap();
    assert_eq!(result.source.matches("struct IncludedOnce {").count(), 1);
}

fn conditional_macro_sources() -> Vec<String> {
    let mut sources = Vec::new();
    for expression in [
        "_Pragma(\"pop_macro(\\\"VALUE\\\")\") VALUE == 1",
        "RESTORE VALUE == 1",
        "PASS(RESTORE) VALUE == 1",
        "RESTORE PASS(VALUE) == 1",
        // Clang prescans VALUE before applying an argument's pragma.
        "PASS(RESTORE VALUE) == 2",
    ] {
        sources.push(format!(
            "#define VALUE 1\n#pragma push_macro(\"VALUE\")\n\
             #undef VALUE\n#define VALUE 2\n\
             #define RESTORE _Pragma(\"pop_macro(\\\"VALUE\\\")\")\n\
             #define PASS(x) x\n\
             #if 0\n#elif {expression}\nint correct;\n\
             #else\n#error stale macro value\n#endif\n\
             #if VALUE != 1\n#error missing pragma effect\n#endif\n"
        ));
    }
    sources.extend([
        "#pragma push_macro(\"__has_attribute\")\n#undef __has_attribute\n\
         #define __has_attribute(x) 0\n\
         #if _Pragma(\"pop_macro(\\\"__has_attribute\\\")\") __has_attribute(aligned)\n\
         int correct;\n#else\n#error stale feature query\n#endif\n"
            .into(),
        "#pragma push_macro(\"ABSENT\")\n#define ABSENT 1\n\
         #if defined(ABSENT) && _Pragma(\"pop_macro(\\\"ABSENT\\\")\") !defined ABSENT\n\
         int correct;\n#else\n#error stale defined result\n#endif\n"
            .into(),
        "#define HEADER \"available.h\"\n#pragma push_macro(\"HEADER\")\n\
         #undef HEADER\n#define HEADER \"missing.h\"\n\
         #define ID(x) x\n\
         #if !__has_include(ID(ID(HEADER))) && _Pragma(\"pop_macro(\\\"HEADER\\\")\") \
             __has_include(ID(ID(HEADER))) && __has_include_next(ID(ID(HEADER)))\n\
         int correct;\n#else\n#error stale header query\n#endif\n"
            .into(),
        "#pragma push_macro(\"ABSENT\")\n#define ABSENT 1\n\
         #if 0 && _Pragma(\"pop_macro(\\\"ABSENT\\\")\") 1\n#error evaluated branch\n#endif\n\
         #if !defined(ABSENT)\nint correct;\n#else\n#error missing unevaluated pragma\n#endif\n"
            .into(),
        "#define VALUE 1\n#pragma push_macro(\"VALUE\")\n#undef VALUE\n#define VALUE 2\n\
         #if 0\n#if _Pragma(\"pop_macro(\\\"VALUE\\\")\") 1\n#endif\n\
         #elif 1\n#elif _Pragma(\"pop_macro(\\\"VALUE\\\")\") 1\n#endif\n\
         #if VALUE == 2\nint correct;\n#else\n#error applied skipped pragma\n#endif\n"
            .into(),
    ]);
    sources
}

#[test]
fn conditional_macro_restoration_precedes_following_expansion() {
    let mut config = config(QueryDialect::Clang, true, true);
    config
        .virtual_headers
        .insert("available.h".into(), String::new());
    let mut pp = Preprocessor::new(config);
    for source in conditional_macro_sources() {
        let result = pp
            .preprocess_str(Path::new("condition.h"), &source)
            .unwrap();
        assert_eq!(compact(&result.source), "intcorrect;", "{source}");
    }
}

#[test]
fn raw_queries_and_scope_lookahead_do_not_consume_deferred_pragmas() {
    for query in [
        "__has_builtin",
        "__has_feature",
        "__has_extension",
        "__building_module",
    ] {
        let config = config(QueryDialect::Clang, true, true);
        let mut pp = Preprocessor::new(config);
        let direct = format!("#define ARG _Pragma(\"once\") aligned\nint value = {query}(ARG);\n");
        assert_eq!(
            compact(
                &pp.preprocess_str(Path::new("query.c"), &direct)
                    .unwrap()
                    .source
            ),
            "intvalue=0;"
        );
        let wrapper = format!(
            "#define ARG _Pragma(\"once\") aligned\n#define W(x) {query}(x)\nint value = W(ARG);\n"
        );
        assert!(pp.preprocess_str(Path::new("query.c"), &wrapper).is_err());
    }
    for source in [
        "#define ARG gnu _Pragma(\"once\") :: aligned\nint value = __has_c_attribute(ARG);\n",
        "#define W(x) __has_c_attribute(x)\nint value = W(gnu _Pragma(\"once\") :: aligned);\n",
        "#define W(x) __has_c_attribute(x)\nint value = W(aligned _Pragma(\"once\"));\n",
    ] {
        assert!(
            Preprocessor::new(config(QueryDialect::Clang, true, true))
                .preprocess_str(Path::new("query.c"), source)
                .is_err()
        );
    }
}

#[test]
fn query_pragmas_keep_compiler_constraints_and_final_environment_limits() {
    for dialect in [QueryDialect::Gnu, QueryDialect::Clang] {
        let mut pp = Preprocessor::new(config(dialect, true, true));
        let pack = "#define ARG _Pragma(\"pack(1)\") aligned\nint value = __has_attribute(ARG);\n";
        let error = pp.preprocess_str(Path::new("query.c"), pack).unwrap_err();
        assert!(
            error
                .message
                .contains("unsupported pragma in feature-query argument")
        );
        let result = pp
            .preprocess_str(
                Path::new("query.c"),
                "#define VALUE __has_attribute(_Pragma(\"once\") aligned)\n",
            )
            .unwrap();
        assert!(
            result
                .expand_object_macro("VALUE")
                .unwrap_err()
                .message
                .contains("_Pragma")
        );
        let conditional = "#if __has_attribute(_Pragma(\"once\") aligned)\nint value;\n#endif\n";
        assert_eq!(
            pp.preprocess_str(Path::new("query.c"), conditional).is_ok(),
            dialect == QueryDialect::Clang
        );
        for pragma in ["GCC diagnostic push", "message(\\\"query\\\")"] {
            let source = format!("int value = __has_attribute(_Pragma(\"{pragma}\") aligned);\n");
            assert_eq!(
                pp.preprocess_str(Path::new("query.c"), &source).is_ok(),
                dialect == QueryDialect::Clang
            );
        }
    }
}

fn native(compiler: &str, standard: &str, source: &str, options: &[&str]) -> std::process::Output {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new(compiler)
        .args(["-x", "c"])
        .arg(format!("-std={standard}"))
        .args(options)
        .arg("-")
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
    child.wait_with_output().unwrap()
}

fn sources(query: &str, name: &str, pragma: &str) -> Vec<String> {
    let pragma = format!("_Pragma({pragma:?})");
    vec![
        format!("int value = {query}({pragma} {name});\n"),
        format!("#define ARG {pragma} {name}\nint value = {query}(ARG);\n"),
        format!("#define ARG {pragma} {name}\n#define W(x) {query}(x)\nint value = W(ARG);\n"),
        format!("int value = {query}({name} {pragma});\n"),
        format!("#define ARG {name} {pragma}\n#define W(x) {query}(x)\nint value = W(ARG);\n"),
        format!("#if {query}({pragma} {name})\nint yes;\n#else\nint no;\n#endif\n"),
        format!("#if 0 && {query}({pragma} {name})\nint yes;\n#else\nint no;\n#endif\n"),
        format!("#if 0\nint unused = {query}({pragma} {name});\n#endif\nint value;\n"),
    ]
}

#[test]
#[ignore = "requires native GCC and Clang; checks original C compilation and preprocessing separately"]
fn query_pragma_handlers_match_native_compilation() {
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let mut comparisons = 0;
    for (compiler, dialect) in [
        (gcc.as_str(), QueryDialect::Gnu),
        ("clang", QueryDialect::Clang),
    ] {
        let queries = if dialect == QueryDialect::Gnu {
            vec![
                ("__has_builtin", "__builtin_bswap32"),
                ("__has_attribute", "aligned"),
                ("__has_c_attribute", "aligned"),
            ]
        } else {
            vec![
                ("__has_builtin", "__builtin_bswap32"),
                ("__has_attribute", "aligned"),
                ("__has_c_attribute", "aligned"),
                ("__has_feature", "aligned"),
                ("__has_extension", "aligned"),
                ("__has_declspec_attribute", "aligned"),
                ("__building_module", "aligned"),
            ]
        };
        for standard in ["c90", "gnu90", "c11", "gnu11"] {
            let namespace = native(
                compiler,
                standard,
                "__has_c_attribute(gnu::aligned)\n",
                &["-E", "-P"],
            );
            assert_eq!(
                toucan_test_support::compiler_acceptance(&namespace),
                Ok(true)
            );
            let namespace = String::from_utf8(namespace.stdout).unwrap().trim() == "1";
            for (query, name) in &queries {
                for pragma in [
                    "",
                    "once",
                    "GCC system_header",
                    "clang system_header",
                    "clang diagnostic push",
                    "GCC diagnostic push",
                    "message(\"query\")",
                ] {
                    let mut cases = sources(query, name, pragma);
                    if pragma == "once" && matches!(*query, "__has_attribute" | "__has_c_attribute")
                    {
                        for argument in [
                            "P gnu::aligned",
                            "gnu::P aligned",
                            "gnu::aligned P",
                            "gnu P ::aligned",
                        ] {
                            let prefix =
                                format!("#define P _Pragma(\"once\")\n#define ARG {argument}\n");
                            cases.push(format!("{prefix}int value = {query}(ARG);\n"));
                            cases.push(format!(
                                "{prefix}#define W(x) {query}(x)\nint value = W(ARG);\n"
                            ));
                        }
                    }
                    for source in cases {
                        let compiled = native(compiler, standard, &source, &["-fsyntax-only"]);
                        let accepted = toucan_test_support::compiler_acceptance(&compiled).unwrap();
                        let actual = Preprocessor::new(config(
                            dialect,
                            namespace,
                            dialect == QueryDialect::Clang || standard.starts_with("gnu"),
                        ))
                        .preprocess_str(Path::new("query.c"), &source);
                        assert_eq!(
                            actual.is_ok(),
                            accepted,
                            "{compiler} {standard}: {source}\n{actual:?}\n{}",
                            String::from_utf8_lossy(&compiled.stderr)
                        );
                        if let Ok(actual) = actual {
                            let preprocessed = native(compiler, standard, &source, &["-E", "-P"]);
                            assert_eq!(
                                toucan_test_support::compiler_acceptance(&preprocessed),
                                Ok(true)
                            );
                            let output = String::from_utf8(preprocessed.stdout).unwrap();
                            let output: String = output
                                .lines()
                                .filter(|line| {
                                    let line = line.trim();
                                    line != "#pragma"
                                        && !["#pragma GCC ", "#pragma clang ", "#pragma message"]
                                            .iter()
                                            .any(|prefix| line.starts_with(prefix))
                                })
                                .collect();
                            assert_eq!(
                                compact(&actual.source),
                                compact(&output),
                                "{compiler} {standard}: {source}"
                            );
                            let replay =
                                native(compiler, standard, &actual.source, &["-fsyntax-only"]);
                            assert_eq!(
                                toucan_test_support::compiler_acceptance(&replay),
                                Ok(true),
                                "{compiler}: {}",
                                String::from_utf8_lossy(&replay.stderr)
                            );
                        }
                        comparisons += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "{comparisons} native C query/pragma acceptance comparisons; accepted outputs also compared and compiled"
    );
}

#[test]
#[ignore = "requires native GCC and Clang and a temporary header"]
fn conditional_macro_restoration_matches_native_preprocessors() {
    let path = std::env::temp_dir().join(format!("toucan-condition-pragma-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("available.h"), "").unwrap();
    for (compiler, dialect) in [("gcc", QueryDialect::Gnu), ("clang", QueryDialect::Clang)] {
        let mut config = config(dialect, true, true);
        config
            .virtual_headers
            .insert("available.h".into(), String::new());
        let mut pp = Preprocessor::new(config);
        for source in conditional_macro_sources() {
            let native = native(
                compiler,
                "gnu11",
                &source,
                &["-I", path.to_str().unwrap(), "-E", "-P"],
            );
            let accepted = toucan_test_support::compiler_acceptance(&native).unwrap();
            let actual = pp.preprocess_str(Path::new("condition.h"), &source);
            assert_eq!(
                actual.is_ok(),
                accepted,
                "{compiler}: {source}\n{actual:?}\n{}",
                String::from_utf8_lossy(&native.stderr)
            );
            if let Ok(actual) = actual {
                assert_eq!(
                    compact(&actual.source),
                    compact(&String::from_utf8_lossy(&native.stdout)),
                    "{compiler}: {source}"
                );
            }
        }
    }
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn conditional_query_pragmas_ignore_compiler_identity_overrides() {
    let source = "#if __has_attribute(_Pragma(\"once\") aligned)\nint value;\n#endif\n";
    for (compiler, dialect) in [("gcc", QueryDialect::Gnu), ("clang", QueryDialect::Clang)] {
        for (prefix, option) in [
            ("#undef __clang__\n", "-U__clang__"),
            ("#define __clang__ 0\n", "-D__clang__=0"),
        ] {
            let source = format!("{prefix}{source}");
            let compiled = native(compiler, "gnu11", &source, &["-fsyntax-only", option]);
            let accepted = toucan_test_support::compiler_acceptance(&compiled).unwrap();
            assert_eq!(accepted, dialect == QueryDialect::Clang);
            let mut config = config(dialect, true, true);
            if option.starts_with("-U") {
                config.undefine("__clang__");
            } else {
                config.defines.insert("__clang__".into(), "0".into());
            }
            let actual = Preprocessor::new(config).preprocess_str(Path::new("query.c"), &source);
            assert_eq!(actual.is_ok(), accepted, "{compiler}: {actual:?}");
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang and temporary header files"]
fn query_once_effects_match_native_header_inclusion() {
    let path = std::env::temp_dir().join(format!("toucan-query-once-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("once.h"), ONCE_HEADER).unwrap();
    for (compiler, dialect) in [("gcc", QueryDialect::Gnu), ("clang", QueryDialect::Clang)] {
        let mut config = config(dialect, true, true);
        config
            .virtual_headers
            .insert("once.h".into(), ONCE_HEADER.into());
        let actual = Preprocessor::new(config)
            .preprocess_str(Path::new("main.c"), INCLUDES)
            .unwrap();
        let compiled = native(
            compiler,
            "gnu11",
            INCLUDES,
            &["-I", path.to_str().unwrap(), "-fsyntax-only"],
        );
        assert_eq!(
            toucan_test_support::compiler_acceptance(&compiled),
            Ok(true),
            "{compiler}: {}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let output = native(
            compiler,
            "gnu11",
            INCLUDES,
            &["-I", path.to_str().unwrap(), "-E", "-P"],
        );
        assert_eq!(toucan_test_support::compiler_acceptance(&output), Ok(true));
        assert_eq!(
            compact(&actual.source),
            compact(&String::from_utf8_lossy(&output.stdout))
        );
        let compiled = native(compiler, "gnu11", &actual.source, &["-fsyntax-only"]);
        assert_eq!(
            toucan_test_support::compiler_acceptance(&compiled),
            Ok(true)
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}

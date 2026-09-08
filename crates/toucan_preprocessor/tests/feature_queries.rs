use std::path::Path;
use std::sync::Arc;

use toucan_preprocessor::{
    Config, FeatureQueries, FeatureQuery, FeatureQueryProvider, Preprocessor, QueryDialect,
};

#[derive(Debug)]
struct Catalog {
    fallthrough: u64,
    gnu_namespace: bool,
    c_fallthrough: u64,
    c11: bool,
    msvc: bool,
}
impl FeatureQueryProvider for Catalog {
    fn query(&self, kind: FeatureQuery, namespace: Option<&str>, name: &str) -> u64 {
        if namespace.is_some_and(|name| !self.gnu_namespace || !matches!(name, "gnu" | "__gnu__")) {
            return 0;
        }
        let name = if kind == FeatureQuery::Builtin {
            name
        } else {
            name.strip_prefix("__")
                .and_then(|name| name.strip_suffix("__"))
                .unwrap_or(name)
        };
        match kind {
            FeatureQuery::Builtin => u64::from(name == "__builtin_bswap32"),
            FeatureQuery::Attribute => match name {
                "aligned" | "__aligned__" | "packed" => 1,
                "fallthrough" => self.fallthrough,
                _ => 0,
            },
            FeatureQuery::Feature | FeatureQuery::Extension => u64::from(
                matches!(name, "c_atomic" | "c_static_assert")
                    && (self.c11 || kind == FeatureQuery::Extension),
            ),
            FeatureQuery::CAttribute => match (namespace, name) {
                (None, "fallthrough") => self.c_fallthrough,
                (Some(_), "aligned") => 1,
                _ => 0,
            },
            FeatureQuery::DeclspecAttribute => {
                u64::from(self.msvc && matches!(name, "align" | "noinline" | "noreturn"))
            }
            FeatureQuery::BuildingModule => 0,
        }
    }
}

fn config(dialect: QueryDialect) -> Config {
    Config {
        feature_queries: Some(FeatureQueries::new(
            dialect,
            Arc::new(Catalog {
                fallthrough: if dialect == QueryDialect::Gnu {
                    201910
                } else {
                    1
                },
                gnu_namespace: true,
                c_fallthrough: 201910,
                c11: true,
                msvc: false,
            }),
        )),
        scope_punctuator: true,
        allow_filesystem: false,
        ..Config::default()
    }
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

const CASES: &[(&str, &str)] = &[
    (
        "direct",
        "a __has_builtin(__builtin_bswap32)\nb __has_builtin(__toucan_missing)\nc __has_attribute(aligned)\nd __has_attribute(fallthrough)\n",
    ),
    (
        "arguments",
        "#define B __builtin_bswap32\n#define A aligned\na __has_builtin(B)\nb __has_attribute(A)\nc __has_attribute(__aligned__)\n",
    ),
    (
        "wrappers",
        "#define Q __has_builtin\n#define R(x) __has_builtin(x)\n#define B __builtin_bswap32\na Q(__builtin_bswap32)\nb R(B)\n",
    ),
    (
        "counter",
        "a __COUNTER__\nb __has_builtin(__COUNTER__)\nc __COUNTER__\n",
    ),
    ("attribute_counter", "a __has_attribute(__COUNTER__)\n"),
    (
        "namespace",
        "a __has_attribute(gnu::aligned)\nb __has_attribute(__gnu__::__aligned__)\nc __has_attribute(toucan::aligned)\n",
    ),
    (
        "namespace_expansion",
        "#define NS gnu\n#define NAME aligned\na __has_attribute(NS::NAME)\n",
    ),
    ("bare", "a __has_builtin;\n"),
    ("empty", "#if __has_builtin()\nyes\n#endif\n"),
    ("number", "a __has_builtin(1)\n"),
    ("nested", "a __has_builtin((__builtin_bswap32))\n"),
    ("comma", "a __has_builtin(__builtin_bswap32, missing)\n"),
    ("missing_close", "a __has_builtin(__builtin_bswap32\n"),
    ("short_circuit", "#if 0 && __has_builtin()\nyes\n#endif\n"),
    (
        "inactive",
        "#if 0\n#if __has_builtin()\nyes\n#endif\n#endif\nok\n",
    ),
    (
        "undefined",
        "#undef __has_builtin\n#if defined(__has_builtin)\nwrong\n#endif\n__has_builtin(anything)\n",
    ),
    (
        "redefined",
        "#define __has_builtin(x) 7\na __has_builtin()\n#undef __has_builtin\n#define __has_builtin 9\nb __has_builtin\n",
    ),
    ("stringified", "#define STR(x) #x\na STR(__has_builtin())\n"),
    (
        "pasted",
        "#define CAT(a,b) a##b\na CAT(__has_,builtin)(__builtin_bswap32)\n",
    ),
    (
        "defined",
        "#if !defined(__has_builtin) || !defined __has_attribute\n#error missing\n#endif\n#ifdef __has_builtin\nyes\n#endif\n",
    ),
    (
        "expanded_comma",
        "#define A aligned,packed\na __has_attribute(A)\n",
    ),
    ("recursive", "#define A A\na __has_attribute(A)\n"),
];

const NEW_QUERIES: &[(&str, &str)] = &[
    ("__has_feature", "c_atomic"),
    ("__has_extension", "c_atomic"),
    ("__has_c_attribute", "fallthrough"),
    ("__has_declspec_attribute", "align"),
    ("__building_module", "toucan"),
];

fn all_cases() -> Vec<(String, String)> {
    let mut cases: Vec<_> = CASES
        .iter()
        .map(|(name, source)| ((*name).into(), (*source).into()))
        .collect();
    for &(query, supported) in NEW_QUERIES {
        for (name, argument) in [
            ("supported", supported),
            ("unknown", "toucan_missing"),
            ("empty", ""),
            ("number", "1"),
            ("string", "\"foo\""),
            ("nested", "(foo)"),
            ("comma", "foo,bar"),
            ("namespace", "gnu::aligned"),
            ("extra", "foo bar"),
            ("counter", "__COUNTER__"),
        ] {
            for (context, source) in [
                ("direct", format!("a {query}({argument})\nb __COUNTER__\n")),
                (
                    "dead",
                    format!("#if 0 && {query}({argument})\nyes\n#endif\nok\n"),
                ),
                (
                    "inactive",
                    format!("#if 0\n{query}({argument})\n#endif\nok\n"),
                ),
            ] {
                cases.push((format!("{query}/{name}/{context}"), source));
            }
        }
        for (name, source) in [
            (
                "macros",
                format!(
                    "#define ARG {supported}\n#define W(x) {query}(x)\na {query}(ARG)\nb W(ARG)\nc {query}(__{supported}__)\n"
                ),
            ),
            (
                "defined",
                format!("#if defined({query})\nyes\n#else\nno\n#endif\n"),
            ),
            (
                "undefined",
                format!(
                    "#undef {query}\n#ifdef {query}\n#error defined\n#endif\n{query}(anything)\n"
                ),
            ),
            (
                "redefined",
                format!(
                    "#define {query}(x) 7\na {query}()\n#undef {query}\n#define {query} 9\nb {query}\n"
                ),
            ),
            (
                "stringified",
                format!("#define STR(x) #x\nSTR({query}())\n"),
            ),
            (
                "operator_alias",
                format!("#define Q {query}\nQ({supported})\n"),
            ),
            ("bare", format!("{query}\n")),
            ("recursive", format!("#define A A\n{query}(A)\n")),
        ] {
            cases.push((format!("{query}/{name}"), source));
        }
    }
    for query in ["__has_attribute", "__has_c_attribute"] {
        for (name, source) in [
            (
                "names",
                "#define NS gnu\n#define NAME aligned\nQ(NS::NAME)\n",
            ),
            ("whole", "#define WHOLE gnu::aligned\nQ(WHOLE)\n"),
            ("separator", "#define SCOPE ::\nQ(gnu SCOPE aligned)\n"),
            ("space", "Q(gnu : : aligned)\n"),
            ("comment", "Q(gnu:/**/:aligned)\n"),
            ("around", "Q(gnu /**/::/**/ aligned)\n"),
            ("pieces", "#define COLON :\nQ(gnu:COLON aligned)\n"),
            ("joined", "#define JOIN(x) gnu:x\nQ(JOIN(:aligned))\n"),
            ("tail", "#define EMPTY\nQ(gnu::aligned EMPTY)\n"),
            ("before", "#define EMPTY\nQ(EMPTY gnu::aligned)\n"),
            ("after_scope", "#define EMPTY\nQ(gnu::EMPTY aligned)\n"),
            ("unscoped_tail", "#define EMPTY\nQ(fallthrough EMPTY)\n"),
            (
                "wrapped_tail",
                "#define EMPTY\n#define R(x) Q(x)\nR(fallthrough EMPTY)\n",
            ),
            ("pasted", "#define CAT(a,b) a##b\nQ(gnu CAT(:,:) aligned)\n"),
            (
                "wrapped_paste",
                "#define CAT(a,b) a##b\n#define R(x) Q(x)\nR(gnu CAT(:,:) aligned)\n",
            ),
            (
                "wrapped_separator",
                "#define SCOPE ::\n#define R(x) Q(x)\nR(gnu SCOPE aligned)\n",
            ),
        ] {
            cases.push((format!("{query}/scope/{name}"), source.replace('Q', query)));
        }
    }
    cases.push((
        "scope_paste".into(),
        "#define CAT(a,b) a##b\nCAT(:,:)\n".into(),
    ));
    cases
}

#[test]
fn query_arguments_and_rescanning_use_the_configured_dialect() {
    for (dialect, expected) in [
        (QueryDialect::Gnu, "a1b1c1"),
        (QueryDialect::Clang, "a0b1c1"),
    ] {
        let result = Preprocessor::new(config(dialect))
            .preprocess_str(Path::new("query.h"), CASES[1].1)
            .unwrap();
        assert_eq!(compact(&result.source), expected);
    }
    let result = Preprocessor::new(config(QueryDialect::Clang))
        .preprocess_str(Path::new("query.h"), CASES[3].1)
        .unwrap();
    assert_eq!(compact(&result.source), "a0b0c1");
    for dialect in [QueryDialect::Gnu, QueryDialect::Clang] {
        let mut preprocessor = Preprocessor::new(config(dialect));
        for name in [
            "bare",
            "empty",
            "number",
            "nested",
            "comma",
            "missing_close",
            "short_circuit",
            "expanded_comma",
        ] {
            let source = CASES.iter().find(|(case, _)| *case == name).unwrap().1;
            assert!(
                preprocessor
                    .preprocess_str(Path::new("query.h"), source)
                    .is_err(),
                "{dialect:?}: {name}"
            );
        }
        let source = CASES
            .iter()
            .find(|(case, _)| *case == "inactive")
            .unwrap()
            .1;
        assert_eq!(
            preprocessor
                .preprocess_str(Path::new("query.h"), source)
                .unwrap()
                .source,
            "ok\n"
        );
    }
}

#[test]
fn query_overrides_survive_final_macro_queries_and_reset_per_entry_point() {
    for dialect in [QueryDialect::Gnu, QueryDialect::Clang] {
        let mut pp = Preprocessor::new(config(dialect));
        let result = pp
            .preprocess_str(
                Path::new("query.h"),
                "#define VALUE __has_builtin(__builtin_bswap32)\nVALUE\n",
            )
            .unwrap();
        assert_eq!(
            result.expand_object_macro("VALUE").unwrap().as_deref(),
            Some("1")
        );
        assert!(result.is_defined("__has_builtin"));
        assert!(!result.macros.contains_key("__has_builtin"));
        assert_eq!(result.expand_object_macro("__has_builtin").unwrap(), None);
        let result = pp
            .preprocess_str(
                Path::new("query.h"),
                "#define VALUE __has_builtin(__builtin_bswap32)\n#undef __has_builtin\n",
            )
            .unwrap();
        assert!(!result.is_defined("__has_builtin"));
        assert_eq!(
            compact(&result.expand_object_macro("VALUE").unwrap().unwrap()),
            "__has_builtin(__builtin_bswap32)"
        );
        let result = pp
            .preprocess_str(Path::new("query.h"), "__has_builtin(__builtin_bswap32)\n")
            .unwrap();
        assert_eq!(result.source, "1\n");
        let mut c = config(dialect);
        c.undefine("__has_builtin");
        let result = Preprocessor::new(c.clone())
            .preprocess_str(
                Path::new("query.h"),
                "#ifdef __has_builtin\n#error enabled\n#endif\n",
            )
            .unwrap();
        assert!(!result.is_defined("__has_builtin"));
        c.defines.insert("__has_builtin(x)".into(), "7".into());
        let result = Preprocessor::new(c)
            .preprocess_str(Path::new("query.h"), "__has_builtin(anything)\n")
            .unwrap();
        assert_eq!(result.source, "7\n");
        let mut c = config(dialect);
        c.defines.insert("__has_builtin(x)".into(), "9".into());
        let result = Preprocessor::new(c)
            .preprocess_str(Path::new("query.h"), "__has_builtin(anything)\n")
            .unwrap();
        assert_eq!(result.source, "9\n");
    }
}

#[test]
fn remaining_queries_preserve_overrides_and_final_macro_state() {
    for dialect in [QueryDialect::Gnu, QueryDialect::Clang] {
        for &(query, supported) in NEW_QUERIES {
            let enabled = dialect == QueryDialect::Clang || query == "__has_c_attribute";
            let mut pp = Preprocessor::new(config(dialect));
            let source = format!("#define VALUE {query}({supported})\nVALUE\n");
            let initial = pp.preprocess_str(Path::new("query.h"), &source).unwrap();
            assert_eq!(initial.is_defined(query), enabled);
            assert!(!initial.macros.contains_key(query));
            assert_eq!(
                initial
                    .expand_object_macro("VALUE")
                    .unwrap()
                    .unwrap()
                    .trim(),
                initial.source.trim()
            );
            assert_eq!(initial.expand_object_macro(query).unwrap(), None);
            let changed = pp
                .preprocess_str(
                    Path::new("query.h"),
                    &format!("#define {query}(x) 7\n#define VALUE {query}()\nVALUE\n"),
                )
                .unwrap();
            assert_eq!(changed.source, "7\n");
            assert_eq!(
                changed.expand_object_macro("VALUE").unwrap().as_deref(),
                Some("7")
            );
            let reset = pp.preprocess_str(Path::new("query.h"), &source).unwrap();
            assert_eq!(reset.source, initial.source);
            let undefined = pp
                .preprocess_str(
                    Path::new("query.h"),
                    &format!("#undef {query}\n#define VALUE {query}(anything)\n"),
                )
                .unwrap();
            assert!(!undefined.is_defined(query));
            assert_eq!(
                compact(&undefined.expand_object_macro("VALUE").unwrap().unwrap()),
                format!("{query}(anything)")
            );
            for replacement in [None, Some("9")] {
                let mut c = config(dialect);
                c.undefine(query);
                if let Some(replacement) = replacement {
                    c.defines.insert(format!("{query}(x)"), replacement.into());
                }
                let configured = Preprocessor::new(c)
                    .preprocess_str(Path::new("query.h"), &source)
                    .unwrap();
                assert_eq!(configured.is_defined(query), replacement.is_some());
                assert_eq!(
                    compact(&configured.source),
                    replacement.map_or_else(|| format!("{query}({supported})"), str::to_owned)
                );
            }
        }
    }
}

#[test]
fn raw_queries_do_not_expand_arguments_or_require_a_live_counter() {
    for query in ["__has_feature", "__has_extension", "__building_module"] {
        let source = format!(
            "#define ARG c_atomic\n#define W(x) {query}(x)\na {query}(ARG)\nb W(ARG)\nc {query}(__COUNTER__)\nd __COUNTER__\n#define FINAL {query}(__COUNTER__)\n"
        );
        let result = Preprocessor::new(config(QueryDialect::Clang))
            .preprocess_str(Path::new("query.h"), &source)
            .unwrap();
        let wrapped = u8::from(query != "__building_module");
        assert_eq!(compact(&result.source), format!("a0b{wrapped}c0d0"));
        assert_eq!(
            result.expand_object_macro("FINAL").unwrap().as_deref(),
            Some("0")
        );
    }
    for query in ["__has_c_attribute", "__has_declspec_attribute"] {
        let source = format!("#define FINAL {query}(__COUNTER__)\n");
        let result = Preprocessor::new(config(QueryDialect::Clang))
            .preprocess_str(Path::new("query.h"), &source)
            .unwrap();
        assert!(
            result
                .expand_object_macro("FINAL")
                .unwrap_err()
                .message
                .contains("final macro environment")
        );
    }
}

#[test]
fn remaining_queries_validate_arguments_before_conditional_short_circuiting() {
    for &(query, _) in NEW_QUERIES {
        let mut pp = Preprocessor::new(config(QueryDialect::Clang));
        for argument in ["", "1", "\"foo\"", "(foo)", "foo,bar", "foo bar"] {
            let error = pp
                .preprocess_str(
                    Path::new("query.h"),
                    &format!("#line 30 \"logical.h\"\n#if 0 && {query}({argument})\nyes\n#endif\n"),
                )
                .unwrap_err();
            assert_eq!(error.path, Path::new("logical.h"));
            assert_eq!(error.line, 30);
            let inactive = pp
                .preprocess_str(
                    Path::new("query.h"),
                    &format!("#if 0\n{query}({argument})\n#endif\nok\n"),
                )
                .unwrap();
            assert_eq!(inactive.source, "ok\n");
        }
        let mut c = config(QueryDialect::Clang);
        c.max_expansion_depth = 1;
        let error = Preprocessor::new(c)
            .preprocess_str(
                Path::new("query.h"),
                &format!("#define Q {query}\nQ(foo)\n"),
            )
            .unwrap_err();
        assert!(error.message.contains("depth limit"));
    }
}

#[test]
fn query_expansion_charges_budgets_and_preserves_invocation_locations() {
    let mut c = config(QueryDialect::Gnu);
    c.max_tokens = 28;
    let error = Preprocessor::new(c)
        .preprocess_str(
            Path::new("query.h"),
            "#define A aligned aligned aligned aligned\n#define B A A A A\n__has_attribute(B)\n",
        )
        .unwrap_err();
    assert!(error.message.contains("limit"), "{error}");
    let mut c = config(QueryDialect::Clang);
    c.max_expansion_depth = 1;
    let error = Preprocessor::new(c)
        .preprocess_str(
            Path::new("query.h"),
            "#define QUERY __has_builtin\nQUERY(__builtin_bswap32)\n",
        )
        .unwrap_err();
    assert!(error.message.contains("depth limit"), "{error}");
    let mut pp = Preprocessor::new(config(QueryDialect::Clang));
    let result = pp
        .preprocess_str(
            Path::new("query.h"),
            "#define Q(x) __has_builtin(x)\n#line 20 \"logical.h\"\n  Q(__builtin_bswap32)\n",
        )
        .unwrap();
    let location = result.resolve_location(0).unwrap();
    assert_eq!(
        (&*location.path, location.line, location.column),
        (Path::new("logical.h"), 20, 3)
    );
    let error = pp
        .preprocess_str(
            Path::new("query.h"),
            "#line 30 \"logical.h\"\n  __has_builtin(1)\n",
        )
        .unwrap_err();
    assert_eq!(
        (error.path.as_path(), error.line, error.column),
        (Path::new("logical.h"), 30, 3)
    );
}

#[test]
#[ignore = "requires native GCC and Clang; Clang cross-target preprocessing uses no sysroot"]
fn feature_query_syntax_values_and_effects_match_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let targets = [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ];
    let preprocess =
        |compiler: &str, target: Option<&str>, standard: &str, source: &str, options: &[String]| {
            let mut command = Command::new(compiler);
            command
                .args(["-E", "-P", "-x", "c"])
                .arg(format!("-std={standard}"))
                .args(options);
            if let Some(target) = target {
                command.arg(format!("--target={target}"));
            }
            let mut child = command
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
        };
    let cases = all_cases();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for (compiler, dialect) in [
        (gcc.as_str(), QueryDialect::Gnu),
        ("clang", QueryDialect::Clang),
    ] {
        // Newer GCC releases define additional operators. Keep their raw
        // availability visible, then explicitly select the GNU 13 query
        // environment exercised by this dialect's mechanical tests.
        let mut query_options = Vec::new();
        if dialect == QueryDialect::Gnu {
            for &(query, _) in NEW_QUERIES {
                if query == "__has_c_attribute" {
                    continue;
                }
                let source = format!("#ifdef {query}\n1\n#else\n0\n#endif\n");
                let native = preprocess(compiler, None, "gnu11", &source, &[]);
                assert_eq!(toucan_test_support::compiler_acceptance(&native), Ok(true));
                let available = String::from_utf8(native.stdout).unwrap().trim() == "1";
                eprintln!("{compiler}: raw {query} availability={available}");
                if available {
                    query_options.push(format!("-U{query}"));
                }
            }
            eprintln!("{compiler}: GNU 13 query-environment flags={query_options:?}");
        }
        let preprocess = |compiler: &str,
                          target: Option<&str>,
                          standard: &str,
                          source: &str,
                          options: &[String]| {
            let options: Vec<_> = query_options.iter().chain(options).cloned().collect();
            preprocess(compiler, target, standard, source, &options)
        };
        // This catalog tests operator mechanics. Attribute revision dates belong
        // to the selected compiler, and GCC releases use different values.
        let revision = preprocess(
            compiler,
            None,
            "gnu11",
            "__has_attribute(fallthrough)\n",
            &[],
        );
        assert_eq!(
            toucan_test_support::compiler_acceptance(&revision),
            Ok(true),
            "{compiler}: {revision:?}"
        );
        let fallthrough = String::from_utf8(revision.stdout)
            .unwrap()
            .trim()
            .parse::<u64>()
            .unwrap();
        let revision = preprocess(
            compiler,
            None,
            "gnu11",
            "__has_c_attribute(fallthrough)\n",
            &[],
        );
        assert_eq!(
            toucan_test_support::compiler_acceptance(&revision),
            Ok(true)
        );
        let c_fallthrough = String::from_utf8(revision.stdout)
            .unwrap()
            .trim()
            .trim_end_matches('L')
            .parse::<u64>()
            .unwrap();
        for &(query, _) in NEW_QUERIES {
            for replacement in [None, Some("7")] {
                let mut options = vec![format!("-U{query}")];
                let mut c = config(dialect);
                c.undefine(query);
                if let Some(replacement) = replacement {
                    options.push(format!("-D{query}(x)={replacement}"));
                    c.defines.insert(format!("{query}(x)"), replacement.into());
                }
                let source = format!("#ifdef {query}\nyes\n#else\nno\n#endif\n{query}(anything)\n");
                let native = preprocess(compiler, None, "gnu11", &source, &options);
                assert_eq!(toucan_test_support::compiler_acceptance(&native), Ok(true));
                let actual = Preprocessor::new(c)
                    .preprocess_str(Path::new("query.h"), &source)
                    .unwrap();
                assert_eq!(
                    compact(&actual.source),
                    compact(&String::from_utf8_lossy(&native.stdout))
                );
            }
        }
        let native_targets = ["native"];
        let targets = if dialect == QueryDialect::Gnu {
            &native_targets[..]
        } else {
            &targets[..]
        };
        for target in targets {
            for standard in ["c90", "gnu90", "c11", "gnu11"] {
                // GCC versions differ in strict-mode namespace handling. These
                // catalogs exercise operator mechanics against the selected
                // compiler; production profile capabilities are tested upstream.
                let native_target = (dialect == QueryDialect::Clang).then_some(*target);
                let namespace = preprocess(
                    compiler,
                    native_target,
                    standard,
                    "__has_c_attribute(gnu::aligned)\n",
                    &[],
                );
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&namespace),
                    Ok(true)
                );
                let gnu_namespace = String::from_utf8(namespace.stdout).unwrap().trim() == "1";
                let scope = preprocess(
                    compiler,
                    native_target,
                    standard,
                    "#define CAT(a,b) a##b\nCAT(:,:)\n",
                    &[],
                );
                let scope_punctuator = toucan_test_support::compiler_acceptance(&scope).unwrap();
                for (name, source) in &cases {
                    let native = preprocess(
                        compiler,
                        (dialect == QueryDialect::Clang).then_some(*target),
                        standard,
                        source,
                        &[],
                    );
                    let accepted = toucan_test_support::compiler_acceptance(&native).unwrap();
                    let mut query_config = config(dialect);
                    query_config.feature_queries.as_mut().unwrap().provider = Arc::new(Catalog {
                        fallthrough,
                        gnu_namespace,
                        c_fallthrough,
                        c11: standard.ends_with("11"),
                        msvc: target.ends_with("windows-msvc"),
                    });
                    query_config.scope_punctuator = scope_punctuator;
                    let actual = Preprocessor::new(query_config)
                        .preprocess_str(Path::new("query.h"), source);
                    assert_eq!(
                        actual.is_ok(),
                        accepted,
                        "{compiler} {target} {standard} {name}: {actual:?}\n{}",
                        String::from_utf8_lossy(&native.stderr)
                    );
                    if let Ok(actual) = actual {
                        assert_eq!(
                            compact(&actual.source),
                            compact(&String::from_utf8_lossy(&native.stdout)),
                            "{compiler} {target} {standard} {name}"
                        );
                    }
                }
                eprintln!(
                    "{compiler} {target} {standard}: {} cases, namespace={gnu_namespace}, scope_punctuator={scope_punctuator}",
                    cases.len()
                );
            }
        }
    }
    eprintln!(
        "{} native query cases across 24 compiler/target/mode routes; 20 command-line override cases",
        cases.len() * 24
    );
}

#[test]
fn feature_query_preserves_catalog_revision_numbers() {
    for dialect in [QueryDialect::Gnu, QueryDialect::Clang] {
        for fallthrough in [1, 201910, 202311] {
            let mut query_config = config(dialect);
            query_config.feature_queries.as_mut().unwrap().provider = Arc::new(Catalog {
                fallthrough,
                gnu_namespace: true,
                c_fallthrough: fallthrough,
                c11: true,
                msvc: false,
            });
            let result = Preprocessor::new(query_config)
                .preprocess_str(Path::new("query.h"), "__has_attribute(fallthrough)\n")
                .unwrap();
            let suffix = if dialect == QueryDialect::Clang && fallthrough > 1 {
                "L"
            } else {
                ""
            };
            assert_eq!(result.source, format!("{fallthrough}{suffix}\n"));
        }
    }
}

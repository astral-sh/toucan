use std::collections::BTreeMap;
use std::path::Path;

use crate::{Config, ForcedInclude, PreprocessingTimestamp, Preprocessor};

const TIMESTAMPS: &[(u64, &str, &str)] = &[
    (0, "Jan  1 1970", "00:00:00"),
    (694_923, "Jan  9 1970", "01:02:03"),
    (822_896, "Jan 10 1970", "12:34:56"),
    (946_684_799, "Dec 31 1999", "23:59:59"),
    (951_782_400, "Feb 29 2000", "00:00:00"),
    (1_709_251_199, "Feb 29 2024", "23:59:59"),
    (4_107_542_399, "Feb 28 2100", "23:59:59"),
    (4_107_542_400, "Mar  1 2100", "00:00:00"),
    (13_574_608_496, "Feb 29 2400", "12:34:56"),
    (253_402_300_799, "Dec 31 9999", "23:59:59"),
];

#[test]
fn date_time_formats_validated_utc_timestamps() {
    for &(seconds, date, time) in TIMESTAMPS {
        let timestamp = PreprocessingTimestamp::from_unix_seconds(seconds).unwrap();
        assert_eq!(timestamp.unix_seconds(), seconds);
        let result = Preprocessor::new(Config {
            timestamp,
            ..Config::default()
        })
        .preprocess_str(Path::new("stamp.h"), "__DATE__ __TIME__\n")
        .unwrap();
        assert_eq!(result.source, format!("\"{date}\" \"{time}\"\n"));
        assert_eq!(
            result.expand_object_macro("__DATE__").unwrap(),
            Some(format!("\"{date}\""))
        );
        assert_eq!(
            result.expand_object_macro("__TIME__").unwrap(),
            Some(format!("\"{time}\""))
        );
    }
    assert_eq!(
        PreprocessingTimestamp::default(),
        PreprocessingTimestamp::UNIX_EPOCH
    );
    assert_eq!(
        preprocess("__DATE__ __TIME__\n"),
        "\"Jan  1 1970\" \"00:00:00\"\n"
    );
    for invalid in [
        "",
        "-1",
        "+1",
        " 1",
        "1 ",
        "1.0",
        "1e2",
        "١",
        "253402300800",
        "18446744073709551616",
    ] {
        assert!(
            invalid.parse::<PreprocessingTimestamp>().is_err(),
            "{invalid:?}"
        );
    }
    assert!(PreprocessingTimestamp::from_unix_seconds(u64::MAX).is_err());
    assert_eq!(
        "0001"
            .parse::<PreprocessingTimestamp>()
            .unwrap()
            .unix_seconds(),
        1
    );
}

#[test]
fn date_time_share_configuration_across_includes_queries_and_resets() {
    let timestamp = PreprocessingTimestamp::from_unix_seconds(951_782_400).unwrap();
    let config = Config {
        timestamp,
        allow_filesystem: false,
        virtual_headers: BTreeMap::from([(
            "inner.h".into(),
            "#line 40 \"logical.h\"\nBUILD_DATE BUILD_TIME\n".into(),
        )]),
        forced_includes: vec![ForcedInclude {
            path: "forced.h".into(),
            source:
                "#define BUILD_DATE __DATE__\n#define BUILD_TIME __TIME__\nBUILD_DATE BUILD_TIME\n"
                    .into(),
        }],
        ..Config::default()
    };
    let source = "#if !defined(__DATE__) || !defined(__TIME__)\n#error missing standard macro\n#endif\n#ifdef __TIMESTAMP__\n#error unsupported extension advertised\n#endif\n#include <inner.h>\n  BUILD_DATE BUILD_TIME\n";
    let mut preprocessor = Preprocessor::new(config);
    for _ in 0..2 {
        let result = preprocessor
            .preprocess_str(Path::new("entry.h"), source)
            .unwrap();
        assert_eq!(result.source, "\"Feb 29 2000\" \"00:00:00\"\n".repeat(3));
        for (name, expected) in [
            ("BUILD_DATE", "\"Feb 29 2000\""),
            ("BUILD_TIME", "\"00:00:00\""),
        ] {
            assert_eq!(
                result.expand_object_macro(name).unwrap().as_deref(),
                Some(expected)
            );
        }
        let origins: Vec<_> = result
            .mappings
            .iter()
            .map(|m| {
                (
                    &m.origin.path,
                    m.origin.line,
                    m.origin.column,
                    m.origin.kind,
                )
            })
            .collect();
        assert_eq!(origins.len(), 6);
        for (index, (path, line, column, kind)) in origins.into_iter().enumerate() {
            let (expected_path, expected_line, columns) = match index / 2 {
                0 => ("forced.h", 3, [1, 12]),
                1 => ("logical.h", 40, [1, 12]),
                _ => ("entry.h", 8, [3, 14]),
            };
            assert_eq!(
                (path.as_ref(), line, column),
                (Path::new(expected_path), expected_line, columns[index % 2])
            );
            assert_eq!(kind, crate::OriginKind::MacroInvocation);
        }
    }
}

#[test]
fn date_time_obey_builtin_redefinition_and_expansion_rules() {
    for name in ["__DATE__", "__TIME__"] {
        for source in [
            format!("#define {name} \"override\"\n"),
            format!("#undef {name}\n"),
        ] {
            assert!(
                Preprocessor::new(Config::default())
                    .preprocess_str(Path::new("bad.h"), &source)
                    .is_err()
            );
        }
        let config = Config {
            defines: BTreeMap::from([(name.into(), "\"override\"".into())]),
            ..Config::default()
        };
        let error = Preprocessor::new(config)
            .preprocess_str(Path::new("empty.h"), "")
            .unwrap_err();
        assert_eq!(error.path, Path::new("<predefined>"));
        assert!(error.message.contains("reserved macro"));
    }
    assert_eq!(
        preprocess("#define S(x) #x\n#define C(a,b) a##b\nS(__DATE__) C(__TI,ME__)\n"),
        "\"__DATE__\" \"00:00:00\"\n"
    );
    assert!(
        Preprocessor::new(Config::default())
            .preprocess_str(Path::new("mtime.h"), "__TIMESTAMP__\n")
            .unwrap_err()
            .message
            .contains("file modification timestamps")
    );
    assert_eq!(
        preprocess("#define __TIMESTAMP__ \"provided\"\n__TIMESTAMP__\n"),
        "\"provided\"\n"
    );
}

#[test]
fn date_time_match_native_preprocessor() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut timestamps: Vec<_> = TIMESTAMPS.iter().map(|&(seconds, _, _)| seconds).collect();
    // Sample the whole supported range, beyond the explicit calendar boundaries.
    let mut state = 0x1234_5678_u64;
    for _ in 0..48 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        timestamps.push(state % (PreprocessingTimestamp::MAX_UNIX_SECONDS + 1));
    }
    let source = "#if !defined(__DATE__) || !defined(__TIME__)\n#error missing standard macro\n#endif\n#define DATE __DATE__\n#define TIME __TIME__\nDATE TIME\n#line 90 \"remapped.h\"\nDATE TIME\n";
    for seconds in timestamps {
        let mut child = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
            .args(["-E", "-P", "-std=c11", "-pedantic-errors", "-x", "c", "-"])
            .env("SOURCE_DATE_EPOCH", seconds.to_string())
            .env("TZ", "Pacific/Honolulu")
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
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = String::from_utf8(output.stdout).unwrap();
        let config = Config {
            timestamp: PreprocessingTimestamp::from_unix_seconds(seconds).unwrap(),
            ..Config::default()
        };
        let actual = Preprocessor::new(config)
            .preprocess_str(Path::new("date.h"), source)
            .unwrap();
        assert_eq!(
            actual
                .source
                .lines()
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>(),
            expected
                .lines()
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>(),
            "timestamp {seconds}"
        );
    }
}

fn preprocess(source: &str) -> String {
    Preprocessor::new(Config::default())
        .preprocess_str(Path::new("test.h"), source)
        .unwrap()
        .source
}

#[test]
fn object_and_function_macros_rescan_with_following_input() {
    let source = "\
#define VALUE 42
#define INDIRECT VALUE
#define FUNCTION(x) (x + INDIRECT)
#define ALIAS FUNCTION
ALIAS(3)
#define SELF SELF
#define FIRST SECOND
#define SECOND FIRST
SELF FIRST
#define EMPTY()
EMPTY()
";
    assert_eq!(preprocess(source), "( 3 + 42 )\nSELF FIRST\n\n");
}

#[test]
fn arguments_prescan_and_suppress_recursive_expansion() {
    assert_eq!(
        preprocess("#define F(x) (x + x)\nF(F(1))\n"),
        "( ( 1 + 1 ) + ( 1 + 1 ) )\n"
    );
    assert_eq!(
        preprocess("#define F(x) F(x)\nF(F(1))\n"),
        "F ( F ( 1 ) )\n"
    );
    assert_eq!(preprocess("#define A B\n#define B A\nA B\n"), "A B\n");
}

#[test]
fn function_invocation_suppression_ends_at_the_closing_parenthesis() {
    let source = "\
#define ALIAS FUNCTION
#define FUNCTION(x) ALIAS
ALIAS(0) ALIAS(0)(1)
#define PASS(x) x
PASS(ALIAS(0)) PASS(ALIAS)(0)
";
    assert_eq!(
        preprocess(source),
        "FUNCTION FUNCTION ( 1 )\nFUNCTION FUNCTION\n"
    );
}

#[test]
fn strings_pastes_and_empty_arguments() {
    let source = r#"
#define STR(x) #x
#define XSTR(x) STR(x)
#define CAT(a,b) a ## b
#define VALUE 99
STR(VALUE) XSTR(VALUE) STR(a +  b) STR(a+b)
STR("quoted\\string")
CAT(VAL,UE) CAT(,VALUE) CAT(VALUE,) CAT(,)
#define OBJ VAL ## UE
OBJ
"#;
    assert_eq!(
        preprocess(source),
        "\"VALUE\" \"99\" \"a + b\" \"a+b\" \"\\\"quoted\\\\\\\\string\\\"\" 99 99 99\n99\n"
    );
}

#[test]
fn variadic_macros_distinguish_empty_and_omitted_arguments() {
    let source = "\
#define LOG(fmt, ...) call(fmt, ## __VA_ARGS__)
LOG(1) LOG(1,) LOG(1, 2, 3)
#define GNU(args...) call(args)
GNU(1, 2)
#define STANDARD(...) __VA_ARGS__
STANDARD() STANDARD(1, 2)
";
    assert_eq!(
        preprocess(source),
        "call ( 1 ) call ( 1 , ) call ( 1 , 2 , 3 )\ncall ( 1 , 2 )\n1 , 2\n"
    );
}

#[test]
fn conditionals_are_checked_and_short_circuit() {
    let source = "\
#define VALUE 42
#if defined(VALUE) && !defined(MISSING) && VALUE == 42
 yes
#elif 1 / 0
no
#else
no
#endif
#if 0
#if malformed ignored stuff
#error ignored
#endif
#else
#if (1 || 1 / 0) && (-1 > 0U)
yes
#endif
#endif
#undef VALUE
#ifndef VALUE
undefined
#endif
";
    assert_eq!(preprocess(source), "yes\nyes\nundefined\n");
}

#[test]
fn comments_splices_and_multiline_invocations() {
    assert_eq!(
        preprocess("#define ADD(a,b) a + \\\nb\nADD(1,\n2) /* comment\n */\n\"//not a comment\"\n"),
        "1 + 2 \"//not a comment\"\n"
    );
}

#[test]
fn multiline_block_comments_do_not_end_directives() {
    let source = "\
# /* comment
*/ define FIRST /* more
*/ 41
#define ADD(x) /* comment
*/ (x + 1)
#if /* comment
*/ FIRST == 41
int value = ADD(FIRST);
#endif
__LINE__
";
    let result = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("comments.h"), source)
        .unwrap();
    assert_eq!(result.source, "int value = ( 41 + 1 ) ;\n10\n");
    let value = result
        .resolve_location(result.source.find("value").unwrap())
        .unwrap();
    assert_eq!((value.line, value.column), (8, 5));
}

#[test]
fn builtin_macros_and_line_directive() {
    assert_eq!(
        preprocess("__LINE__ __FILE__\n#line 100 \"logical.h\"\n__LINE__ __FILE__\n"),
        "1 \"test.h\"\n100 \"logical.h\"\n"
    );
}

#[test]
fn line_filenames_decode_c_escapes() {
    let result = Preprocessor::new(Config::default())
        .preprocess_str(
            Path::new("physical.h"),
            "#line 40 \"dir\\\\quoted\\\"\\142\\x2eh\"\nint value; __FILE__ __LINE__\n",
        )
        .unwrap();
    assert_eq!(result.source, "int value ; \"dir\\\\quoted\\\"b.h\" 40\n");
    let location = result.resolve_location(0).unwrap();
    assert_eq!(location.path.as_ref(), Path::new("dir\\quoted\"b.h"));
    assert_eq!(location.line, 40);
    for filename in ["\\x", "\\400", "\\0", "\\xff", "\\u0061", "\\U00110000"] {
        let source = format!("#line 1 \"{filename}\"\n__FILE__\n");
        assert!(
            Preprocessor::new(Config::default())
                .preprocess_str(Path::new("test.h"), &source)
                .is_err(),
            "{filename}"
        );
    }
}

#[test]
fn virtual_headers_once_and_macro_includes() {
    let config = Config {
        virtual_headers: BTreeMap::from([(
            "virtual.h".into(),
            "#pragma once\n#define VALUE 7\nint value;\n".into(),
        )]),
        ..Config::default()
    };
    let source = "#define HEADER <virtual.h>\n#include HEADER\n#include <virtual.h>\n#if defined(__has_include) && __has_include(HEADER) && !__has_include(<missing.h>)\nVALUE\n#endif\n";
    let result = Preprocessor::new(config)
        .preprocess_str(Path::new("test.h"), source)
        .unwrap();
    assert_eq!(result.source, "int value ;\n7\n");
    assert!(result.dependencies.is_empty());
    assert_eq!(
        result.expand_object_macro("VALUE").unwrap().as_deref(),
        Some("7")
    );
}

#[test]
fn forced_includes_share_macros_limits_and_source_locations() {
    let config = Config {
        allow_filesystem: false,
        forced_includes: vec![
            ForcedInclude {
                path: "compiler.h".into(),
                source: "#define TYPE int\ntypedef TYPE word;\n".into(),
            },
            ForcedInclude {
                path: "config.h".into(),
                source: "#define VALUE 7\nTYPE configured;\n".into(),
            },
        ],
        ..Config::default()
    };
    let mut preprocessor = Preprocessor::new(config.clone());
    for _ in 0..2 {
        let result = preprocessor
            .preprocess_str(Path::new("user.h"), "\nword value[VALUE];\n#undef TYPE\n")
            .unwrap();
        assert_eq!(
            result.source,
            "typedef int word ;\nint configured ;\nword value [ 7 ] ;\n"
        );
        for (text, path, line) in [
            ("typedef", "compiler.h", 2),
            ("configured", "config.h", 2),
            ("value", "user.h", 2),
        ] {
            let origin = result
                .resolve_location(result.source.find(text).unwrap())
                .unwrap();
            assert_eq!(origin.path.as_ref(), Path::new(path));
            assert_eq!(origin.line, line);
        }
        assert!(result.dependencies.is_empty());
    }

    let mut limited = config;
    limited.max_source_bytes = limited
        .forced_includes
        .iter()
        .map(|include| include.source.len())
        .sum();
    let error = Preprocessor::new(limited)
        .preprocess_str(Path::new("user.h"), "word value;")
        .unwrap_err();
    assert!(error.message.contains("source byte limit"));
    assert_eq!(error.path, Path::new("user.h"));
}

#[test]
fn literal_header_names_do_not_expand_macros() {
    let config = Config {
        virtual_headers: BTreeMap::from([("literal.h".into(), "yes\n".into())]),
        ..Config::default()
    };
    assert_eq!(Preprocessor::new(config).preprocess_str(Path::new("test.h"), "#define literal wrong\n#include <literal.h>\n#if __has_include(<literal.h>)\nyes\n#endif\n").unwrap().source, "yes\nyes\n");
}

#[test]
fn include_queries_stop_after_virtual_headers() {
    let config = Config {
        allow_filesystem: false,
        virtual_headers: BTreeMap::from([(
            "virtual.h".into(),
            "#if !defined(__has_include_next)\n#error missing builtin\n#endif\n\
             #if __has_include_next(<virtual.h>)\n#error queried the current header\n\
             #elif __has_include(<virtual.h>)\nfound\n#endif\n"
                .into(),
        )]),
        ..Config::default()
    };
    let result = Preprocessor::new(config)
        .preprocess_str(Path::new("test.h"), "#include <virtual.h>\n")
        .unwrap();
    assert_eq!(result.source, "found\n");
    for source in [
        "#if __has_include_next <virtual.h>\n#endif\n",
        "#if __has_include_next(<virtual.h>\n#endif\n",
    ] {
        assert!(
            Preprocessor::new(Config::default())
                .preprocess_str(Path::new("test.h"), source)
                .is_err()
        );
    }
}

#[test]
fn incompatible_redefinitions_and_malformed_directives_fail() {
    for source in [
        "#define VALUE 1\n#define VALUE 2\n",
        "#define F(a,a) a\n",
        "#define F(a) # 1\n",
        "#define F(a) a ##\n",
        "#if 1\n",
        "#endif\n",
        "#if 1\n#else\n#else\n#endif\n",
        "#if 1 / 0\n#endif\n",
        "#include <missing.h>\n",
        "#pragma unsupported\n",
        "#define F(a,b) a\nF(1)\n",
        "#define F(a) a\nF(1\n",
        "/* unterminated",
        "#define CAT(a,b) a ## b\nCAT(+,*)\n",
        "#if defined(1)\n#endif\n",
    ] {
        assert!(
            Preprocessor::new(Config::default())
                .preprocess_str(Path::new("test.h"), source)
                .is_err(),
            "{source}"
        );
    }
    assert_eq!(
        preprocess("#define VALUE  1  +  2\n#define VALUE 1 + 2\nVALUE\n"),
        "1 + 2\n"
    );
}

#[test]
fn configuration_function_macros_and_translation_unit_reset() {
    let config = Config {
        defines: BTreeMap::from([("__has_builtin(x)".into(), "0".into())]),
        ..Config::default()
    };
    let mut preprocessor = Preprocessor::new(config);
    let result = preprocessor
        .preprocess_str(
            Path::new("first.h"),
            "#define FIRST 1\n#if !__has_builtin(anything)\nyes\n#endif\n",
        )
        .unwrap();
    assert_eq!(result.source, "yes\n");
    let result = preprocessor
        .preprocess_str(
            Path::new("second.h"),
            "#ifdef FIRST\n#error leaked\n#endif\n",
        )
        .unwrap();
    assert!(result.source.is_empty());
}

#[test]
fn expansion_and_include_limits_bound_work() {
    let config = Config {
        max_tokens: 32,
        ..Config::default()
    };
    let source = "#define F(x) x x x x\nF(F(F(1)))\n";
    let error = Preprocessor::new(config)
        .preprocess_str(Path::new("limit.h"), source)
        .unwrap_err();
    assert!(error.message.contains("token limit"));
    let config = Config {
        max_expansion_depth: 2,
        ..Config::default()
    };
    let error = Preprocessor::new(config)
        .preprocess_str(
            Path::new("limit.h"),
            "#define A B\n#define B C\n#define C D\nA\n",
        )
        .unwrap_err();
    assert!(error.message.contains("depth limit"));
    let config = Config {
        max_include_depth: 3,
        virtual_headers: BTreeMap::from([("loop.h".into(), "#include <loop.h>\n".into())]),
        ..Config::default()
    };
    let error = Preprocessor::new(config)
        .preprocess_str(Path::new("limit.h"), "#include <loop.h>\n")
        .unwrap_err();
    assert!(error.message.contains("include depth"));
}

#[test]
fn pack_directives_survive_preprocessing() {
    assert_eq!(
        preprocess("#pragma pack(push, 1)\nstruct S { int a; };\n#pragma pack(pop)\n"),
        "#pragma pack ( push , 1 )\nstruct S { int a ; } ;\n#pragma pack ( pop )\n"
    );
}

#[test]
fn physical_lines_survive_splices_and_comments() {
    assert_eq!(
        preprocess("__LINE__ \\\n__LINE__\n/* a\n b */ __LINE__\n"),
        "1 2 4\n"
    );
    assert_eq!(
        preprocess("#define LINE __LINE__\nLINE\n#line 20\nLINE \\\nLINE\n"),
        "2\n20 21\n"
    );
}

#[test]
fn cumulative_bytes_and_expression_nesting_are_bounded() {
    let config = Config {
        max_source_bytes: 32,
        ..Config::default()
    };
    let error = Preprocessor::new(config)
        .preprocess_str(Path::new("huge.h"), &format!("/* {} */", "x".repeat(33)))
        .unwrap_err();
    assert!(error.message.contains("source byte limit"));
    let config = Config {
        max_source_bytes: 64,
        virtual_headers: BTreeMap::from([("a.h".into(), "int a;\n".repeat(5))]),
        ..Config::default()
    };
    let error = Preprocessor::new(config)
        .preprocess_str(Path::new("repeat.h"), "#include <a.h>\n#include <a.h>\n")
        .unwrap_err();
    assert!(error.message.contains("source byte limit"));
    let config = Config {
        max_source_bytes: 512,
        ..Config::default()
    };
    let source = format!(
        "#define REPEAT(x) x x x x x x x x\nREPEAT({})\n",
        "a".repeat(100)
    );
    let error = Preprocessor::new(config)
        .preprocess_str(Path::new("expansion.h"), &source)
        .unwrap_err();
    assert!(error.message.contains("byte limit"));
    for expression in [
        format!("{}1{}", "(".repeat(200), ")".repeat(200)),
        format!("{}1", "!".repeat(200)),
    ] {
        let error = Preprocessor::new(Config::default())
            .preprocess_str(Path::new("deep.h"), &format!("#if {expression}\n#endif\n"))
            .unwrap_err();
        assert!(error.message.contains("nesting limit"));
    }
}

#[test]
fn differential_macro_corpus_matches_native_c_preprocessor() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let corpus = [
        "#define A 3\n#define F(x) (x+A)\n#define G F\nG(G(2))\n",
        "#define F() 1\nF _Pragma(\"pack(1)\") ()\n",
        "#define E(x)\n#define F(x) E(x)\nF(_Pragma(\"pack(1)\"))\n",
        "#define F(a,b) b a\nF(__COUNTER__,__COUNTER__)\n",
        "??=define F(x) x ??! x\nint a??(2??); F(1) ??' ??- ??< ??>\n\"??/n\"\n",
        "??=define X 4??/\n2\nX __LINE__\n// ignored??/\nstill ignored\n__LINE__\n",
        "#define DO(x) _Pragma(#x)\nDO(pack(push,1))\nstruct S { char x; int y; };\nDO(pack(pop))\n",
        "#define F(x) x x\nF(int a; _Pragma(\"pack(1)\") int b;)\n",
        "#define ARG \"pack(push,2)\"\n_Pragma(ARG)\n_Pragma(L\"pack(pop)\")\n_Pragma(\"pack/**/(1)\")\n",
        "#define F(x) x x\n#define IGNORE(x)\nF(F(__COUNTER__)) IGNORE(__COUNTER__) __COUNTER__\n",
        "#define S(...) #__VA_ARGS__\nS(a ,b) S(a, b) S(a , b) S(,)\n#define T(x,...) #__VA_ARGS__\nT(0,a ,b, c)\n",
        "#line 40 \"dir\\\\quoted\\\"\\142\\x2eh\"\n__FILE__ __LINE__\n#line 50 \"\\u00e9\\U0001f426.h\"\n__FILE__ __LINE__\n",
        "#define A F\n#define F(x) A\nA(0) A(0)(1)\n#define P(x) x\nP(A(0)) P(A)(0)\n",
        "# /* comment\n*/ define A /* more\n*/ 41\n#define F(x) /* comment\n*/ (x + 1)\n#if /* comment\n*/ A == 41\nint value = F(A);\n#endif\n__LINE__\n",
        "#define A a /* one\n two */ b\n#define S(x) #x\n#define T(x) S(x)\nT(A) S(a /* one\n two */ b)\n",
        "#define A /* comment\n*/ 42\nint value = A;\n",
        "#define F(x) x\n#define G F\nF(G)(2) G(F)(3)\n",
        "#define F(x) x\n#define H F\nH(H)(2)\n",
        "#define A B\n#define B A\nA B A(A)\n",
        "#define S(x) #x\n#define T(x) S(x)\n#define A one+two\nS(A) T(A) S(a/**/b) S(+)\n",
        "#define C(a,b) a##b\n#define A 3\nC(A,) C(,A) C(,) C(1,e3)\n",
        "#define L(x,...) f(x,##__VA_ARGS__)\nL(1) L(1,) L(1,2,3)\n",
        "#if (0xffffffffffffffff > 0) && (-1 > 1U) && (1 ? -1 : 0U) > 0\nyes\n#endif\n",
        "__LINE__ \\\n__LINE__\n/* comment\n */ __LINE__\n",
        "#define A(x) x+x\nA(A(A(1)))\n",
        "#define x 3\n#define f(a) f(x * (a))\n#undef x\n#define x 2\n#define g f\n#define z z[0]\n#define h g(~\n#define m(a) a(w)\n#define w 0,1\n#define t(a) a\nf(y+1)+f(f(z))%t(t(g)(0)+t)(1);\ng(x+(3,4)-w)|h 5)&m(f)^m(m);\n",
    ];
    for source in corpus {
        let mut child = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
            .args([
                "-E",
                "-P",
                "-undef",
                "-std=gnu11",
                "-trigraphs",
                "-x",
                "c",
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the differential test requires a C compiler (set CC)");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = crate::token::render(
            &crate::token::lex(&String::from_utf8(output.stdout).unwrap()).unwrap(),
        );
        let actual = crate::token::render(&crate::token::lex(&preprocess(source)).unwrap());
        assert_eq!(actual, expected, "source:\n{source}");
    }
}

#[test]
fn include_next_tracks_search_origin_and_quoted_local_includes() {
    use std::fs;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "toucan-preprocessor-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let first = directory.join("first");
    let second = directory.join("second");
    let third = directory.join("third");
    for path in [&first, &second, &third] {
        fs::create_dir(path).unwrap();
    }
    fs::write(directory.join("local-only.h"), "").unwrap();
    fs::write(
        directory.join("main.h"),
        "#line 20 \"renamed/main.h\"\n\
         #if !__has_include(\"local-only.h\")\n#error lost physical path\n#endif\n\
         #include <layer.h>\n",
    )
    .unwrap();
    fs::write(first.join("layer.h"), "#ifndef FIRST_LAYER\n#define FIRST_LAYER\nfirst\n#include \"helper.h\"\n#else\nrevisited\n#include_next <layer.h>\n#endif\n").unwrap();
    fs::write(
        first.join("helper.h"),
        "#line 20 \"renamed/helper.h\"\n\
         #if !__has_include(\"helper.h\")\n#error lost physical path\n#endif\n\
         #if __has_include_next(\"helper.h\")\nquery_restarts\n#endif\n\
         #define NEXT_HEADER <layer.h>\n\
         #if __has_include_next(NEXT_HEADER)\n#include_next NEXT_HEADER\n\
         #else\n#error missing next header\n#endif\n",
    )
    .unwrap();
    fs::write(
        second.join("layer.h"),
        "second\n#if __has_include_next(<layer.h>)\n#include_next <layer.h>\n\
         #else\n#error missing resource header\n#endif\n",
    )
    .unwrap();
    let final_header = "#if __has_include_next(<layer.h>)\n#error found another header\n#endif\n\
                        #define NEXT __has_include_next\n\
                        #if NEXT(<layer.h>)\nquery_macro_restarts\n#endif\nthird\n";
    fs::write(third.join("layer.h"), final_header).unwrap();
    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let version = Command::new(&compiler).arg("--version").output().unwrap();
    let clang = String::from_utf8_lossy(&version.stdout).contains("clang");
    let config = Config {
        include_dirs: vec![first.clone(), second.clone()],
        virtual_headers: BTreeMap::from([("layer.h".into(), final_header.into())]),
        defines: if clang {
            BTreeMap::from([("__clang__".into(), "1".into())])
        } else {
            BTreeMap::new()
        },
        ..Config::default()
    };
    let result = Preprocessor::new(config)
        .preprocess(&directory.join("main.h"))
        .unwrap();
    assert_eq!(
        result.source,
        if clang {
            "first\nsecond\nquery_macro_restarts\nthird\n"
        } else {
            "first\nquery_restarts\nrevisited\nsecond\nthird\n"
        }
    );
    assert_eq!(result.dependencies.len(), 4);
    let output = Command::new(compiler)
        .args(["-E", "-P", "-undef", "-x", "c"])
        .arg("-I")
        .arg(first)
        .arg("-I")
        .arg(second)
        .arg("-I")
        .arg(third)
        .arg(directory.join("main.h"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = crate::token::render(
        &crate::token::lex(&String::from_utf8(output.stdout).unwrap()).unwrap(),
    );
    assert_eq!(
        crate::token::render(&crate::token::lex(&result.source).unwrap()),
        expected
    );
}

#[test]
fn digraphs_preserve_spelling_during_stringification_and_pasting() {
    assert_eq!(
        preprocess("%:define S(x) %:x\n%:define C(a,b) a %:%: b\nS(<:) S(<%) C(<,%)\n"),
        "\"<:\" \"<%\" {\n"
    );
}

#[test]
fn wide_character_signedness_uses_the_target_profile() {
    for (wchar_type, expected) in [("int", "signed\n"), ("unsigned int", "unsigned\n")] {
        let config = Config {
            defines: BTreeMap::from([("__WCHAR_TYPE__".into(), wchar_type.into())]),
            ..Config::default()
        };
        let result = Preprocessor::new(config)
            .preprocess_str(
                Path::new("wide.h"),
                "#if L'\\0' - 1 > 0\nunsigned\n#else\nsigned\n#endif\n",
            )
            .unwrap();
        assert_eq!(result.source, expected);
    }
}

#[test]
fn filesystem_access_can_be_disabled_for_embedding() {
    let config = Config {
        allow_filesystem: false,
        virtual_headers: BTreeMap::from([("safe.h".into(), "#pragma once\nint safe;\n".into())]),
        ..Config::default()
    };
    let mut preprocessor = Preprocessor::new(config);
    assert!(
        preprocessor
            .preprocess(Path::new("Cargo.toml"))
            .unwrap_err()
            .message
            .contains("filesystem access is disabled")
    );
    let result = preprocessor
        .preprocess_str(
            Path::new("input.h"),
            "#include <safe.h>\n#include <safe.h>\n",
        )
        .unwrap();
    assert_eq!(result.source, "int safe ;\n");
    assert!(result.dependencies.is_empty());
    let actual_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let result = preprocessor
        .preprocess_str(
            Path::new("input.h"),
            &format!(
                "#if __has_include(\"{}\")\n#error filesystem leaked\n#endif\n",
                actual_file.display()
            ),
        )
        .unwrap();
    assert!(result.source.is_empty());
    assert!(
        preprocessor
            .preprocess_str(
                Path::new("input.h"),
                &format!("#include \"{}\"\n", actual_file.display())
            )
            .is_err()
    );
}

#[test]
fn provenance_distinguishes_source_tokens_from_macro_invocations() {
    use crate::OriginKind;

    let result = Preprocessor::new(Config::default())
        .preprocess_str(
            Path::new("positions.h"),
            "int first;\n#define DECL(name) unsigned name\n  DECL(value);\n",
        )
        .unwrap();
    let direct = result
        .resolve_location(result.source.find("first").unwrap())
        .unwrap();
    assert_eq!(
        (
            direct.path.as_ref(),
            direct.line,
            direct.column,
            direct.kind
        ),
        (Path::new("positions.h"), 1, 5, OriginKind::Token)
    );
    for generated in ["unsigned", "value"] {
        let location = result
            .resolve_location(result.source.find(generated).unwrap())
            .unwrap();
        assert_eq!(
            (location.line, location.column, location.kind),
            (3, 3, OriginKind::MacroInvocation)
        );
    }
    let semicolon = result
        .resolve_location(result.source.rfind(';').unwrap())
        .unwrap();
    assert_eq!(
        (semicolon.line, semicolon.column, semicolon.kind),
        (3, 14, OriginKind::Token)
    );
    assert_eq!(result.mappings.first().unwrap().generated.start, 0);
    assert_eq!(
        result.mappings.last().unwrap().generated.end,
        result.source.len()
    );
    for pair in result.mappings.windows(2) {
        assert_eq!(pair[0].generated.end, pair[1].generated.start);
    }
    assert_eq!(
        result.resolve_location(result.source.len()),
        Some(semicolon)
    );
    assert!(result.resolve_location(result.source.len() + 1).is_none());
}

#[test]
fn provenance_retains_spliced_columns_and_line_remapping() {
    use crate::OriginKind;

    let result = Preprocessor::new(Config::default())
        .preprocess_str(
            Path::new("physical.h"),
            "int\n    foo\\\nbar;\n#line 40 \"logical.h\"\n  int remapped;\n",
        )
        .unwrap();
    let joined = result
        .resolve_location(result.source.find("foobar").unwrap())
        .unwrap();
    assert_eq!(
        (joined.line, joined.column, joined.kind),
        (2, 5, OriginKind::Token)
    );
    let semicolon = result
        .resolve_location(result.source.find(';').unwrap())
        .unwrap();
    assert_eq!((semicolon.line, semicolon.column), (3, 4));
    let remapped = result
        .resolve_location(result.source.find("remapped").unwrap())
        .unwrap();
    assert_eq!(
        (remapped.path.as_ref(), remapped.line, remapped.column),
        (Path::new("logical.h"), 40, 7)
    );
}

#[test]
fn provenance_maps_nested_includes_and_external_macro_definitions() {
    use crate::OriginKind;

    let config = Config {
        allow_filesystem: false,
        virtual_headers: BTreeMap::from([
            (
                "outer.h".into(),
                "#include <inner.h>\n#define DECL(name) unsigned name\n".into(),
            ),
            ("inner.h".into(), "\ntypedef int Inner;\n".into()),
        ]),
        ..Config::default()
    };
    let result = Preprocessor::new(config)
        .preprocess_str(Path::new("main.h"), "#include <outer.h>\n\nDECL(value);\n")
        .unwrap();
    let included = result
        .resolve_location(result.source.find("Inner").unwrap())
        .unwrap();
    assert_eq!(
        (included.path.as_ref(), included.line, included.column),
        (Path::new("<builtin>/inner.h"), 2, 13)
    );
    let expanded = result
        .resolve_location(result.source.find("unsigned").unwrap())
        .unwrap();
    assert_eq!(
        (
            expanded.path.as_ref(),
            expanded.line,
            expanded.column,
            expanded.kind
        ),
        (Path::new("main.h"), 3, 1, OriginKind::MacroInvocation)
    );
}

#[test]
fn provenance_covers_directives_empty_expansions_and_utf8_boundaries() {
    use crate::OriginKind;

    let result = Preprocessor::new(Config::default())
        .preprocess_str(
            Path::new("positions.h"),
            "#pragma pack(1)\n#define EMPTY\nEMPTY\nconst char *s = \"α\";\n",
        )
        .unwrap();
    let pragma = result.resolve_location(0).unwrap();
    assert_eq!(
        (pragma.line, pragma.column, pragma.kind),
        (1, 1, OriginKind::Directive)
    );
    let quote = result.source.find('α').unwrap();
    assert!(result.resolve_location(quote).is_some());
    assert!(result.resolve_location(quote + 1).is_none());
    let result = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("empty.h"), "#define EMPTY\nEMPTY\n")
        .unwrap();
    assert_eq!(result.source, "\n");
    assert_eq!(
        result.resolve_location(0).unwrap().kind,
        OriginKind::MacroInvocation
    );
    assert_eq!(result.resolve_location(0).unwrap().line, 2);
}

#[test]
fn macro_expansion_failures_report_the_invocation_after_prior_declarations() {
    let error = Preprocessor::new(Config::default())
        .preprocess_str(
            Path::new("broken.h"),
            "#define ONE(x) x\nint good;\n   ONE(1, 2)\n",
        )
        .unwrap_err();
    assert_eq!(
        (error.path.as_path(), error.line, error.column),
        (Path::new("broken.h"), 3, 4)
    );
    assert!(error.message.contains("expects 1 arguments"));
}

#[test]
fn trigraphs_precede_splicing_and_retain_original_columns() {
    let source = "int x??(2??);\nint y??/\nname;\n// ignored??/\nstill ignored\n__LINE__\n";
    let result = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("tri.h"), source)
        .unwrap();
    assert_eq!(result.source, "int x [ 2 ] ; int yname ; 6\n");
    for (needle, line, column) in [
        ("[", 1, 6),
        ("2", 1, 9),
        ("]", 1, 10),
        (";", 1, 13),
        ("yname", 2, 5),
    ] {
        let location = result
            .resolve_location(result.source.find(needle).unwrap())
            .unwrap();
        assert_eq!((location.line, location.column), (line, column), "{needle}");
    }
    let semicolon = result
        .resolve_location(result.source.rfind(';').unwrap())
        .unwrap();
    assert_eq!((semicolon.line, semicolon.column), (3, 5));
}

#[test]
fn pragma_operators_preserve_order_and_share_pragma_once_identity() {
    let source = "#define DO(x) _Pragma(#x)\nint before; DO(pack(push, 1)) struct S { char x; int y; }; DO(pack(pop))\n";
    let result = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("pragma.h"), source)
        .unwrap();
    assert_eq!(
        result.source,
        "int before ;\n#pragma pack ( push , 1 )\nstruct S { char x ; int y ; } ;\n#pragma pack ( pop )\n"
    );
    let location = result
        .resolve_location(result.source.find("#pragma").unwrap())
        .unwrap();
    assert_eq!(
        (location.line, location.column, location.kind),
        (2, 13, crate::OriginKind::MacroInvocation)
    );
    let config = Config {
        allow_filesystem: false,
        virtual_headers: BTreeMap::from([(
            "once.h".into(),
            "#line 40 \"logical.h\"\n_Pragma(\"once\")\nint once;\n".into(),
        )]),
        ..Config::default()
    };
    let result = Preprocessor::new(config)
        .preprocess_str(
            Path::new("main.h"),
            "#include <once.h>\n#include <once.h>\n",
        )
        .unwrap();
    assert_eq!(result.source, "int once ;\n");
    let origin = result.resolve_location(0).unwrap();
    assert_eq!(
        (origin.path.as_ref(), origin.line),
        (Path::new("logical.h"), 41)
    );
}

#[test]
fn counter_expands_each_argument_once_and_resets_per_translation_unit() {
    let mut preprocessor = Preprocessor::new(Config::default());
    let source = "#define F(x) x x\n#define S(x) #x\n#define IGNORE(x)\nF(F(__COUNTER__)) IGNORE(__COUNTER__) __COUNTER__ S(__COUNTER__) __COUNTER__\n";
    for _ in 0..2 {
        assert_eq!(
            preprocessor
                .preprocess_str(Path::new("counter.h"), source)
                .unwrap()
                .source,
            "0 0 0 0 1 \"__COUNTER__\" 2\n"
        );
    }
    let result = preprocessor
        .preprocess_str(
            Path::new("counter.h"),
            "#define C __COUNTER__\n#define P _Pragma(\"pack(1)\")\n",
        )
        .unwrap();
    assert!(
        result
            .expand_object_macro("C")
            .unwrap_err()
            .message
            .contains("final macro environment")
    );
    assert!(
        result
            .expand_object_macro("P")
            .unwrap_err()
            .message
            .contains("final macro environment")
    );
}

#[test]
fn pragma_operator_errors_and_work_are_bounded() {
    for source in [
        "_Pragma",
        "_Pragma()",
        "_Pragma(1)",
        "_Pragma(\"pack(1)\", \"pack(2)\")",
        "_Pragma(\"unknown_abi\")",
    ] {
        assert!(
            Preprocessor::new(Config::default())
                .preprocess_str(Path::new("invalid.h"), source)
                .is_err(),
            "{source}"
        );
    }
    let source = "#define P _Pragma(\"pack(push,1)\")\nP P P P P P P P P P\n";
    let config = Config {
        max_tokens: 30,
        ..Config::default()
    };
    assert!(
        Preprocessor::new(config)
            .preprocess_str(Path::new("limit.h"), source)
            .unwrap_err()
            .message
            .contains("token limit")
    );
    let error = Preprocessor::new(Config::default())
        .preprocess_str(
            Path::new("real.h"),
            "#define BAD _Pragma(\"unknown_abi\")\n#line 90 \"logical.h\"\n  BAD\n",
        )
        .unwrap_err();
    assert_eq!(
        (error.path.as_path(), error.line, error.column),
        (Path::new("logical.h"), 90, 3)
    );
}

#[test]
fn pragma_conditions_follow_the_compiler_profile() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let source = "#if _Pragma(\"pack(1)\") 1\nint value;\n#endif\n";
    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let version = Command::new(&compiler).arg("--version").output().unwrap();
    let clang = String::from_utf8_lossy(&version.stdout).contains("clang");
    let mut config = Config::default();
    if clang {
        config.defines.insert("__clang__".into(), "1".into());
    }
    let actual = Preprocessor::new(config).preprocess_str(Path::new("condition.h"), source);
    let mut child = Command::new(compiler)
        .args(["-E", "-P", "-std=c11", "-x", "c", "-"])
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
    let expected = child.wait_with_output().unwrap();
    assert_eq!(actual.is_ok(), expected.status.success());
    if let Ok(actual) = actual {
        assert_eq!(
            crate::token::render(&crate::token::lex(&actual.source).unwrap()),
            crate::token::render(
                &crate::token::lex(&String::from_utf8(expected.stdout).unwrap()).unwrap()
            )
        );
    }
}

#[test]
fn virtual_header_paths_preserve_raw_backslashes() {
    for name in [
        r"C:\temp\new\header.h",
        r"\\server\share\header.h",
        r"raw\\two.h",
        r"escaped\x41\123.h",
    ] {
        let config = Config {
            allow_filesystem: false,
            virtual_headers: BTreeMap::from([(name.into(), "found\n".into())]),
            ..Config::default()
        };
        let source = format!(
            "#define HEADER \"{name}\"\n\
             #if !__has_include(\"{name}\") || !__has_include(HEADER) || !__has_include(<{name}>)\n\
             #error missing raw header\n#endif\n\
             #include \"{name}\"\n#include HEADER\n#include <{name}>\n"
        );
        let result = Preprocessor::new(config)
            .preprocess_str(Path::new("input.h"), &source)
            .unwrap();
        assert_eq!(result.source, "found\nfound\nfound\n", "{name}");
        assert!(result.dependencies.is_empty());
    }
}

#[test]
fn invalid_quoted_header_names_are_diagnosed() {
    for header in [
        "\"\"",
        "\"nul\0.h\"",
        "L\"wide.h\"",
        "\"a.h\" \"b.h\"",
        "\"unclosed",
    ] {
        for source in [
            format!("#include {header}\n"),
            format!("#if __has_include({header})\n#endif\n"),
        ] {
            let error = Preprocessor::new(Config {
                allow_filesystem: false,
                ..Config::default()
            })
            .preprocess_str(Path::new("input.h"), &source)
            .unwrap_err();
            assert_eq!(error.path, Path::new("input.h"));
            assert_eq!(error.line, 1);
            assert!(!error.message.contains("not found"), "{source}: {error}");
        }
    }
}

#[test]
fn native_header_paths_use_host_filesystem_semantics() {
    check_native_header_paths(false);
}

#[test]
#[ignore = "requires a native C compiler (CC or cc); run with --include-ignored"]
fn native_header_paths_match_c_compiler() {
    check_native_header_paths(true);
}

fn check_native_header_paths(compare_compiler: bool) {
    use std::fs;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "toucan-header-paths-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    fs::write(directory.join("native path.h"), "native\n").unwrap();
    fs::write(directory.join("absolute.h"), "absolute\n").unwrap();
    fs::create_dir(directory.join("nested")).unwrap();
    fs::write(directory.join("nested").join("path.h"), "separator\n").unwrap();
    #[cfg(unix)]
    fs::write(directory.join(r"nested\path.h"), "literal\n").unwrap();
    let absolute = directory.join("absolute.h");
    let entry = directory.join("main.h");
    let mut source = format!(
        "#if !__has_include(\"{}\")\n#error missing absolute header\n#endif\n\
         #include \"{}\"\n#include \"native path.h\"\n\
         #include \"nested/path.h\"\n#include \"nested\\path.h\"\n",
        absolute.display(),
        absolute.display()
    );
    // The currently documented angle-header whitespace limit is independent of
    // host path separators. Typical CI temporary paths contain no whitespace.
    let absolute_angle = !absolute.to_string_lossy().chars().any(char::is_whitespace);
    if absolute_angle {
        source.push_str(&format!(
            "#if !__has_include(<{}>)\n#error missing absolute header\n#endif\n#include <{}>\n",
            absolute.display(),
            absolute.display()
        ));
    }
    fs::write(&entry, &source).unwrap();
    let expected = format!(
        "absolute\nnative\nseparator\n{}\n{}",
        if cfg!(windows) {
            "separator"
        } else {
            "literal"
        },
        if absolute_angle { "absolute\n" } else { "" }
    );
    let result = Preprocessor::new(Config::default())
        .preprocess(&entry)
        .unwrap();
    assert_eq!(result.source, expected);
    // A cross target macro profile must not reinterpret paths on the host.
    let other_profile = Config {
        defines: BTreeMap::from([(
            if cfg!(windows) { "__linux__" } else { "_WIN32" }.into(),
            "1".into(),
        )]),
        ..Config::default()
    };
    assert_eq!(
        Preprocessor::new(other_profile)
            .preprocess(&entry)
            .unwrap()
            .source,
        expected
    );

    let disabled = Config {
        allow_filesystem: false,
        ..Config::default()
    };
    let query = format!(
        "#if __has_include(\"{}\")\n#error filesystem leaked\n#endif\n",
        absolute.display()
    );
    let result = Preprocessor::new(disabled.clone())
        .preprocess_str(&entry, &query)
        .unwrap();
    assert!(result.source.is_empty() && result.dependencies.is_empty());
    let error = Preprocessor::new(disabled)
        .preprocess_str(&entry, &source)
        .unwrap_err();
    assert!(error.message.contains("missing absolute header"));

    if !compare_compiler {
        return;
    }

    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let output = Command::new(compiler)
        .args(["-E", "-P", "-x", "c"])
        .arg(&entry)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>(),
        expected.split_whitespace().collect::<Vec<_>>()
    );
}

use std::collections::BTreeMap;
use std::path::Path;

use crate::{Config, Preprocessor};

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
fn builtin_macros_and_line_directive() {
    assert_eq!(
        preprocess("__LINE__ __FILE__\n#line 100 \"logical.h\"\n__LINE__ __FILE__\n"),
        "1 \"test.h\"\n100 \"logical.h\"\n"
    );
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
fn literal_header_names_do_not_expand_macros() {
    let config = Config {
        virtual_headers: BTreeMap::from([("literal.h".into(), "yes\n".into())]),
        ..Config::default()
    };
    assert_eq!(Preprocessor::new(config).preprocess_str(Path::new("test.h"), "#define literal wrong\n#include <literal.h>\n#if __has_include(<literal.h>)\nyes\n#endif\n").unwrap().source, "yes\nyes\n");
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
            .args(["-E", "-P", "-undef", "-std=gnu11", "-x", "c", "-"])
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
    fs::write(directory.join("main.h"), "#include <layer.h>\n").unwrap();
    fs::write(first.join("layer.h"), "#ifndef FIRST_LAYER\n#define FIRST_LAYER\nfirst\n#include \"helper.h\"\n#else\nrevisited\n#include_next <layer.h>\n#endif\n").unwrap();
    fs::write(first.join("helper.h"), "#include_next <layer.h>\n").unwrap();
    fs::write(second.join("layer.h"), "second\n#include_next <layer.h>\n").unwrap();
    fs::write(third.join("layer.h"), "third\n").unwrap();
    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let version = Command::new(&compiler).arg("--version").output().unwrap();
    let clang = String::from_utf8_lossy(&version.stdout).contains("clang");
    let config = Config {
        include_dirs: vec![first.clone(), second.clone()],
        virtual_headers: BTreeMap::from([("layer.h".into(), "third\n".into())]),
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
            "first\nsecond\nthird\n"
        } else {
            "first\nrevisited\nsecond\nthird\n"
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

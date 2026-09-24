use std::path::Path;

use toucan_preprocessor::{Config, Preprocessor};

const PACKING: &str = concat!(
    "#define WHOLE (\"pack(push, 1)\")\n",
    "#define START (\"pack(pop)\"\n",
    "#define END \"pack(push, 2)\")\n",
    "#define OPEN (\n",
    "#define CLOSE )\n",
    "#define PUSH4 \"pack(push, 4)\"\n",
    "#define POP \"pack(pop)\"\n",
    "#define ALIAS _Pragma\n",
    "#define ID(x) x\n",
    "#define EMPTY\n",
    "_Pragma WHOLE\n",
    "struct One { char first; int second; };\n",
    "_Pragma START)\n",
    "_Pragma(END\n",
    "struct Two { char first; int second; };\n",
    "_Pragma OPEN POP CLOSE\n",
    "ALIAS EMPTY OPEN EMPTY ID(PUSH4) EMPTY CLOSE\n",
    "struct Four { char first; int second; };\n",
    "_Pragma(\"pack(pop)\" CLOSE\n",
    "struct Default { char first; int second; };\n",
);

const EXPECTED_PACKING: &str = concat!(
    "#pragma pack(push, 1)\n",
    "struct One { char first; int second; };\n",
    "#pragma pack(pop)\n",
    "#pragma pack(push, 2)\n",
    "struct Two { char first; int second; };\n",
    "#pragma pack(pop)\n",
    "#pragma pack(push, 4)\n",
    "struct Four { char first; int second; };\n",
    "#pragma pack(pop)\n",
    "struct Default { char first; int second; };\n",
);

const MACRO_STATE: &str = concat!(
    "#define VALUE 1\n",
    "#pragma push_macro(\"VALUE\")\n",
    "#undef VALUE\n",
    "#define VALUE 2\n",
    "#define OPEN (\n",
    "#define CLOSE )\n",
    "#define RESTORE \"pop_macro(\\\"VALUE\\\")\"\n",
    "_Pragma OPEN RESTORE CLOSE int restored = VALUE;\n",
);

fn compact(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn pragma_expands_each_syntax_token_before_applying_the_directive() {
    for (source, expected) in [
        (PACKING, EXPECTED_PACKING),
        (MACRO_STATE, "int restored = 1;"),
    ] {
        let result = Preprocessor::new(Config::default())
            .preprocess_str(Path::new("input.h"), source)
            .unwrap();
        assert_eq!(compact(&result.source), compact(expected));
    }
}

#[test]
fn expanded_pragma_tokens_still_require_one_parenthesized_string() {
    for source in [
        "#define OPEN (\n_Pragma OPEN)\n",
        "#define NUMBER 1\n_Pragma(NUMBER)\n",
        "#define TWO \"pack(1)\" \"pack(2)\"\n_Pragma(TWO)\n",
        "#define COMMA ,\n_Pragma(\"pack(1)\" COMMA \"pack(2)\")\n",
        "#define CLOSE ]\n_Pragma(\"pack(1)\" CLOSE\n",
    ] {
        assert!(
            Preprocessor::new(Config::default())
                .preprocess_str(Path::new("input.h"), source)
                .is_err(),
            "{source}"
        );
    }
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn expanded_pragma_tokens_match_native_preprocessors() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    for compiler in ["gcc", "clang"] {
        for (source, expected) in [
            (PACKING, EXPECTED_PACKING),
            (MACRO_STATE, "int restored = 1;"),
        ] {
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
            let output = child.wait_with_output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(true),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                compact(&String::from_utf8(output.stdout).unwrap()),
                compact(expected),
                "{compiler}"
            );
        }
    }
}

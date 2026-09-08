use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::{
    IntegerKind, StringEncoding, analyze, decode_character_literal, decode_string_literals,
    evaluate_integer,
};
use toucan_target::Target;

const NATIVE: Target = Target::X86_64UnknownLinuxGnu;

fn strings(tokens: &[&str], target: Target) -> toucan_semantic::DecodedString {
    let tokens = tokens
        .iter()
        .map(|token| (*token).to_owned())
        .collect::<Vec<_>>();
    decode_string_literals(&tokens, target, 0).unwrap()
}

#[test]
fn escapes_preserve_code_units_and_concatenation_boundaries() {
    let value = strings(&[r#""\a\b\f\n\r\t\v\\\'\"\?\0\1\10\100\xff""#], NATIVE);
    assert_eq!(
        value.to_bytes().unwrap(),
        [7, 8, 12, 10, 13, 9, 11, 92, 39, 34, 63, 0, 1, 8, 64, 255, 0]
    );
    assert_eq!(
        strings(&[r#""\0123\x40""#, r#""A""#], NATIVE).code_units,
        [10, 51, 64, 65, 0]
    );
    assert_eq!(
        strings(&[r#""\x1234""#, r#"L"""#], NATIVE).code_units,
        [0x1234, 0]
    );
    assert_eq!(strings(&[r#"u"\xd800""#], NATIVE).code_units, [0xd800, 0]);
    assert_eq!(
        strings(&[r#"U"\xffffffff""#], NATIVE).code_units,
        [u32::MAX, 0]
    );
    assert_eq!(
        strings(&[r#""\u0024\u0040\u0060""#], NATIVE)
            .to_bytes()
            .unwrap(),
        b"$@`\0"
    );
    assert!(strings(&[r#"u"abc""#], NATIVE).to_bytes().is_none());
    for literal in [
        r#""\x""#,
        r#""\x100""#,
        r#""\400""#,
        r#"u"\x10000""#,
        r#"U"\x100000000""#,
        r#""\8""#,
        r#""\z""#,
        r#""\u0041""#,
        r#""\u0080""#,
        r#""\ud800""#,
        r#""\U00110000""#,
        r#""\u12""#,
    ] {
        let error = decode_string_literals(&[literal.to_owned()], NATIVE, 17).unwrap_err();
        assert_eq!(error.offset, 17, "{literal}");
    }
    for tokens in [[r#"u8"x""#, r#"L"y""#], [r#"u"x""#, r#"U"y""#]] {
        assert!(decode_string_literals(&tokens.map(str::to_owned), NATIVE, 0).is_err());
    }
}

#[test]
fn unicode_scalars_use_each_targets_execution_encoding() {
    for target in Target::ALL {
        for prefix in ["", "u8"] {
            let value = strings(&[&format!(r#"{prefix}"é\u4f60\U0001f600""#)], target);
            assert_eq!(value.to_bytes().unwrap(), "é你😀\0".as_bytes());
            assert_eq!(value.element_type, IntegerKind::Char);
            assert_eq!(
                value.encoding,
                if prefix.is_empty() {
                    StringEncoding::Ordinary
                } else {
                    StringEncoding::Utf8
                }
            );
        }
        let utf16 = strings(&[r#"u"é\u4f60\U0001f600""#], target);
        assert_eq!(utf16.element_type, IntegerKind::UnsignedShort);
        assert_eq!(utf16.code_units, [0xe9, 0x4f60, 0xd83d, 0xde00, 0]);
        let utf32 = strings(&[r#"U"é\u4f60\U0001f600""#], target);
        assert_eq!(utf32.element_type, IntegerKind::UnsignedInt);
        assert_eq!(utf32.code_units, [0xe9, 0x4f60, 0x1f600, 0]);
        let wide = strings(&[r#"L"é\u4f60\U0001f600""#], target);
        assert_eq!(
            wide.code_units,
            if target.wchar_width() == 16 {
                utf16.code_units
            } else {
                utf32.code_units
            }
        );
        assert_eq!(
            wide.element_type,
            if target.wchar_width() == 16 {
                IntegerKind::UnsignedShort
            } else if target.wchar_is_signed() {
                IntegerKind::Int
            } else {
                IntegerKind::UnsignedInt
            }
        );
    }
}

#[test]
fn character_values_and_types_follow_the_target_profile() {
    for target in Target::ALL {
        let unit = analyze("", target).unwrap();
        for (expression, value, size) in [
            (r"'\1'", 1, 4),
            (r"'\10'", 8, 4),
            (r"'\100'", 64, 4),
            (r"'\xff'", if target.char_is_signed() { -1 } else { 255 }, 4),
            ("'ab'", 0x6162, 4),
            ("'abcde'", 0x62636465, 4),
            (r"u'\xffff'", 65535, 2),
            (r"U'\xffffffff'", 4294967295, 4),
            (r"U'\U0001f600'", 0x1f600, 4),
            (r"L'\uffff'", 65535, target.wchar_width() / 8),
        ] {
            assert_eq!(
                evaluate_integer(&unit, expression).unwrap().signed_value(),
                value,
                "{target}: {expression}"
            );
            assert_eq!(
                evaluate_integer(&unit, &format!("sizeof({expression})"))
                    .unwrap()
                    .value,
                u128::from(size)
            );
        }
        let wide = decode_character_literal("L'A'", target, 0).unwrap();
        assert_eq!(u64::from(wide.bits), target.wchar_width());
        assert_eq!(wide.signed, target.wchar_is_signed());
        let gnu = matches!(
            target,
            Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
        );
        for (expression, value) in [("'é'", 0xc3a9), ("L'ab'", 98), (r"u'\U0001f600'", 0xde00)] {
            let actual = decode_character_literal(expression, target, 0);
            if gnu {
                assert_eq!(actual.unwrap().value, value);
            } else {
                assert!(actual.is_err(), "{target}: {expression}");
            }
        }
    }
}

const VALID: &[&str] = &[
    r#"char a[] = "\x40"; _Static_assert(sizeof a == 2, "bound");"#,
    r#"char a[] = "é"; _Static_assert(sizeof a == 3, "bytes");"#,
    r#"unsigned char a[] = u8"é";"#,
    r#"unsigned short a[] = u"😀"; _Static_assert(sizeof a == 6, "surrogates");"#,
    r#"unsigned short a[2] = u"😀";"#,
    r#"unsigned int a[] = U"😀"; _Static_assert(sizeof a == 8, "utf32");"#,
    r#"unsigned int a[][2] = {U"a", U"b"};"#,
    r#"const unsigned short a[] = {u"é"};"#,
    r#"typedef unsigned short A[]; A a = u"a", b = u"abc"; _Static_assert(sizeof a == 4 && sizeof b == 8, "independent bounds");"#,
    r#"unsigned short a[] = "\x1234" u"";"#,
    r#"unsigned short a[] = u"\xd800";"#,
    r#"unsigned int a[] = U"\xffffffff";"#,
    r#"const unsigned int *p = U"hi";"#,
    r#"void f(const unsigned short *); void g(void) { f(u"é"); }"#,
    r#"_Static_assert(_Generic(u'a', unsigned short: 1, default: 0), "char16 type");"#,
    r#"_Static_assert(_Generic(U'a', unsigned int: 1, default: 0), "char32 type");"#,
];

const INVALID: &[&str] = &[
    r#"_Static_assert(1, "\x100");"#,
    r#"int a = sizeof "\x100";"#,
    r#"char a[] = "\x100";"#,
    r#"char a[] = "\400";"#,
    r#"char a[] = "\x";"#,
    r#"char a[] = "\u0041";"#,
    r#"char a[] = "\ud800";"#,
    r#"char a[] = "\U00110000";"#,
    r#"unsigned short a[] = u"\x10000";"#,
    r#"unsigned int a[] = U"\x100000000";"#,
    r#"unsigned short a[1] = u"😀";"#,
    r#"char a[1] = "é";"#,
    r#"char a[] = u8"x" L"y";"#,
    r#"unsigned short a[] = u"x" U"y";"#,
    r#"short a[] = u"x";"#,
    r#"int a[] = U"x";"#,
    r#"int a[] = "x";"#,
    r#"char a[] = U"x";"#,
    r#"unsigned int a[] = u"x";"#,
    r#"int x = '\400';"#,
    r#"int x = '\x100';"#,
];

fn wide_source(target: Target) -> String {
    let ty = if target.wchar_width() == 16 {
        "unsigned short"
    } else if target.wchar_is_signed() {
        "int"
    } else {
        "unsigned int"
    };
    let bound = if target.wchar_width() == 16 { 5 } else { 4 };
    let signed_byte = if target.char_is_signed() { -1 } else { 255 };
    format!(
        r#"
        {ty} a[] = L"é你😀";
        _Static_assert(sizeof a == {bound} * sizeof({ty}), "wide array bound");
        _Static_assert(_Generic(L'A', {ty}: 1, default: 0), "wchar type");
        _Static_assert('\xff' == {signed_byte}, "plain char signedness");
    "#
    )
}

#[test]
fn literal_types_check_initializers_on_every_target() {
    for target in Target::ALL {
        let wide = wide_source(target);
        for source in VALID.iter().copied().chain(std::iter::once(wide.as_str())) {
            analyze(source, target).unwrap_or_else(|error| panic!("{target}: {source}: {error}"));
        }
        for source in INVALID {
            assert!(
                analyze(source, target).is_err(),
                "{target}: accepted {source}"
            );
        }
        if target != Target::X86_64PcWindowsMsvc {
            analyze(r#"enum E { X }; enum E a[] = U"x";"#, target).unwrap();
        }
    }
}

#[test]
fn decoding_limits_public_input_before_allocating_code_units() {
    let source = format!("\"{}\"", "a".repeat(16 * 1024 * 1024));
    assert!(
        decode_string_literals(&[source], NATIVE, 0)
            .unwrap_err()
            .message
            .contains("limit")
    );
}

fn compile(compiler: &str, source: &str, args: &[&str]) -> std::process::Output {
    let mut child = Command::new(compiler)
        .args(["-std=c11", "-pedantic-errors", "-x", "c", "-"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(source.as_bytes()).unwrap();
    stdin.write_all(b"\n").unwrap();
    drop(stdin);
    child.wait_with_output().unwrap()
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn literal_constraints_match_native_compilers() {
    for compiler in ["gcc", "clang"] {
        for (sources, valid) in [(VALID, true), (INVALID, false)] {
            for source in sources {
                let output = compile(compiler, source, &["-fsyntax-only"]);
                assert_eq!(
                    output.status.success(),
                    valid,
                    "{compiler}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        let output = compile(compiler, &wide_source(NATIVE), &["-fsyntax-only"]);
        assert!(
            output.status.success(),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[ignore = "requires Clang with all five target backends; run with --include-ignored"]
fn literal_types_match_clang_on_every_target() {
    for target in Target::ALL {
        let wide = wide_source(target);
        for source in VALID.iter().copied().chain(std::iter::once(wide.as_str())) {
            let output = compile(
                "clang",
                source,
                &["-fsyntax-only", "-target", target.triple()],
            );
            assert!(
                output.status.success(),
                "{target}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn universal_character_spelling_survives_the_parser_adapter() {
    let source = r#"enum { A = U'\u00e9', B = U'\U0001f600' }; _Static_assert(A == 233 && B == 128512, "values");"#;
    analyze(source, NATIVE).unwrap();
    let source = r#"int x; enum { X = U'\u0041' };"#;
    let error = analyze(source, NATIVE).unwrap_err();
    assert_eq!(error.offset, source.find("U'").unwrap());
    assert!(error.message.contains("universal character name"));
}

#[test]
#[ignore = "requires native GCC and Clang; run with --include-ignored"]
fn code_units_and_character_values_match_native_compilers() {
    let literals: &[&[&str]] = &[
        &[r#""\a\b\f\n\r\t\v\\\'\"\?\0\1\10\100\xff""#],
        &[r#""\0123\x40""#, r#""A""#],
        &[r#""é你😀""#],
        &[r#"u8"é\u4f60\U0001f600""#],
        &[r#"u"é\u4f60\U0001f600""#],
        &[r#"U"é\u4f60\U0001f600""#],
        &[r#"L"é你😀""#],
        &[r#""\x1234""#, r#"L"""#],
        &[r#"u"\xd800""#],
        &[r#"U"\xffffffff""#],
        &[r#"L"\xffffffff""#],
    ];
    let mut declarations = String::new();
    let mut checks = String::new();
    for (index, tokens) in literals.iter().enumerate() {
        let value = strings(tokens, NATIVE);
        let (ty, bits) = match value.element_type {
            IntegerKind::Char => ("char", 8),
            IntegerKind::UnsignedShort => ("unsigned short", 16),
            IntegerKind::Int => ("int", 32),
            IntegerKind::UnsignedInt => ("unsigned int", 32),
            _ => unreachable!(),
        };
        declarations.push_str(&format!("{ty} a{index}[] = {};\n", tokens.join(" ")));
        declarations.push_str(&format!(
            "_Static_assert(sizeof a{index} == {} * sizeof({ty}), \"bound\");\n",
            value.code_units.len()
        ));
        let mask = (1u64 << bits) - 1;
        for (offset, unit) in value.code_units.iter().enumerate() {
            checks.push_str(&format!(
                "if (((unsigned int)a{index}[{offset}] & {mask}u) != {unit}u) return 1;\n"
            ));
        }
    }
    for compiler in ["gcc", "clang"] {
        let mut character_checks = String::new();
        let mut characters = vec![
            r"'\xff'",
            "'ab'",
            "'abcde'",
            r"u'\xd800'",
            r"U'\xffffffff'",
            r"L'\xffffffff'",
            r"U'\U0001f600'",
        ];
        if compiler == "gcc" {
            characters.extend(["'é'", "L'ab'", r"u'\U0001f600'"]);
        }
        for character in characters {
            let value = decode_character_literal(character, NATIVE, 0).unwrap();
            character_checks.push_str(&format!(
                "if ((long long)({character}) != {}LL) return 2;\n",
                value.signed_value()
            ));
        }
        let source =
            format!("{declarations}\nint main(void) {{\n{checks}{character_checks}return 0;\n}}\n");
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("literals");
        let output = compile(compiler, &source, &["-o", executable.to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            Command::new(&executable).status().unwrap().success(),
            "{compiler}: literal values differ"
        );
        if compiler == "clang" {
            for character in ["'é'", "L'ab'", r"u'\U0001f600'"] {
                assert!(
                    !compile(
                        compiler,
                        &format!("int x = {character};"),
                        &["-fsyntax-only"]
                    )
                    .status
                    .success()
                );
            }
        }
    }
}

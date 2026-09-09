#[path = "../src/macro_values.rs"]
mod macro_values;

use macro_values::{Character, Context, ErrorKind, Limits, Value};
use toucan::{CompilerProfile, LanguageMode, Target};
use toucan_preprocessor::Macro;

fn profile() -> CompilerProfile {
    CompilerProfile::default_for(Target::X86_64UnknownLinuxGnu)
}

fn definition(replacement: &str) -> Macro {
    Macro {
        parameters: None,
        variadic: false,
        variadic_parameter: None,
        replacement: replacement.into(),
    }
}

fn value(context: &mut Context, name: &str, replacement: &str) -> Value {
    context
        .define(name, &definition(replacement), false)
        .unwrap()
        .unwrap()
        .value
}

#[test]
fn sequential_values_preserve_first_emission_and_update_duplicates() {
    let mut context = Context::new(Limits::default(), profile());
    assert_eq!(value(&mut context, "A", "1"), Value::Integer(1));
    assert_eq!(value(&mut context, "B", "A"), Value::Integer(1));
    let duplicate = context
        .define("A", &definition("A+1"), false)
        .unwrap()
        .unwrap();
    assert!(!duplicate.first_definition);
    assert_eq!(duplicate.value, Value::Integer(2));
    assert_eq!(value(&mut context, "C", "B"), Value::Integer(1));
    assert_eq!(value(&mut context, "D", "A"), Value::Integer(2));
    assert!(
        context
            .define("Unknown", &definition("missing"), false)
            .is_err()
    );
    assert!(
        context
            .define("Unknown", &definition("3"), false)
            .unwrap()
            .unwrap()
            .first_definition
    );
    assert!(
        context
            .note("Invalid", Value::Invalid)
            .unwrap()
            .first_definition
    );
    assert_eq!(value(&mut context, "Alias", "Invalid"), Value::Invalid);
    assert!(
        !context
            .define("Invalid", &definition("4"), false)
            .unwrap()
            .unwrap()
            .first_definition
    );
}

#[test]
fn numeric_and_literal_values_are_the_reference_domain() {
    let mut context = Context::new(Limits::default(), profile());
    for (source, expected) in [
        ("18446744073709551615ULL", -1),
        ("1<<64", 1),
        ("-8>>1", -4),
        ("1<<-1", i64::MIN),
        ("9223372036854775807+1", i64::MIN),
        ("(-9223372036854775807-1)/-1", i64::MIN),
        ("(-9223372036854775807-1)%-1", 0),
        ("1+2*3<<1|1", 15),
        ("0x12Ull", 18),
        ("~0", -1),
    ] {
        assert_eq!(
            value(&mut context, "value", source),
            Value::Integer(expected),
            "{source}"
        );
    }
    assert_eq!(value(&mut context, "Float", "1.5f+2"), Value::Float(3.5));
    assert_eq!(
        value(&mut context, "Bytes", "L\"a\" u\"b\\xFF\""),
        Value::Bytes(vec![b'a', b'b', 255])
    );
    assert_eq!(
        value(&mut context, "Alias", "(Bytes)"),
        Value::Bytes(vec![b'a', b'b', 255])
    );
    assert_eq!(
        value(&mut context, "Ascii", "'a'"),
        Value::Character(Character::Unicode('a'))
    );
    assert_eq!(
        value(&mut context, "Wide", "'\\x100'"),
        Value::Character(Character::Raw(256))
    );
    assert_eq!(
        value(&mut context, "Unicode", "L'\\u00e9'"),
        Value::Character(Character::Unicode('é'))
    );
    for source in [
        "'a'+1",
        "sizeof(int)",
        "(unsigned)-1",
        "1&&2",
        "1?2:3",
        "1.0<<2",
        "18446744073709551616",
        "0x1p0",
        "1++2",
        "(\"a\") \"b\"",
    ] {
        assert!(
            context
                .define("failed", &definition(source), false)
                .is_err(),
            "{source}"
        );
    }
}

#[test]
fn integer_zero_division_is_checked_without_mutation() {
    let mut context = Context::new(Limits::default(), profile());
    value(&mut context, "A", "7");
    for source in ["1/0", "1%(3-3)"] {
        let error = context.define("A", &definition(source), false).unwrap_err();
        assert_eq!(error.kind, ErrorKind::DivisionByZero);
        assert_eq!(error.offset, Some(1));
    }
    assert_eq!(value(&mut context, "B", "A"), Value::Integer(7));
    assert!(matches!(value(&mut context,"Inf","1.0/0.0"),Value::Float(f) if f.is_infinite()));
}

#[test]
fn callback_presence_controls_function_parameter_expression_quirk() {
    let mut context = Context::new(Limits::default(), profile());
    value(&mut context, "Known", "9");
    // The caller's final classification applies even to an earlier object macro.
    assert!(
        context
            .define("EarlierObject", &definition("7"), true)
            .unwrap()
            .is_none()
    );
    assert!(
        context
            .define("EarlierObject", &definition("11"), false)
            .unwrap()
            .unwrap()
            .first_definition
    );
    let mut function = definition("");
    function.parameters = Some(vec!["Known".into()]);
    assert_eq!(
        context
            .define("NoCallback", &function, false)
            .unwrap()
            .unwrap()
            .value,
        Value::Integer(9)
    );
    assert!(
        context
            .define("Callback", &function, true)
            .unwrap()
            .is_none()
    );
    function.replacement = "-3".into();
    assert_eq!(
        context
            .define("Arithmetic", &function, false)
            .unwrap()
            .unwrap()
            .value,
        Value::Integer(6)
    );
    function.parameters = Some(vec!["Unknown".into()]);
    let error = context.define("Missing", &function, false).unwrap_err();
    assert_eq!(error.kind, ErrorKind::UnknownIdentifier);
    assert_eq!(error.offset, None);
    function.parameters = Some(vec!["Known".into()]);
    function.replacement = "/0".into();
    let error = context.define("Zero", &function, false).unwrap_err();
    assert_eq!(error.kind, ErrorKind::DivisionByZero);
    assert_eq!(error.offset, Some(0));
}

#[test]
fn every_growth_boundary_reports_a_limit() {
    let defaults = Limits::default();
    for (limits, source, expected) in [
        (
            Limits {
                source_bytes: 8,
                ..defaults
            },
            "12345678",
            ErrorKind::SourceLimit,
        ),
        (
            Limits {
                tokens: 2,
                ..defaults
            },
            "1+2",
            ErrorKind::TokenLimit,
        ),
        (
            Limits {
                depth: 8,
                ..defaults
            },
            "((((((((1))))))))",
            ErrorKind::DepthLimit,
        ),
        (
            Limits {
                string_bytes: 2,
                ..defaults
            },
            "\"abc\"",
            ErrorKind::StringLimit,
        ),
        (
            Limits {
                total_work: 2,
                ..defaults
            },
            "1+2",
            ErrorKind::WorkLimit,
        ),
        (
            Limits {
                context_entries: 0,
                ..defaults
            },
            "1",
            ErrorKind::ContextLimit,
        ),
        (
            Limits {
                context_bytes: 0,
                ..defaults
            },
            "1",
            ErrorKind::ContextLimit,
        ),
    ] {
        let error = Context::new(limits, profile())
            .define("A", &definition(source), false)
            .unwrap_err();
        assert_eq!(error.kind, expected, "{source}");
    }
    let deeply_nested = format!("{}1{}", "(".repeat(512), ")".repeat(512));
    assert_eq!(
        Context::new(
            Limits {
                depth: usize::MAX,
                ..defaults
            },
            profile()
        )
        .define("A", &definition(&deeply_nested), false)
        .unwrap_err()
        .kind,
        ErrorKind::DepthLimit,
    );
    let mut context = Context::new(
        Limits {
            string_bytes: 16,
            ..defaults
        },
        profile(),
    );
    value(&mut context, "A", "\"12345678\"");
    assert_eq!(
        value(&mut context, "B", "A A"),
        Value::Bytes(b"1234567812345678".to_vec())
    );
    assert_eq!(
        context
            .define("C", &definition("B B"), false)
            .unwrap_err()
            .kind,
        ErrorKind::StringLimit
    );
    let mut context = Context::new(
        Limits {
            total_work: 64,
            ..defaults
        },
        profile(),
    );
    let mut failed = 0;
    loop {
        match context.define("A", &definition("missing"), false) {
            Err(error) if error.kind == ErrorKind::WorkLimit => break,
            Err(_) => failed += 1,
            _ => panic!("unknown identifier accepted"),
        }
        assert!(failed < 10);
    }
    assert!(failed > 0);
}

#[test]
fn cursor_keyword_tokens_follow_language_and_target() {
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            let mut context = Context::new(Limits::default(), profile);
            let c99 = !matches!(mode, LanguageMode::C90 | LanguageMode::Gnu90);
            for (name, keyword) in [
                ("int", true),
                ("__typeof__", true),
                ("__null", false),
                ("_Float32", false),
                ("__builtin_FILE", true),
                ("restrict", c99),
                ("inline", c99 || mode.is_gnu()),
                ("asm", mode.is_gnu()),
                ("typeof", mode.is_gnu()),
                ("__declspec", profile.target().is_windows()),
                ("static_assert", profile.target().is_windows()),
            ] {
                let result = context.define(name, &definition("7"), false);
                assert_eq!(result.is_err(), keyword, "{profile:?}: {name}");
                context.note(name, Value::Integer(7)).unwrap();
                let result = context.define("alias", &definition(name), false);
                assert_eq!(result.is_err(), keyword, "{profile:?}: {name}");
                if let Err(error) = result {
                    assert_eq!(error.kind, ErrorKind::Keyword);
                    assert_eq!(error.offset, Some(0));
                }
            }
        }
    }
}

#[test]
fn malformed_tokens_and_alias_growth_stay_within_explicit_budgets() {
    let limits = Limits {
        source_bytes: 512,
        tokens: 128,
        depth: 16,
        string_bytes: 64,
        context_entries: 8,
        context_bytes: 1024,
        total_work: 16_384,
    };
    let alphabet = b"0123456789abcdefxULu'\"\\()+-*/%<>&|^!?:,._ \n";
    let mut random = 0x971a_413bu32;
    for length in 0..256 {
        for _ in 0..8 {
            let mut source = String::new();
            for _ in 0..length {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                source.push(char::from(alphabet[random as usize % alphabet.len()]));
            }
            let mut context = Context::new(limits, profile());
            let _ = context.define("input", &definition(&source), false);
        }
    }
    let mut context = Context::new(limits, profile());
    value(&mut context, "Bytes", "\"12345678\"");
    for expected in [16, 32, 64] {
        assert!(
            matches!(value(&mut context, "Bytes", "Bytes Bytes"), Value::Bytes(bytes) if bytes.len()==expected)
        );
    }
    assert_eq!(
        context
            .define("Bytes", &definition("Bytes Bytes"), false)
            .unwrap_err()
            .kind,
        ErrorKind::StringLimit,
    );
    assert!(
        matches!(value(&mut context, "After", "Bytes"), Value::Bytes(bytes) if bytes.len()==64)
    );
    for source in [
        "\"unterminated",
        "'\\x'",
        "'\\UFFFFFFFF'",
        "0b2",
        "0x",
        "1e+",
        "0x1p0",
    ] {
        assert_eq!(
            context
                .define("Bad", &definition(source), false)
                .unwrap_err()
                .kind,
            ErrorKind::InvalidLiteral,
            "{source}"
        );
    }
}

#[test]
fn literal_prefix_tokens_follow_the_cursor_language_mode() {
    for mode in LanguageMode::ALL {
        let mut context = Context::new(Limits::default(), profile().with_language_mode(mode));
        let unicode = matches!(
            mode,
            LanguageMode::C11 | LanguageMode::Gnu11 | LanguageMode::C17 | LanguageMode::Gnu17
        );
        for source in ["u8\"a\"", "u\"a\"", "U\"a\"", "u'a'", "U'a'"] {
            assert_eq!(
                context.define("A", &definition(source), false).is_ok(),
                unicode,
                "{mode:?}: {source}"
            );
        }
        assert!(context.define("A", &definition("u8'a'"), false).is_err());
        assert!(context.define("A", &definition("L'a'"), false).is_ok());
        value(&mut context, "u8", "\"prefix\"");
        let actual = value(&mut context, "Split", "u8\"suffix\"");
        assert_eq!(
            actual,
            Value::Bytes(if unicode {
                b"suffix".to_vec()
            } else {
                b"prefixsuffix".to_vec()
            })
        );
    }
}

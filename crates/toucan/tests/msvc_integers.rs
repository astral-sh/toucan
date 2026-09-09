use std::path::Path;
use std::process::Command;

use toucan::{AnalysisOptions, Compiler, CompilerProfile, Config, Target};

const CASES: &[(&str, bool)] = &[
    ("__int8 x;", true),
    ("signed __int8 x;", true),
    ("unsigned __int8 x;", true),
    ("int __int8 x;", false),
    ("__int8 __int8 x;", false),
    ("__int16 x;", true),
    ("int __int16 x;", true),
    ("short __int16 x;", true),
    ("__int16 short x;", true),
    ("__int16 __int16 x;", true),
    ("long __int16 x;", false),
    ("__int32 x;", true),
    ("signed __int32 x;", true),
    ("unsigned __int32 x;", true),
    ("long __int32 x;", true),
    ("short __int32 x;", true),
    ("int __int32 x;", false),
    ("__int32 __int32 x;", false),
    ("__int64 x;", true),
    ("signed __int64 x;", true),
    ("unsigned __int64 x;", true),
    ("int __int64 x;", true),
    ("long __int64 x;", true),
    ("long long __int64 x;", true),
    ("__int64 long x;", false),
    ("__int64 __int64 x;", true),
    ("__int64 __int64 __int64 x;", true),
    ("long long long __int64 x;", false),
    ("short __int64 x;", false),
    ("__int16 __int32 x;", true),
    ("__int64 __int32 x;", true),
    ("double __int64 x;", false),
    ("float __int32 x;", false),
    ("unsigned signed __int64 x;", false),
    (
        "typedef unsigned __int64 U; U f(U x){U y=(U)3; return x+y;}",
        true,
    ),
    (
        "struct S {__int8 x; __int16 y; __int32 z; __int64 w;};",
        true,
    ),
    (
        "typedef __int64 (*F)(__int32); F f(F callback){return callback;}",
        true,
    ),
];

#[test]
fn microsoft_integer_keywords_follow_the_target_and_preserve_retained_types() {
    for profile in CompilerProfile::ALL {
        for &(source, valid) in CASES {
            for single_underscore in [false, true] {
                let source = if single_underscore {
                    source.replace("__int", "_int")
                } else {
                    source.to_owned()
                };
                let plain = toucan::semantic::analyze_with_profile(
                    &source,
                    profile,
                    &AnalysisOptions::default(),
                );
                let retained = toucan::semantic::analyze_with_profile(
                    &source,
                    profile,
                    &AnalysisOptions {
                        retain_code: true,
                        ..Default::default()
                    },
                );
                assert_eq!(
                    plain.is_ok(),
                    valid && profile.target().is_windows(),
                    "{profile:?}: {source}: {plain:?}"
                );
                match (plain, retained) {
                    (Ok(plain), Ok(retained)) => assert_eq!(
                        serde_json::to_value(plain.unit()).unwrap(),
                        serde_json::to_value(retained.unit()).unwrap(),
                    ),
                    (Err(plain), Err(retained)) => assert_eq!(
                        (plain.offset, plain.message),
                        (retained.offset, retained.message),
                    ),
                    values => panic!("ordinary/retained mismatch: {values:?}"),
                }
            }
        }
        if !profile.target().is_windows() {
            toucan::semantic::analyze_with_profile(
                "int __int8; typedef int __int64; __int64 f(__int64 x){return x+__int8;}",
                profile,
                &AnalysisOptions::default(),
            )
            .unwrap();
        }
    }
}

const API: &str = r#"
typedef __int8 Byte;
typedef signed __int8 SignedByte;
typedef unsigned __int16 Word;
typedef __int32 Number;
typedef unsigned __int64 Wide;
struct Values {Byte byte; SignedByte signed_byte; Word word; Number number; Wide wide;};
Wide convert(Number, struct Values);
#define WIDE_MAX ((unsigned __int64)-1)
_Static_assert(_Generic((Byte)0, char:1, default:0), "plain char identity");
_Static_assert(_Generic((SignedByte)0, signed char:1, default:0), "signed char identity");
_Static_assert(_Generic((Word)0, unsigned short:1, default:0), "short identity");
_Static_assert(_Generic((Number)0, int:1, default:0), "int identity");
_Static_assert(_Generic((Wide)0, unsigned long long:1, default:0), "long long identity");
_Static_assert(sizeof(struct Values)==16 && _Alignof(struct Values)==8, "layout");
"#;

#[test]
fn microsoft_integer_api_and_macros_match_the_canonical_c_types() {
    let config = Config::new(Target::X86_64PcWindowsMsvc);
    let canonical = API
        .replace("__int8", "char")
        .replace("__int16", "short")
        .replace("__int32", "int")
        .replace("__int64", "long long");
    let compile = |source| {
        toucan::parse_source(Path::new("integers.h"), source, &config)
            .unwrap()
            .bindings(&Default::default())
            .unwrap()
            .0
    };
    assert_eq!(compile(API), compile(&canonical));
    let compilation = toucan::parse_source(Path::new("integers.h"), API, &config).unwrap();
    assert_eq!(
        toucan::semantic::evaluate_integer(compilation.unit(), "(unsigned __int64)-1")
            .unwrap()
            .as_u64()
            .unwrap(),
        u64::MAX
    );
}

#[test]
#[ignore = "requires Clang's supported target frontends"]
fn microsoft_integer_syntax_and_type_identity_match_clang() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("integers.c");
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|profile| profile.compiler() == Compiler::Clang)
    {
        for &(source, _) in CASES {
            for single_underscore in [false, true] {
                let source = if single_underscore {
                    source.replace("__int", "_int")
                } else {
                    source.into()
                };
                std::fs::write(&input, &source).unwrap();
                let output = Command::new("clang")
                    .args([
                        "-target",
                        profile.target().triple(),
                        "-std=gnu11",
                        "-fsyntax-only",
                    ])
                    .arg(&input)
                    .output()
                    .unwrap();
                let accepted = toucan_test_support::compiler_acceptance(&output).unwrap();
                let result = toucan::semantic::analyze_with_profile(
                    &source,
                    profile,
                    &AnalysisOptions::default(),
                );
                assert_eq!(
                    result.is_ok(),
                    accepted,
                    "{profile:?}: {source}: {result:?}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
    std::fs::write(&input, API).unwrap();
    let output = Command::new("clang")
        .args([
            "-target",
            "x86_64-pc-windows-msvc",
            "-std=gnu11",
            "-fsyntax-only",
        ])
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        toucan_test_support::compiler_acceptance(&output).unwrap(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

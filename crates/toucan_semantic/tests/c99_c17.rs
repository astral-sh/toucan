use toucan_semantic::{AnalysisOptions, analyze_with_profile, evaluate_integer};
use toucan_target::{Compiler, CompilerProfile, LanguageMode, Target};

const MODES: [LanguageMode; 4] = [
    LanguageMode::C99,
    LanguageMode::Gnu99,
    LanguageMode::C17,
    LanguageMode::Gnu17,
];

fn cases(profile: CompilerProfile) -> Vec<(&'static str, bool)> {
    let mode = profile.language_mode();
    let unicode =
        mode.is_c11() || (mode == LanguageMode::Gnu99 && profile.compiler() == Compiler::Gnu);
    vec![
        ("inline int f(int *restrict p){return *p;}", true),
        ("int inline;int f(void){return inline;}", false),
        ("int restrict;int f(void){return restrict;}", false),
        ("int asm; typedef int typeof; typeof x;", !mode.is_gnu()),
        ("int x;typeof(x) y;", mode.is_gnu()),
        ("int x=1; // line comment\nint y=2;", true),
        (
            "int f(void){int x=0;for(int i=0;i<3;i++)x+=i;return x;}",
            true,
        ),
        ("object;", false),
        ("int f(void){return missing(1);}", false),
        ("int f(a){return a;}", false),
        ("int f(a) int a;{return a;}", true),
        ("const char*s=u8\"hello\";", unicode),
        ("const unsigned short*s=u\"hello\";", unicode),
        ("const unsigned int*s=U\"hello\";", unicode),
        ("int x=u'a';", unicode),
        ("int x=U'a';", unicode),
        ("const void*s=L\"hello\";", true),
        ("_Atomic(int) x; int y=_Generic(0,int:1,default:2);", true),
        (
            "_Alignas(16) int x; _Static_assert(_Alignof(int)==4,\"int\");",
            true,
        ),
        ("_Thread_local int x; _Noreturn void f(void);", true),
        ("_Bool b; double _Complex z; double d=0x1.8p+2;", true),
        ("struct S{int a,b;};struct S s=(struct S){.b=4};", true),
        ("int f(int n){int a[n];return sizeof(a);}", true),
        ("struct S{int n;char data[];};", true),
        ("int x=1'000;", false),
    ]
}

#[test]
fn c99_and_c17_keep_compiler_extensions_and_constant_query_modes() {
    for profile in CompilerProfile::ALL {
        for mode in MODES {
            let profile = profile.with_language_mode(mode);
            for (source, expected) in cases(profile) {
                let ordinary = analyze_with_profile(source, profile, &Default::default());
                let retained = analyze_with_profile(
                    source,
                    profile,
                    &AnalysisOptions {
                        retain_code: true,
                        ..Default::default()
                    },
                );
                assert_eq!(
                    ordinary.is_ok(),
                    expected,
                    "{profile:?}: {source}: {ordinary:?}"
                );
                match (&ordinary, &retained) {
                    (Ok(a), Ok(b)) => {
                        assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit()))
                    }
                    (Err(a), Err(b)) => assert_eq!((&a.message, a.offset), (&b.message, b.offset)),
                    _ => panic!("retention disagreement: {profile:?}: {source}"),
                }
            }
            let analysis = analyze_with_profile("", profile, &Default::default()).unwrap();
            assert_eq!(analysis.unit().profile().unwrap(), profile);
            for expression in ["u'a'", "U'a'"] {
                let value = evaluate_integer(analysis.unit(), expression);
                let unicode = mode.is_c11()
                    || (mode == LanguageMode::Gnu99 && profile.compiler() == Compiler::Gnu);
                assert_eq!(
                    value.is_ok(),
                    unicode,
                    "{profile:?}: {expression}: {value:?}"
                );
                if let Ok(value) = value {
                    assert_eq!(value.value, 97);
                }
            }
            let expected = if profile.target().long_width() == 64 {
                "long"
            } else {
                "long long"
            };
            let expression =
                format!("__builtin_types_compatible_p(__typeof__(2147483648),{expected})");
            assert_eq!(
                evaluate_integer(analysis.unit(), &expression)
                    .unwrap()
                    .value,
                1,
                "{profile:?}"
            );
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang"]
fn c99_and_c17_admission_matches_native_compilers() {
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("modes.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let clang = std::env::var("TOUCAN_CLANG").unwrap_or_else(|_| "clang".into());
    let native = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Some(Target::X86_64UnknownLinuxGnu),
        ("aarch64", "linux") => Some(Target::Aarch64UnknownLinuxGnu),
        _ => None,
    };
    for profile in CompilerProfile::ALL {
        if profile.compiler() == Compiler::Gnu && Some(profile.target()) != native {
            continue;
        }
        for mode in MODES {
            let profile = profile.with_language_mode(mode);
            for (source, expected) in cases(profile) {
                std::fs::write(&input, source).unwrap();
                let mut command = Command::new(if profile.compiler() == Compiler::Gnu {
                    &gcc
                } else {
                    &clang
                });
                command.args([
                    format!("-std={mode}"),
                    "-fsyntax-only".into(),
                    "-Werror=implicit-int".into(),
                    "-Werror=implicit-function-declaration".into(),
                ]);
                if profile.compiler() == Compiler::Clang {
                    command.arg(format!("--target={}", profile.target()));
                }
                if matches!(
                    profile.target(),
                    Target::X86_64AppleDarwin | Target::Aarch64AppleDarwin
                ) {
                    command.arg("-mmacosx-version-min=11.0");
                }
                let output = command.arg(&input).output().unwrap();
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&output),
                    Ok(expected),
                    "{profile:?}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(
                    analyze_with_profile(source, profile, &Default::default()).is_ok(),
                    expected,
                    "{profile:?}: {source}"
                );
            }
        }
    }
}

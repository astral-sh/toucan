use std::process::Command;

use toucan_semantic::{
    AnalysisOptions, FunctionDefinitionKind as Kind, SymbolBinding as Binding, analyze_with_profile,
};
use toucan_target::{Compiler, CompilerProfile, LanguageMode, Target};

type Outcome = Option<(Kind, Binding)>;
const EXTERNAL: Outcome = Some((Kind::External, Binding::Strong));
const EXTERNAL_WEAK: Outcome = Some((Kind::External, Binding::Weak));
const INLINE: Outcome = Some((Kind::InlineOnly, Binding::Strong));
const INLINE_WEAK_REFERENCE: Outcome = Some((Kind::InlineOnly, Binding::Weak));
const WEAK_INLINE: Outcome = Some((Kind::WeakInline, Binding::Weak));
const MICROSOFT: Outcome = Some((Kind::MicrosoftInline, Binding::Strong));
const MICROSOFT_WEAK: Outcome = Some((Kind::MicrosoftInline, Binding::Weak));
const MICROSOFT_EXTERN: Outcome = Some((Kind::MicrosoftExternInline, Binding::Strong));
const MICROSOFT_EXTERN_WEAK: Outcome = Some((Kind::MicrosoftExternInline, Binding::Weak));

// Columns: GNU90, GNU11, Clang90, Clang11, Microsoft90, Microsoft11.
const CASES: &[(&str, [Outcome; 6])] = &[
    (
        "__inline__ __attribute__((weak)) int f(void){return 1;}",
        [
            EXTERNAL,
            INLINE,
            EXTERNAL_WEAK,
            WEAK_INLINE,
            MICROSOFT_WEAK,
            MICROSOFT_WEAK,
        ],
    ),
    (
        "extern __inline__ __attribute__((weak)) int f(void){return 1;}",
        [
            INLINE,
            EXTERNAL,
            WEAK_INLINE,
            EXTERNAL_WEAK,
            MICROSOFT_EXTERN_WEAK,
            MICROSOFT_EXTERN_WEAK,
        ],
    ),
    (
        "int f(void) __attribute__((weak)); __inline__ int f(void){return 1;}",
        [
            EXTERNAL_WEAK,
            EXTERNAL_WEAK,
            EXTERNAL_WEAK,
            EXTERNAL_WEAK,
            MICROSOFT_WEAK,
            MICROSOFT_WEAK,
        ],
    ),
    (
        "__inline__ int f(void){return 1;} int f(void) __attribute__((weak));",
        [
            EXTERNAL_WEAK,
            EXTERNAL_WEAK,
            EXTERNAL,
            EXTERNAL,
            MICROSOFT,
            MICROSOFT,
        ],
    ),
    (
        "__inline__ int f(void){return 1;} __inline__ int f(void) __attribute__((weak));",
        [EXTERNAL, INLINE, EXTERNAL, INLINE, MICROSOFT, MICROSOFT],
    ),
    (
        "void other(void){int f(void) __attribute__((weak));} __inline__ int f(void){return 1;}",
        [
            EXTERNAL_WEAK,
            EXTERNAL_WEAK,
            EXTERNAL_WEAK,
            WEAK_INLINE,
            MICROSOFT_WEAK,
            MICROSOFT_WEAK,
        ],
    ),
    (
        "__inline__ int f(void){return 1;} void other(void){int f(void) __attribute__((weak));}",
        [
            EXTERNAL_WEAK,
            INLINE_WEAK_REFERENCE,
            EXTERNAL,
            INLINE,
            MICROSOFT,
            MICROSOFT,
        ],
    ),
    (
        "extern __inline__ __attribute__((weak)) int f(void){return 1;} int f(void){return 2;}",
        [
            EXTERNAL,
            None,
            EXTERNAL_WEAK,
            None,
            MICROSOFT_EXTERN_WEAK,
            None,
        ],
    ),
    (
        "extern __inline__ int f(void){return 1;} int f(void) __attribute__((weak)); int f(void){return 2;}",
        [EXTERNAL_WEAK, None, EXTERNAL, None, MICROSOFT_EXTERN, None],
    ),
    (
        "__attribute__((weak)) int f(void); extern __inline__ int f(void){return 1;}",
        [
            INLINE_WEAK_REFERENCE,
            EXTERNAL_WEAK,
            WEAK_INLINE,
            EXTERNAL_WEAK,
            MICROSOFT_EXTERN_WEAK,
            MICROSOFT_EXTERN_WEAK,
        ],
    ),
    (
        "static __inline__ __attribute__((weak)) int f(void){return 1;}",
        [
            Some((Kind::Internal, Binding::Strong)),
            Some((Kind::Internal, Binding::Strong)),
            None,
            None,
            None,
            None,
        ],
    ),
    (
        "__inline__ __attribute__((always_inline)) int f(void){return 1;}",
        [EXTERNAL, INLINE, EXTERNAL, INLINE, MICROSOFT, MICROSOFT],
    ),
    (
        "extern __inline__ __attribute__((always_inline)) int f(void){return 1;}",
        [
            INLINE,
            EXTERNAL,
            INLINE,
            EXTERNAL,
            MICROSOFT_EXTERN,
            MICROSOFT_EXTERN,
        ],
    ),
];

fn outcome(table: &[Outcome; 6], profile: CompilerProfile) -> Outcome {
    let family = if profile.target() == Target::X86_64PcWindowsMsvc {
        4
    } else if profile.compiler() == Compiler::Clang {
        2
    } else {
        0
    };
    table[family + usize::from(!profile.language_mode().is_c90())]
}

#[test]
fn weak_inline_bodies_keep_binding_ownership_and_written_annotations_separate() {
    for (source, table) in CASES {
        for profile in CompilerProfile::ALL {
            for mode in LanguageMode::ALL {
                let profile = profile.with_language_mode(mode);
                let expected = outcome(table, profile);
                let plain = analyze_with_profile(source, profile, &AnalysisOptions::default());
                let retained = analyze_with_profile(
                    source,
                    profile,
                    &AnalysisOptions {
                        retain_code: true,
                        ..Default::default()
                    },
                );
                match (plain, retained, expected) {
                    (Ok(plain), Ok(retained), Some((kind, binding))) => {
                        assert_eq!(
                            format!("{:?}", plain.unit()),
                            format!("{:?}", retained.unit())
                        );
                        let declaration = retained
                            .unit()
                            .declarations
                            .iter()
                            .find(|d| d.name == "f")
                            .unwrap();
                        assert_eq!(
                            (
                                declaration.function_definition_kind,
                                declaration.symbol_binding
                            ),
                            (Some(kind), binding),
                            "{profile:?}: {source}"
                        );
                        let code = retained.checked().unwrap();
                        let (_, entity) = code
                            .entities()
                            .find(|(_, e)| e.name() == Some("f"))
                            .unwrap();
                        assert_eq!(entity.symbol_binding(), binding);
                        assert_eq!(
                            code.body(entity.body().unwrap()).unwrap().definition_kind(),
                            kind
                        );
                        assert_eq!(
                            code.declarations()
                                .filter(|(_, site)| site.weak_attribute().is_some())
                                .count(),
                            usize::from(source.contains("weak"))
                        );
                    }
                    (Err(plain), Err(retained), None) => assert_eq!(
                        (plain.offset, plain.message),
                        (retained.offset, retained.message)
                    ),
                    (plain, retained, expected) => panic!(
                        "{profile:?}: {source}: expected {expected:?}, got {:?}, {:?}",
                        plain.map(|_| "accepted"),
                        retained.map(|_| "accepted")
                    ),
                }
            }
        }
    }
}

#[test]
#[ignore = "requires GNU GCC and Clang cross-target symbol output"]
fn weak_and_forced_inline_ownership_matches_native_objects() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("weak-inline.c");
    let object = directory.path().join("weak-inline.o");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let clang = std::env::var("TOUCAN_CLANG").unwrap_or_else(|_| "clang".into());
    for (source, table) in CASES {
        for profile in CompilerProfile::ALL.into_iter().filter(|profile| {
            profile.compiler() == Compiler::Clang
                || cfg!(target_os = "linux")
                    && match profile.target() {
                        Target::X86_64UnknownLinuxGnu => cfg!(target_arch = "x86_64"),
                        Target::Aarch64UnknownLinuxGnu => cfg!(target_arch = "aarch64"),
                        _ => false,
                    }
        }) {
            for mode in LanguageMode::ALL {
                let profile = profile.with_language_mode(mode);
                for referenced in [false, true] {
                    let source = if referenced {
                        format!("{source}\n__typeof__(f) *address=f;\n")
                    } else {
                        (*source).to_owned()
                    };
                    std::fs::write(&input, &source).unwrap();
                    let mut command = Command::new(if profile.compiler() == Compiler::Gnu {
                        &gcc
                    } else {
                        &clang
                    });
                    command.args([format!("-std={mode}"), "-O0".into(), "-fno-inline".into()]);
                    if profile.compiler() == Compiler::Clang {
                        command.args([
                            format!("--target={}", profile.target()),
                            "-S".into(),
                            "-emit-llvm".into(),
                            "-o".into(),
                            "-".into(),
                        ]);
                    } else {
                        command.arg("-c").arg("-o").arg(&object);
                    }
                    let result = command.arg(&input).output().unwrap();
                    let expected = outcome(table, profile);
                    assert_eq!(
                        toucan_test_support::compiler_acceptance(&result),
                        Ok(expected.is_some()),
                        "{profile:?}: {source}: {}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    let Some((kind, binding)) = expected else {
                        continue;
                    };
                    let text = if profile.compiler() == Compiler::Clang {
                        String::from_utf8(result.stdout).unwrap()
                    } else {
                        String::from_utf8(Command::new("nm").arg(&object).output().unwrap().stdout)
                            .unwrap()
                    };
                    let symbol = text.lines().find(|line| {
                        (line.starts_with("define ") || line.starts_with("declare "))
                            && line.contains("@f(")
                            || line.split_whitespace().last() == Some("f")
                    });
                    let defined = symbol.is_some_and(|line| {
                        if profile.compiler() == Compiler::Clang {
                            line.starts_with("define ") && !line.contains(" available_externally ")
                        } else {
                            matches!(line.split_whitespace().rev().nth(1), Some("T" | "t" | "W"))
                        }
                    });
                    if referenced {
                        assert_eq!(
                            defined,
                            kind != Kind::InlineOnly,
                            "{profile:?}: {source}: {text}"
                        );
                    }
                    if matches!(kind, Kind::External | Kind::MicrosoftExternInline) {
                        assert!(defined, "{profile:?}: {source}: {text}");
                    }
                    if defined {
                        let symbol = symbol.unwrap();
                        let weak = if profile.compiler() == Compiler::Clang {
                            symbol.contains(" weak ")
                        } else {
                            symbol.split_whitespace().rev().nth(1) == Some("W")
                        };
                        assert_eq!(
                            weak,
                            binding == Binding::Weak,
                            "{profile:?}: {source}: {text}"
                        );
                    }
                }
            }
        }
    }
}

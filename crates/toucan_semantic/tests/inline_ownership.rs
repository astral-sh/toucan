use std::collections::BTreeMap;
use std::process::Command;

use serde::Deserialize;
use toucan_semantic::{Analysis, AnalysisOptions, Error, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, LanguageMode, Target};

#[derive(Deserialize)]
struct Case {
    name: String,
    source: String,
    has_body: bool,
    expected: BTreeMap<String, Option<String>>,
}

fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("fixtures/inline_ownership.json")).unwrap()
}

fn expected(case: &Case, profile: CompilerProfile) -> Option<&str> {
    let family = if profile.target().is_windows() {
        "msvc"
    } else if profile.compiler() == Compiler::Gnu {
        "gcc"
    } else {
        "clang"
    };
    let mode = if profile.language_mode().is_c90() {
        "gnu90"
    } else {
        "gnu11"
    };
    case.expected[&format!("{family}:{mode}")].as_deref()
}

fn parity(source: &str, profile: CompilerProfile) -> Result<Analysis, Error> {
    let ordinary = analyze_with_profile(source, profile, &AnalysisOptions::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&ordinary, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(
            format!("{:?}", a.unit()),
            format!("{:?}", b.unit()),
            "{source}"
        ),
        (Err(a), Err(b)) => assert_eq!((&a.message, a.offset), (&b.message, b.offset), "{source}"),
        _ => panic!("{profile:?}: {source}: ordinary {ordinary:?}, retained {retained:?}"),
    }
    retained
}

#[test]
fn inline_definition_ownership_follows_all_profiles_and_modes() {
    let mut failures = Vec::new();
    for case in cases() {
        for profile in CompilerProfile::ALL {
            for mode in LanguageMode::ALL {
                let profile = profile.with_language_mode(mode);
                let result = parity(&case.source, profile);
                let expected = expected(&case, profile);
                match (result, expected) {
                    (Ok(analysis), Some(expected)) => {
                        let declaration = analysis
                            .unit()
                            .declarations
                            .iter()
                            .find(|d| d.name == "f")
                            .unwrap();
                        assert_eq!(declaration.is_definition, case.has_body);
                        let actual = declaration
                            .function_definition_kind
                            .map(|kind| format!("{kind:?}"))
                            .unwrap_or_else(|| "Declaration".into());
                        if actual != expected {
                            failures.push(format!(
                                "{} {profile:?}: expected {expected}, got {actual}",
                                case.name
                            ));
                        }
                        let code = analysis.checked().unwrap();
                        let (_, entity) = code
                            .entities()
                            .find(|(_, e)| e.name() == Some("f"))
                            .unwrap();
                        if case.has_body {
                            let body = code.body(entity.body().unwrap()).unwrap();
                            assert_eq!(
                                Some(body.definition_kind()),
                                declaration.function_definition_kind,
                                "{} {profile:?}",
                                case.name
                            );
                        } else {
                            assert!(entity.body().is_none());
                        }
                    }
                    (Err(_), None) => {}
                    (result, expected) => failures.push(format!(
                        "{} {profile:?}: expected {expected:?}, got {:?}",
                        case.name,
                        result.map(|_| "accepted")
                    )),
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn inline_sites_keep_written_spans_and_every_body() {
    let source = String::from(
        "unsigned short text[] = u\"\\U0001F600\";\n__attribute__((__gnu_inline__)) extern __inline__ int f(void){return 1;}\nint f(void){return 2;}\nint g(void){extern int f(void);return f();}",
    );
    let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    let analysis = parity(&source, profile).unwrap();
    let code = analysis.checked().unwrap();
    let (entity_id, entity) = code
        .entities()
        .find(|(_, e)| e.name() == Some("f"))
        .unwrap();
    let bodies = code
        .bodies()
        .filter(|(_, body)| body.entity() == entity_id)
        .collect::<Vec<_>>();
    assert_eq!(bodies.len(), 2);
    assert_eq!(format!("{:?}", bodies[0].1.definition_kind()), "Superseded");
    assert_eq!(format!("{:?}", bodies[1].1.definition_kind()), "External");
    assert_eq!(entity.body(), Some(bodies[1].0));
    for (index, (id, body)) in bodies.iter().enumerate() {
        let statement = code.statement(body.statement()).unwrap();
        let span = code.occurrence(statement.occurrence()).unwrap().source();
        assert_eq!(&source[span.range()], ["{return 1;}", "{return 2;}"][index]);
        assert_eq!(
            code.declaration(body.declaration()).unwrap().body(),
            Some(*id)
        );
    }
    let sites = code.function_inline_sites().collect::<Vec<_>>();
    assert_eq!(sites.len(), 3);
    assert_eq!(
        &source[sites[0].inline_specifier().unwrap().range()],
        "__inline__"
    );
    assert_eq!(
        &source[sites[0].gnu_inline_attribute().unwrap().range()],
        "__gnu_inline__"
    );
    assert!(sites[0].written_extern());
    assert!(sites[1].inline_specifier().is_none());
    assert!(sites[1].gnu_inline_attribute().is_none());
    assert!(!sites[1].written_extern());
    assert!(sites[2].written_extern());
    drop(source);
    assert_eq!(code.function_inline_sites().count(), 3);
}

#[test]
fn ordinary_definitions_have_ownership_without_inline_history() {
    for profile in CompilerProfile::ALL {
        let analysis = parity(
            "int f(void){return 1;} static int g(void){return 2;} int declared(void); int object;",
            profile,
        )
        .unwrap();
        let names = analysis
            .unit()
            .declarations
            .iter()
            .map(|declaration| {
                (
                    declaration.name.as_str(),
                    declaration
                        .function_definition_kind
                        .map(|kind| format!("{kind:?}")),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                ("f", Some("External".into())),
                ("g", Some("Internal".into())),
                ("declared", None),
                ("object", None)
            ]
        );
        assert_eq!(
            analysis.checked().unwrap().function_inline_sites().count(),
            0
        );
    }
}

#[test]
#[ignore = "requires GNU GCC and Clang cross-target LLVM emission; run with --include-ignored"]
fn inline_ownership_matches_native_symbols() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("probe.c");
    let output = directory.path().join("probe.o");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let clang = std::env::var("TOUCAN_CLANG").unwrap_or_else(|_| "clang".into());
    let native_gnu = if cfg!(target_os = "linux") {
        Some(
            match (cfg!(target_arch = "aarch64"), cfg!(target_env = "musl")) {
                (false, false) => Target::X86_64UnknownLinuxGnu,
                (true, false) => Target::Aarch64UnknownLinuxGnu,
                (false, true) => Target::X86_64UnknownLinuxMusl,
                (true, true) => Target::Aarch64UnknownLinuxMusl,
            },
        )
    } else {
        None
    };
    if native_gnu.is_some() {
        assert!(
            !String::from_utf8_lossy(&Command::new(&gcc).arg("--version").output().unwrap().stdout)
                .to_lowercase()
                .contains("clang")
        );
    }
    for case in cases() {
        let source = format!("{}\n__typeof__(f) *address=f;\n", case.source);
        std::fs::write(&input, source).unwrap();
        for profile in CompilerProfile::ALL.into_iter().filter(|profile| {
            profile.compiler() == Compiler::Clang || Some(profile.target()) == native_gnu
        }) {
            for mode in LanguageMode::ALL {
                let profile = profile.with_language_mode(mode);
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
                    command.arg("-c").arg("-o").arg(&output);
                }
                let result = command.arg(&input).output().unwrap();
                let expected = expected(&case, profile);
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&result),
                    Ok(expected.is_some()),
                    "{} {profile:?}: {}",
                    case.name,
                    String::from_utf8_lossy(&result.stderr)
                );
                let Some(expected) = expected else { continue };
                let text = if profile.compiler() == Compiler::Clang {
                    String::from_utf8(result.stdout).unwrap()
                } else {
                    String::from_utf8(Command::new("nm").arg(&output).output().unwrap().stdout)
                        .unwrap()
                };
                let symbol = text
                    .lines()
                    .find(|line| {
                        (line.starts_with("define ") || line.starts_with("declare "))
                            && line.contains("@f(")
                            || line.split_whitespace().last() == Some("f")
                    })
                    .unwrap_or_else(|| panic!("{} {profile:?}: {text}", case.name));
                let actual = if !case.has_body {
                    "Declaration"
                } else if profile.compiler() == Compiler::Clang {
                    if symbol.starts_with("declare ") {
                        "InlineOnly"
                    } else if symbol.contains(" internal ") {
                        "Internal"
                    } else if symbol.contains(" weak_odr ") {
                        "MicrosoftExternInline"
                    } else if symbol.contains(" linkonce_odr ") {
                        "MicrosoftInline"
                    } else {
                        "External"
                    }
                } else {
                    match symbol.split_whitespace().rev().nth(1).unwrap() {
                        "T" => "External",
                        "t" => "Internal",
                        "U" => "InlineOnly",
                        other => panic!("unexpected symbol {other}: {symbol}"),
                    }
                };
                assert_eq!(actual, expected, "{} {profile:?}: {symbol}", case.name);
            }
        }
    }
}

#[test]
fn inline_metadata_fits_existing_body_and_declaration_padding() {
    use std::mem::size_of;
    use toucan_semantic::{
        Declaration, Type,
        checked::{DeclarationSite, Entity, FunctionBody},
    };
    assert_eq!(size_of::<Type>(), 40);
    assert_eq!(size_of::<Declaration>(), 136);
    assert_eq!(size_of::<DeclarationSite>(), 256);
    assert_eq!(size_of::<Entity>(), 88);
    assert_eq!(size_of::<FunctionBody>(), 56);
}

#[test]
#[ignore = "requires native GCC/Clang linking and Clang Windows LLVM emission"]
fn inline_ownership_controls_unused_emission_and_native_linking() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("inline.c");
    let implementation = directory.path().join("implementation.c");
    let executable = directory.path().join("probe");
    let clang = std::env::var("TOUCAN_CLANG").unwrap_or_else(|_| "clang".into());
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for mode in LanguageMode::ALL {
        for (source, emitted) in [
            ("__inline__ int f(void){return 1;}", false),
            ("extern __inline__ int f(void){return 1;}", true),
            (
                "__inline__ int f(void){return 1;} extern int f(void);",
                true,
            ),
            ("int f(void){return 1;} __inline__ int f(void);", true),
        ] {
            std::fs::write(&input, source).unwrap();
            let result = Command::new(&clang)
                .args([
                    format!("-std={mode}"),
                    "--target=x86_64-pc-windows-msvc".into(),
                    "-O0".into(),
                    "-fno-inline".into(),
                    "-S".into(),
                    "-emit-llvm".into(),
                    "-o".into(),
                    "-".into(),
                ])
                .arg(&input)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let llvm = String::from_utf8(result.stdout).unwrap();
            assert_eq!(
                llvm.lines()
                    .any(|line| line.starts_with("define ") && line.contains("@f(")),
                emitted,
                "{mode}: {source}: {llvm}"
            );
        }
        for compiler in [&clang]
            .into_iter()
            .chain(cfg!(target_os = "linux").then_some(&gcc))
        {
            let specifier = if mode.is_c90() {
                "extern __inline__"
            } else {
                "__inline__"
            };
            let source =
                format!("{specifier} int f(void){{return 1;}} int (*get(void))(void){{return f;}}");
            std::fs::write(&input, &source).unwrap();
            std::fs::write(&implementation, "int f(void){return 37;} int (*get(void))(void); int main(void){return get()()!=37;}\n").unwrap();
            let profile = CompilerProfile::new(
                Target::X86_64UnknownLinuxGnu,
                if compiler == &gcc {
                    Compiler::Gnu
                } else {
                    Compiler::Clang
                },
            )
            .unwrap()
            .with_language_mode(mode);
            let analysis = parity(&source, profile).unwrap();
            assert_eq!(
                analysis.unit().declarations[0].function_definition_kind,
                Some(toucan_semantic::FunctionDefinitionKind::InlineOnly)
            );
            let result = Command::new(compiler)
                .args([
                    format!("-std={mode}"),
                    "-O0".into(),
                    "-fno-inline".into(),
                    "-fsanitize=undefined".into(),
                    "-fno-sanitize-recover=undefined".into(),
                ])
                .arg(&input)
                .arg(&implementation)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{compiler} {mode}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            let result = Command::new(&executable).output().unwrap();
            assert!(
                result.status.success(),
                "{compiler} {mode}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

use std::path::Path;
use std::process::Command;

use toucan::{AnalysisOptions, Compiler, CompilerProfile, Config, Target};

const KEYWORDS: &[&str] = &[
    "__cdecl",
    "__stdcall",
    "__fastcall",
    "__thiscall",
    "__pascal",
];
const CASES: &[(&str, bool)] = &[
    ("int CC f(int);", true),
    ("CC int f(int);", true),
    ("int (CC *f)(int);", true),
    ("int (*CC f)(int);", true),
    ("int (CC **f)(int);", true),
    ("int *CC f(int);", true),
    ("int CC f(int x){return x;}", true),
    ("struct S {int (CC *callback)(int);};", true),
    (
        "typedef int (CC *F)(int); F choose(F value){return value;}",
        true,
    ),
    ("int f(int (CC *callback)(int)){return callback(4);}", true),
    ("int f(void){return sizeof(int (CC *)(int));}", true),
    ("int f(void){return sizeof(int (*CC)(int));}", true),
    ("int CC object;", true),
    ("int CC CC f(int);", true),
    ("int f(int) CC;", false),
    ("CC", false),
    ("int f(int CC x){return x;}", true),
    ("int CC (*f(void))(int);", true),
];

#[test]
fn calling_keywords_preserve_declarator_boundaries_and_profile_reservation() {
    for profile in CompilerProfile::ALL {
        for keyword in KEYWORDS {
            for &(template, valid) in CASES {
                let source = template.replace("CC", keyword);
                let ordinary =
                    toucan::semantic::analyze_with_profile(&source, profile, &Default::default());
                let retained = toucan::semantic::analyze_with_profile(
                    &source,
                    profile,
                    &AnalysisOptions {
                        retain_code: true,
                        ..Default::default()
                    },
                );
                assert_eq!(
                    ordinary.is_ok(),
                    valid && profile.compiler() == Compiler::Clang,
                    "{profile:?}: {source}: {ordinary:?}"
                );
                match (ordinary, retained) {
                    (Ok(a), Ok(b)) => assert_eq!(
                        serde_json::to_value(a.unit()).unwrap(),
                        serde_json::to_value(b.unit()).unwrap()
                    ),
                    (Err(a), Err(b)) => assert_eq!((a.offset, a.message), (b.offset, b.message)),
                    result => panic!("retention changed result: {result:?}"),
                }
            }
        }
        if profile.compiler() == Compiler::Gnu {
            toucan::semantic::analyze_with_profile(
                "int __cdecl; typedef int __stdcall; __stdcall f(__stdcall x){return x+__cdecl;}",
                profile,
                &Default::default(),
            )
            .unwrap();
        }
    }
}

#[test]
fn microsoft_single_underscore_aliases_follow_the_extension_mode() {
    for keyword in ["_cdecl", "_stdcall", "_fastcall", "_thiscall"] {
        for profile in CompilerProfile::ALL {
            let source = format!("int {keyword} f(int); typedef int ({keyword} *F)(int);");
            let result =
                toucan::semantic::analyze_with_profile(&source, profile, &Default::default());
            assert_eq!(
                result.is_ok(),
                profile.target() == Target::X86_64PcWindowsMsvc,
                "{source}: {result:?}"
            );
        }
    }
}

#[test]
fn default_calling_keywords_generate_the_canonical_rust_api() {
    let source = "typedef int (CC *Callback)(int); int CC call(Callback, int);";
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let compile = |source: &str| {
            toucan::parse_source(Path::new("api.h"), source, &Config::with_profile(profile))
                .unwrap()
                .bindings(&Default::default())
                .unwrap()
                .0
        };
        let canonical = compile(&source.replace("CC", ""));
        for keyword in KEYWORDS {
            assert_eq!(compile(&source.replace("CC", keyword)), canonical);
        }
    }
}

const ALIAS: &str = r#"
typedef int __attribute__((ms_abi)) M(int);
typedef int C(int);
M *__cdecl pointer;
M *__cdecl f(int);
_Static_assert(__builtin_types_compatible_p(__typeof__(pointer), C *), "pointer override");
_Static_assert(__builtin_types_compatible_p(__typeof__(f(0)), C *), "returned callback override");
int (__attribute__((ms_abi)) *returns_microsoft(int))(int);
_Static_assert(__builtin_types_compatible_p(__typeof__(returns_microsoft(0)), M *), "leading attribute selects callback");
"#;

#[test]
fn pointer_keywords_select_the_first_function_boundary() {
    let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    for retained in [false, true] {
        toucan::semantic::analyze_with_profile(
            ALIAS,
            profile,
            &AnalysisOptions {
                retain_code: retained,
                ..Default::default()
            },
        )
        .unwrap();
        for source in [
            "typedef int __attribute__((ms_abi)) M(int); M __cdecl *pointer;",
            "typedef int __attribute__((ms_abi)) M(int); M (__cdecl *pointer);",
        ] {
            assert!(
                toucan::semantic::analyze_with_profile(
                    source,
                    profile,
                    &AnalysisOptions {
                        retain_code: retained,
                        ..Default::default()
                    }
                )
                .is_err()
            );
        }
    }
}

#[test]
fn nondefault_x86_calling_keywords_have_explicit_diagnostics() {
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        for keyword in ["__vectorcall", "__regcall"] {
            let source = format!("float {keyword} f(float x){{return x;}}");
            let result =
                toucan::semantic::analyze_with_profile(&source, profile, &Default::default());
            if profile.target().is_aarch64() {
                result.unwrap();
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .message
                        .contains("unsupported calling-convention keyword")
                );
            }
        }
    }
}

#[test]
#[ignore = "requires Clang's five target frontends"]
fn calling_keyword_placement_matches_clang() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("calling.c");
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        for keyword in KEYWORDS {
            for &(template, _) in CASES {
                let source = template.replace("CC", keyword);
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
                let result =
                    toucan::semantic::analyze_with_profile(&source, profile, &Default::default());
                assert_eq!(
                    result.is_ok(),
                    accepted,
                    "{profile:?}: {source}: {result:?}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
    std::fs::write(&input, ALIAS).unwrap();
    let output = Command::new("clang")
        .args([
            "-target",
            "x86_64-unknown-linux-gnu",
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

#[test]
fn pointer_level_conventions_override_the_function_type_at_that_boundary() {
    for target in [Target::X86_64UnknownLinuxGnu, Target::X86_64PcWindowsMsvc] {
        let profile = CompilerProfile::new(target, Compiler::Clang).unwrap();
        for first in [
            "__cdecl",
            "__attribute__((ms_abi))",
            "__attribute__((sysv_abi))",
        ] {
            for second in [
                "__cdecl",
                "__attribute__((ms_abi))",
                "__attribute__((sysv_abi))",
            ] {
                for template in [
                    "int (FIRST *SECOND pointer)(int);",
                    "typedef int FIRST M(int); M *SECOND pointer;",
                    "typedef int (FIRST *M)(int); M SECOND pointer;",
                ] {
                    let source = format!(
                        "{} typedef int {second} Expected(int); _Static_assert(__builtin_types_compatible_p(__typeof__(pointer), Expected*),\"pointer convention\");",
                        template.replace("FIRST", first).replace("SECOND", second)
                    );
                    for retain_code in [false, true] {
                        toucan::semantic::analyze_with_profile(
                            &source,
                            profile,
                            &AnalysisOptions {
                                retain_code,
                                ..Default::default()
                            },
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}

#[test]
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
#[ignore = "requires native x86_64 Linux Clang and rustc; supports TOUCAN_TEST_RUST_TOOLCHAIN"]
fn generated_calling_keyword_bindings_execute_both_callback_abis() {
    let directory = tempfile::tempdir().unwrap();
    let header = r#"
typedef int __attribute__((ms_abi)) Microsoft(int);
Microsoft *__cdecl plain_factory(int);
int (__attribute__((ms_abi)) *microsoft_factory(int))(int);
int __cdecl invoke(int (*__cdecl callback)(int), int value);
int invoke_microsoft(Microsoft callback, int value);
"#;
    std::fs::write(directory.path().join("api.h"), header).unwrap();
    std::fs::write(
        directory.path().join("api.c"),
        r#"
#include "api.h"
static int plain(int value) { return value * 3 + 1; }
static int __attribute__((ms_abi)) microsoft(int value) { return value * 7 + 2; }
Microsoft *__cdecl plain_factory(int unused) { return plain; }
int (__attribute__((ms_abi)) *microsoft_factory(int unused))(int) { return microsoft; }
int __cdecl invoke(int (*__cdecl callback)(int), int value) { return callback(value) + 5; }
int invoke_microsoft(Microsoft callback, int value) { return callback(value) + 9; }
"#,
    )
    .unwrap();
    let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    let bindings = toucan::parse_source(Path::new("api.h"), header, &Config::with_profile(profile))
        .unwrap()
        .bindings(&toucan::BindingOptions {
            rust_target: toucan::RustTarget::RUST_1_64,
            ..Default::default()
        })
        .unwrap()
        .0;
    std::fs::write(directory.path().join("bindings.rs"), bindings).unwrap();
    std::fs::write(
        directory.path().join("main.rs"),
        r#"
#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]
include!("bindings.rs");
unsafe extern "C" fn plain_callback(value: i32) -> i32 { value * 13 }
unsafe extern "win64" fn microsoft_callback(value: i32) -> i32 { value * 17 }
fn main() {
    for value in 0..1000 {
        unsafe {
            assert_eq!(plain_factory(0).unwrap()(value), value * 3 + 1);
            assert_eq!(microsoft_factory(0).unwrap()(value), value * 7 + 2);
            assert_eq!(invoke(Some(plain_callback), value), value * 13 + 5);
            assert_eq!(invoke_microsoft(Some(microsoft_callback), value), value * 17 + 9);
        }
    }
}
"#,
    )
    .unwrap();
    for c_optimization in ["-O0", "-O2"] {
        let output = Command::new("clang")
            .current_dir(directory.path())
            .args(["-std=gnu11", c_optimization, "-c", "api.c", "-o", "api.o"])
            .output()
            .unwrap();
        assert!(
            toucan_test_support::compiler_acceptance(&output).unwrap(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for rust_optimization in ["0", "3"] {
            let mut command = Command::new("rustc");
            if let Ok(toolchain) = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
                command.arg(format!("+{toolchain}"));
            }
            let output = command
                .current_dir(directory.path())
                .args([
                    "--edition=2021",
                    "main.rs",
                    "-C",
                    "link-arg=api.o",
                    "-C",
                    &format!("opt-level={rust_optimization}"),
                    "-o",
                    "consumer",
                ])
                .output()
                .unwrap();
            assert!(
                toucan_test_support::compiler_acceptance(&output).unwrap(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(directory.path().join("consumer"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{c_optimization}/Rust{rust_optimization}: {output:?}"
            );
        }
    }
}

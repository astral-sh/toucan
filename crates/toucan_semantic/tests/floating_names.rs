use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, LanguageMode, Target};

const NAMES: [&str; 7] = [
    "_Float16",
    "_Float32",
    "_Float64",
    "_Float32x",
    "_Float64x",
    "_Float128",
    "__float128",
];

fn cases(profile: CompilerProfile, name: &str) -> Vec<(String, bool)> {
    let keyword = match profile.compiler() {
        Compiler::Gnu => name != "__float128",
        Compiler::Clang => matches!(name, "_Float16" | "__float128"),
    };
    let predefined = profile.compiler() == Compiler::Gnu
        && (profile.target().is_x86_64() || profile.target() == Target::I686UnknownLinuxGnu)
        && name == "__float128";
    [
        ("typedef int NAME;NAME value;", !keyword),
        ("void f(int NAME){NAME=1;}", !keyword),
        ("int NAME=1;", !keyword && !predefined),
        ("void f(void){extern int NAME;}", !keyword && !predefined),
        ("typedef NAME NAME;", predefined),
        ("enum E{NAME=1};int f(void){return NAME;}", !keyword),
        ("struct S{int NAME;};", !keyword),
    ]
    .into_iter()
    .map(|(source, expected)| (source.replace("NAME", name), expected))
    .collect()
}

fn compare(source: &str, profile: CompilerProfile, expected: bool) {
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
    match (ordinary, retained) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => assert_eq!((a.offset, a.message), (b.offset, b.message)),
        (a, b) => panic!("{profile:?}: {source}: {a:?} {b:?}"),
    }
}

#[test]
fn floating_keywords_and_predefined_typedef_names_keep_distinct_namespaces() {
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            for name in NAMES {
                for (source, expected) in cases(profile, name) {
                    compare(&source, profile, expected);
                }
                let ordinary = profile.compiler() == Compiler::Clang
                    && !matches!(name, "_Float16" | "__float128")
                    || profile.compiler() == Compiler::Gnu && name == "__float128";
                for source in [
                    "typedef int NAME;typedef int NAME;NAME value;",
                    "int f(void){static int NAME;return NAME;}",
                    "typedef int NAME;void f(void){typedef char NAME;NAME value;_Static_assert(sizeof(value)==1,\"local\");}NAME value;_Static_assert(sizeof(value)==sizeof(int),\"outer\");",
                    "struct NAME{int field;};int f(void){goto NAME;NAME:return 1;}",
                ] {
                    compare(&source.replace("NAME", name), profile, ordinary);
                }
                compare(
                    &format!("typedef int {name};typedef double {name};"),
                    profile,
                    false,
                );
            }
        }
    }
}

#[test]
#[ignore = "requires native GNU Linux compiler and Clang cross-target syntax checks"]
fn floating_name_constraints_match_native_compilers() {
    use std::process::Command;
    let mut compilers = Target::ALL
        .into_iter()
        .map(|target| ("clang".to_owned(), Compiler::Clang, target, true))
        .collect::<Vec<_>>();
    if cfg!(target_os = "linux") {
        let target = if cfg!(target_arch = "aarch64") {
            Target::Aarch64UnknownLinuxGnu
        } else {
            Target::X86_64UnknownLinuxGnu
        };
        compilers.push((
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            Compiler::Gnu,
            target,
            false,
        ));
    }
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("names.c");
    for (compiler, family, target, cross) in compilers {
        if family == Compiler::Gnu {
            let identity = Command::new(&compiler).arg("--version").output().unwrap();
            assert!(identity.status.success());
            assert!(
                !String::from_utf8_lossy(&identity.stdout)
                    .to_ascii_lowercase()
                    .contains("clang")
            );
        }
        for mode in [LanguageMode::Gnu90, LanguageMode::Gnu17] {
            let profile = CompilerProfile::new(target, family)
                .unwrap()
                .with_language_mode(mode);
            for name in NAMES {
                for (source, expected) in cases(profile, name) {
                    std::fs::write(&input, &source).unwrap();
                    let mut command = Command::new(&compiler);
                    command
                        .arg(format!("-std={mode}"))
                        .args(["-ffreestanding", "-fsyntax-only"]);
                    if cross {
                        command.arg(format!("--target={target}"));
                    }
                    let output = command.arg(&input).output().unwrap();
                    assert_eq!(
                        toucan_test_support::compiler_acceptance(&output),
                        Ok(expected),
                        "{compiler} {profile:?}: {source}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    compare(&source, profile, expected);
                }
            }
        }
    }
}

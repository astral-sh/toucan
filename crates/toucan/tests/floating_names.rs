use std::path::Path;

use toucan::{Compiler, CompilerProfile, Config, LanguageMode};

const HEADER: &str = "#if __GNUC__ < 7\ntypedef float _Float32;\n#endif\n_Float32 value;\n";

#[test]
fn version_overrides_select_headers_without_changing_floating_keywords() {
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            for version in [None, Some("4"), Some("13")] {
                let mut config = Config::with_profile(profile);
                if let Some(version) = version {
                    config
                        .preprocessor
                        .defines
                        .insert("__GNUC__".into(), version.into());
                }
                let expected = match version {
                    None => true,
                    Some("4") => profile.compiler() == Compiler::Clang,
                    _ => profile.compiler() == Compiler::Gnu,
                };
                for retain in [false, true] {
                    config.analysis.retain_code = retain;
                    let result = toucan::parse_source(Path::new("float-name.h"), HEADER, &config);
                    assert_eq!(
                        result.is_ok(),
                        expected,
                        "{profile:?} {version:?}: {:?}",
                        result.as_ref().err()
                    );
                }
            }
            let config = Config::with_profile(profile);
            let source = "#define _Float32 float\n_Float32 value;_Static_assert(__builtin_types_compatible_p(__typeof__(value),float),\"explicit macro\");\n";
            toucan::parse_source(Path::new("macro.h"), source, &config).unwrap();
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn overridden_version_float_branches_match_native_preprocessing() {
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("header.c");
    std::fs::write(&input, HEADER).unwrap();
    for compiler in ["gcc", "clang"] {
        let identity = Command::new(compiler).arg("--version").output().unwrap();
        assert!(identity.status.success());
        if compiler == "gcc" {
            assert!(
                !String::from_utf8_lossy(&identity.stdout)
                    .to_ascii_lowercase()
                    .contains("clang")
            );
        }
        let family = if compiler == "gcc" {
            Compiler::Gnu
        } else {
            Compiler::Clang
        };
        let profile = CompilerProfile::new(toucan::Target::X86_64UnknownLinuxGnu, family).unwrap();
        for mode in LanguageMode::ALL {
            for version in ["4", "13"] {
                let mut config = Config::with_profile(profile.with_language_mode(mode));
                config
                    .preprocessor
                    .defines
                    .insert("__GNUC__".into(), version.into());
                let output = Command::new(compiler)
                    .arg(format!("-std={mode}"))
                    .args([
                        "-U__GNUC__",
                        &format!("-D__GNUC__={version}"),
                        "-fsyntax-only",
                    ])
                    .arg(&input)
                    .output()
                    .unwrap();
                let accepted = toucan_test_support::compiler_acceptance(&output).unwrap();
                assert_eq!(
                    accepted,
                    (family == Compiler::Gnu) == (version == "13"),
                    "{compiler} {mode} {version}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(
                    toucan::parse_source(&input, HEADER, &config).is_ok(),
                    accepted
                );
            }
        }
    }
}

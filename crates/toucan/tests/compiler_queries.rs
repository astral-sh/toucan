use std::path::Path;

use toucan::{Compiler, CompilerProfile, Config, FeatureQuery, LanguageMode, Target};

const FAMILIES: &[(&str, &str)] = &[
    ("c_alignas", "_Alignas(16) int aligned_object;"),
    (
        "c_alignof",
        "_Static_assert(_Alignof(int) > 0, \"alignment\");",
    ),
    ("c_atomic", "_Atomic(int) atomic_object;"),
    (
        "c_generic_selections",
        "_Static_assert(_Generic(1, int: 1, default: 0), \"selection\");",
    ),
    ("c_static_assert", "_Static_assert(1, \"assertion\");"),
    ("c_thread_local", "extern _Thread_local int thread_object;"),
];

#[test]
fn language_queries_select_checked_features_in_each_mode() {
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            let mut config = Config::with_profile(profile);
            for &(family, declaration) in FAMILIES {
                let source = if profile.compiler() == Compiler::Clang {
                    format!(
                        "#if !__has_extension({family}) || !__has_extension(__{family}__)\n#error missing extension\n#endif\n_Static_assert(__has_feature({family}) == {}, \"standard\");\n{declaration}\n",
                        u8::from(mode.is_c11())
                    )
                } else {
                    format!(
                        "#if defined(__has_feature) || defined(__has_extension) || defined(__has_declspec_attribute) || defined(__building_module)\n#error Clang query defined under GNU13\n#endif\n{declaration}\n"
                    )
                };
                config.analysis.retain_code = false;
                let ordinary = toucan::parse_source(Path::new("features.h"), &source, &config)
                    .unwrap_or_else(|error| panic!("{profile:?} {family}: {error}"));
                config.analysis.retain_code = true;
                let retained = toucan::parse_source(Path::new("features.h"), &source, &config)
                    .unwrap_or_else(|error| panic!("{profile:?} {family}: {error}"));
                assert_eq!(
                    format!("{:?}", ordinary.unit()),
                    format!("{:?}", retained.unit())
                );
                assert!(ordinary.preprocessed().is_defined("__has_c_attribute"));
                assert!(
                    !ordinary
                        .preprocessed()
                        .macros
                        .contains_key("__has_c_attribute")
                );
            }
        }
    }
}

#[test]
fn source_queries_preserve_catalog_boundaries_and_caller_overrides() {
    for profile in CompilerProfile::ALL
        .into_iter()
        .flat_map(|p| LanguageMode::ALL.map(|m| p.with_language_mode(m)))
    {
        let mut config = Config::with_profile(profile);
        config
            .preprocessor
            .defines
            .insert("__clang__".into(), "99".into());
        config
            .preprocessor
            .defines
            .insert("__GNUC__".into(), "99".into());
        let queries = config.preprocessor.feature_queries.as_ref().unwrap();
        assert_eq!(
            queries.is_enabled(FeatureQuery::Feature),
            profile.compiler() == Compiler::Clang
        );
        assert_eq!(
            config.preprocessor.scope_punctuator,
            profile.compiler() == Compiler::Clang || profile.language_mode().is_gnu()
        );
        let source = if profile.compiler() == Compiler::Clang {
            format!(
                "_Static_assert(__has_declspec_attribute(__align__) == {}, \"declspec alias\");\n_Static_assert(!__has_feature(cxx_exceptions) && !__has_extension(toucan_missing) && !__building_module(_Builtin_stddef), \"unavailable\");\n_Static_assert(!__has_c_attribute(fallthrough), \"C23 syntax\");\n",
                u8::from(profile.target() == Target::X86_64PcWindowsMsvc)
            )
        } else {
            "_Static_assert(!__has_c_attribute(fallthrough), \"C23 syntax\");\n".into()
        };
        toucan::parse_source(Path::new("queries.h"), &source, &config).unwrap();
        config.preprocessor.undefine("__has_c_attribute");
        config
            .preprocessor
            .defines
            .insert("__has_c_attribute(x)".into(), "7".into());
        toucan::parse_source(
            Path::new("override.h"),
            "_Static_assert(__has_c_attribute(anything)==7, \"override\");",
            &config,
        )
        .unwrap();
    }
}

#[test]
fn positive_feature_queries_still_require_valid_source_operands() {
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        for source in [
            "#if __has_extension(c_alignas)\n_Alignas(3) int x;\n#endif\n",
            "#if __has_extension(c_atomic)\n_Atomic(void) x;\n#endif\n",
            "#if __has_extension(c_generic_selections)\nint x=_Generic(1,int:1,int:2);\n#endif\n",
            "#if __has_declspec_attribute(align)\n__declspec(align(3)) int x;\n#else\n#error unsupported profile\n#endif\n",
        ] {
            assert!(
                toucan::parse_source(
                    Path::new("invalid.h"),
                    source,
                    &Config::with_profile(profile)
                )
                .is_err(),
                "{profile:?}: {source}"
            );
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang; checks feature-selected C source on each Clang target"]
fn advertised_language_features_match_native_source_acceptance() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("features.c");
    let clang = std::env::var("TOUCAN_CLANG").unwrap_or_else(|_| "clang".into());
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let mut routes = Target::ALL
        .map(|target| {
            (
                clang.as_str(),
                CompilerProfile::new(target, Compiler::Clang).unwrap(),
                true,
            )
        })
        .to_vec();
    let native = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some(Target::X86_64UnknownLinuxGnu),
        ("linux", "aarch64") => Some(Target::Aarch64UnknownLinuxGnu),
        _ => None,
    };
    if let Some(target) = native {
        routes.push((
            gcc.as_str(),
            CompilerProfile::new(target, Compiler::Gnu).unwrap(),
            false,
        ));
    }
    for (compiler, profile, cross) in routes {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            let mut source = String::new();
            for &(family, declaration) in FAMILIES {
                if profile.compiler() == Compiler::Clang {
                    source.push_str(&format!(
                        "_Static_assert(__has_extension({family}) && __has_extension(__{family}__), \"extension\");\n_Static_assert(__has_feature({family}) == {}, \"standard\");\n",
                        u8::from(mode.is_c11())
                    ));
                }
                source.push_str(declaration);
                source.push('\n');
            }
            if profile.compiler() == Compiler::Clang {
                source.push_str(&format!(
                    "_Static_assert(__has_declspec_attribute(__align__) == {}, \"declspec\");\n",
                    u8::from(profile.target() == Target::X86_64PcWindowsMsvc)
                ));
            }
            std::fs::write(&path, &source).unwrap();
            let mut command = Command::new(compiler);
            command.arg(format!("-std={mode}")).arg("-fsyntax-only");
            if cross {
                command.arg(format!("--target={}", profile.target()));
            }
            if matches!(
                profile.target(),
                Target::X86_64AppleDarwin | Target::Aarch64AppleDarwin
            ) {
                // Match the existing TLS oracle's deployment baseline; an
                // unversioned x86 Darwin triple otherwise selects pre-TLS macOS.
                command.arg("-mmacosx-version-min=11.0");
            }
            let result = command.arg(&path).output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&result),
                Ok(true),
                "{profile:?}: {source}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            toucan::parse_source(&path, &source, &Config::with_profile(profile)).unwrap();
        }
    }
}

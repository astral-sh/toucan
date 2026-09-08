use std::path::Path;
use toucan::{Compiler, CompilerProfile, Config, LanguageMode, Target};

#[test]
fn profile_defaults_and_caller_overrides_select_the_header_api() {
    let source = "#ifdef __STRICT_ANSI__\nint standard;\n#else\nint extension;\n#endif\n#ifdef unix\nint bare_unix;\n#endif\n";
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            let mut config = Config::with_profile(profile);
            let parsed = toucan::parse_source(Path::new("mode.h"), source, &config).unwrap();
            let names = parsed
                .unit()
                .declarations
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>();
            let strict = !mode.is_gnu() && profile.target() != Target::X86_64PcWindowsMsvc;
            assert_eq!(names.contains(&"standard"), strict);
            assert_eq!(names.contains(&"extension"), !strict);
            assert_eq!(
                names.contains(&"bare_unix"),
                mode.is_gnu()
                    && matches!(
                        profile.target(),
                        Target::X86_64UnknownLinuxGnu
                            | Target::X86_64UnknownLinuxMusl
                            | Target::Aarch64UnknownLinuxGnu
                            | Target::Aarch64UnknownLinuxMusl
                    )
            );
            config
                .preprocessor
                .defines
                .insert("__STRICT_ANSI__".into(), "7".into());
            let parsed = toucan::parse_source(
                Path::new("mode.h"),
                "_Static_assert(__STRICT_ANSI__==7,\"override\");",
                &config,
            )
            .unwrap();
            assert_eq!(parsed.unit().language_mode, mode);
        }
    }
}

#[test]
fn physical_and_command_line_trigraphs_use_distinct_profile_rules() {
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            let mut config = Config::with_profile(profile);
            config.preprocessor.defines.clear();
            config.preprocessor.forced_includes.clear();
            let source = "const char *physical=\"??=\";const char *command=VALUE;";
            config
                .preprocessor
                .defines
                .insert("VALUE".into(), "\"??=\"".into());
            for enabled in [false, true] {
                config.preprocessor.trigraphs = enabled;
                let parsed = toucan::parse_source(Path::new("phases.h"), source, &config).unwrap();
                let pp = &parsed.preprocessed().source;
                assert_eq!(
                    pp.matches("\"#\"").count(),
                    usize::from(enabled) * (1 + usize::from(profile.compiler() == Compiler::Clang)),
                    "{profile:?}: {pp}"
                );
                let index = pp.find("physical").unwrap();
                let origin = parsed.preprocessed().resolve_location(index).unwrap();
                assert_eq!(origin.line, 1);
                assert_eq!(origin.column, 13);
            }
        }
    }
}

#[test]
fn command_line_definitions_stop_before_another_physical_line() {
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, compiler).unwrap();
        let mut config = Config::with_profile(profile);
        for body in [
            "3\n#error injected",
            "3\r#error injected",
            "3/* comment */+4",
            "3// comment",
        ] {
            config
                .preprocessor
                .defines
                .insert("VALUE".into(), body.into());
            let parsed =
                toucan::parse_source(Path::new("definition.h"), "enum {value=VALUE};", &config)
                    .unwrap();
            let value = toucan::semantic::evaluate_integer(parsed.unit(), "value").unwrap();
            assert_eq!(value.value, if body.contains("+4") { 7 } else { 3 });
            let offset = parsed.preprocessed().source.find('3').unwrap();
            let origin = parsed.preprocessed().resolve_location(offset).unwrap();
            assert_eq!((origin.line, origin.column), (1, 13));
        }
        config
            .preprocessor
            .defines
            .insert("VALUE".into(), "3\\\n+4".into());
        assert!(
            toucan::parse_source(Path::new("definition.h"), "enum {value=VALUE};", &config)
                .is_err()
        );
    }
}

#[test]
#[ignore = "requires GCC and Clang"]
fn command_line_and_physical_trigraphs_match_native_preprocessing() {
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("phases.c");
    let source = "#ifdef __STRICT_ANSI__\nstrict\n#endif\n#ifdef unix\nbare_unix\n#endif\nphysical \"??=\"\ncommand VALUE\n";
    std::fs::write(&input, source).unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let native_target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Some(Target::X86_64UnknownLinuxGnu),
        ("aarch64", "linux") => Some(Target::Aarch64UnknownLinuxGnu),
        _ => None,
    };
    // GNU profiles are supported on Linux; Darwin and Windows use Clang below.
    let native_gnu = native_target.map_or_else(Vec::new, |target| {
        let version = Command::new(&gcc).arg("--version").output().unwrap();
        assert!(version.status.success(), "{gcc}: {version:?}");
        assert!(
            !String::from_utf8_lossy(&version.stdout)
                .to_ascii_lowercase()
                .contains("clang"),
            "the GNU oracle requires GCC; set TOUCAN_GCC to its executable"
        );
        vec![(target, false, true)]
    });
    for (command, targets) in [
        (gcc.as_str(), native_gnu),
        (
            "clang",
            Target::ALL.map(|target| (target, true, false)).to_vec(),
        ),
    ] {
        for (target, cross, gnu) in targets {
            for mode in LanguageMode::ALL {
                let profile =
                    CompilerProfile::new(target, if gnu { Compiler::Gnu } else { Compiler::Clang })
                        .unwrap()
                        .with_language_mode(mode);
                for body in [
                    "\"??=\"",
                    "3\n#error injected",
                    "3/* comment */+4",
                    "3// comment",
                ] {
                    let mut config = Config::with_profile(profile);
                    config
                        .preprocessor
                        .defines
                        .insert("VALUE".into(), body.into());
                    config.preprocessor.forced_includes.clear();
                    config.preprocessor.line_comments = match config.preprocessor.line_comments {
                        toucan::LineComments::GnuC90 => toucan::LineComments::GnuC90Preprocessing,
                        toucan::LineComments::ClangC90 => {
                            toucan::LineComments::ClangC90Preprocessing
                        }
                        mode => mode,
                    };
                    let pp = toucan::Preprocessor::new(config.preprocessor)
                        .preprocess_str(&input, source)
                        .unwrap();
                    let mut native = Command::new(command);
                    native.args([
                        format!("-std={mode}"),
                        "-E".into(),
                        "-P".into(),
                        format!("-DVALUE={body}"),
                    ]);
                    if cross {
                        native.arg(format!("--target={target}"));
                    }
                    let output = native.arg(&input).output().unwrap();
                    assert_eq!(
                        toucan_test_support::compiler_acceptance(&output),
                        Ok(true),
                        "{command}: {profile:?}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let compact =
                        |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
                    assert_eq!(
                        compact(&pp.source),
                        compact(&String::from_utf8(output.stdout).unwrap()),
                        "{profile:?}: {body}"
                    );
                }
            }
        }
    }
}

#[test]
fn profile_language_mode_round_trips_and_old_profiles_default_to_gnu11() {
    for profile in CompilerProfile::ALL {
        let mut old = serde_json::to_value(profile).unwrap();
        old.as_object_mut().unwrap().remove("language_mode");
        assert_eq!(
            serde_json::from_value::<CompilerProfile>(old).unwrap(),
            profile
        );
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            let value = serde_json::to_value(profile).unwrap();
            assert_eq!(value["language_mode"], mode.to_string());
            assert_eq!(
                serde_json::from_value::<CompilerProfile>(value).unwrap(),
                profile
            );
            assert_eq!(mode.to_string().parse::<LanguageMode>().unwrap(), mode);
        }
    }
    assert_eq!("c89".parse::<LanguageMode>().unwrap(), LanguageMode::C90);
    assert_eq!(
        "iso9899:1990".parse::<LanguageMode>().unwrap(),
        LanguageMode::C90
    );
    assert_eq!(
        "gnu89".parse::<LanguageMode>().unwrap(),
        LanguageMode::Gnu90
    );
    assert!("c23".parse::<LanguageMode>().is_err());
}

#[test]
#[ignore = "requires rustc; supports TOUCAN_TEST_RUST_TOOLCHAIN"]
fn c11_identifier_macros_generate_working_rust_1_64() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = std::process::Command::new("rustc");
    if let Ok(toolchain) = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
        command.arg(format!("+{toolchain}"));
    }
    let version = command.arg("--version").arg("--verbose").output().unwrap();
    assert!(version.status.success(), "{version:?}");
    let version = String::from_utf8(version.stdout).unwrap();
    let host = version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap();
    let profile = CompilerProfile::default_for(Target::parse(host).unwrap())
        .with_language_mode(LanguageMode::C11);
    let mut config = Config::with_profile(profile);
    config.preprocessor.defines.clear();
    let parsed = toucan::parse_source(
        Path::new("mode.h"),
        "#define SUM (asm + typeof)\nenum {asm=3,typeof=5};\n",
        &config,
    )
    .unwrap();
    let (bindings, report) = parsed
        .bindings(&toucan::BindingOptions {
            rust_target: "1.64".parse().unwrap(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(report.language_mode, LanguageMode::C11);
    let input = directory.path().join("main.rs");
    let binary = directory.path().join("probe");
    std::fs::write(
        &input,
        format!("{bindings}\nfn main(){{assert_eq!(SUM,8);}}\n"),
    )
    .unwrap();
    let mut command = std::process::Command::new("rustc");
    if let Ok(toolchain) = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
        command.arg(format!("+{toolchain}"));
    }
    let output = command
        .args(["--edition=2021"])
        .arg(input)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = std::process::Command::new(binary).output().unwrap();
    assert!(output.status.success(), "{output:?}");
}

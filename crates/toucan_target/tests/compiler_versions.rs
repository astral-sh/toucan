use toucan_target::{Compiler, CompilerProfile, LanguageMode, Target};

#[test]
fn version_markers_identify_the_header_profile_in_every_c_mode() {
    for profile in CompilerProfile::ALL
        .into_iter()
        .flat_map(|p| LanguageMode::ALL.map(|m| p.with_language_mode(m)))
    {
        let macros = profile.predefined_macros();
        let windows = profile.target() == Target::X86_64PcWindowsMsvc;
        assert_eq!(
            macros.get("__STDC__").map(String::as_str),
            (!windows).then_some("1")
        );
        let gnu = if profile.compiler() == Compiler::Gnu {
            ["13", "3", "0"]
        } else {
            ["4", "2", "1"]
        };
        for (name, value) in ["__GNUC__", "__GNUC_MINOR__", "__GNUC_PATCHLEVEL__"]
            .into_iter()
            .zip(gnu)
        {
            assert_eq!(
                macros.get(name).map(String::as_str),
                (!windows).then_some(value),
                "{profile:?}: {name}"
            );
        }
        for (name, value) in [
            ("__clang__", "1"),
            ("__clang_major__", "18"),
            ("__clang_minor__", "1"),
            ("__clang_patchlevel__", "3"),
        ] {
            assert_eq!(
                macros.get(name).map(String::as_str),
                (profile.compiler() == Compiler::Clang).then_some(value),
                "{profile:?}: {name}"
            );
        }
        assert_eq!(macros.get("__STDC_HOSTED__").map(String::as_str), Some("1"));
        assert!(!macros.contains_key("__OPTIMIZE__"));
        assert!(!macros.contains_key("__AVX__"));
        assert!(!macros.contains_key("__ARM_FEATURE_SVE"));
    }
}

#[test]
#[ignore = "requires Clang with the supported cross targets"]
fn clang_standard_and_compatibility_markers_match_native_targets() {
    use std::process::Command;
    for target in Target::ALL {
        for mode in LanguageMode::ALL {
            let profile = CompilerProfile::new(target, Compiler::Clang)
                .unwrap()
                .with_language_mode(mode);
            let out = Command::new("clang")
                .arg(format!("--target={target}"))
                .arg(format!("-std={mode}"))
                .args(["-dM", "-E", "-x", "c", "-"])
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{target}/{mode}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let text = String::from_utf8(out.stdout).unwrap();
            let native = text
                .lines()
                .filter_map(|line| {
                    line.strip_prefix("#define ")
                        .and_then(|line| line.split_once(' '))
                })
                .collect::<std::collections::BTreeMap<_, _>>();
            let model = profile.predefined_macros();
            // Native vendor releases can have different Clang version numbers;
            // these stable header conventions remain independent of that number.
            for name in [
                "__STDC__",
                "__STDC_HOSTED__",
                "__STDC_VERSION__",
                "__GNUC__",
                "__GNUC_MINOR__",
                "__GNUC_PATCHLEVEL__",
            ] {
                assert_eq!(
                    native.get(name).copied(),
                    model.get(name).map(String::as_str),
                    "{target}/{mode}: {name}"
                );
            }
        }
    }
}

use toucan_target::{Compiler, CompilerProfile, LanguageMode, Target};

#[test]
fn version_markers_identify_the_header_profile_in_every_c_mode() {
    for profile in CompilerProfile::ALL
        .into_iter()
        .flat_map(|p| LanguageMode::ALL.map(|m| p.with_language_mode(m)))
    {
        let macros = profile.predefined_macros();
        let windows = profile.target().is_windows();
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
fn i686_macros_use_ilp32_and_compiler_specific_integer_types() {
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(Target::I686UnknownLinuxGnu, compiler).unwrap();
        let macros = profile.predefined_macros();
        for (name, value) in [
            ("__ILP32__", "1"),
            ("__i386__", "1"),
            ("__i686__", "1"),
            ("__SIZEOF_POINTER__", "4"),
            ("__POINTER_WIDTH__", "32"),
            ("__SIZEOF_LONG__", "4"),
            ("__SIZEOF_LONG_DOUBLE__", "12"),
            ("__INTPTR_TYPE__", "int"),
            ("__INT64_TYPE__", "long long int"),
            ("__INTMAX_TYPE__", "long long int"),
            ("__BIGGEST_ALIGNMENT__", "16"),
        ] {
            assert_eq!(&macros[name], value, "{compiler}: {name}");
        }
        assert!(!macros.contains_key("__LP64__"));
        assert!(!macros.contains_key("__x86_64__"));
        assert!(!macros.contains_key("__SIZEOF_INT128__"));
        assert_eq!(
            &macros["__WCHAR_TYPE__"],
            if compiler == Compiler::Gnu {
                "long int"
            } else {
                "int"
            }
        );
        assert_eq!(
            &macros["__INT_FAST16_TYPE__"],
            if compiler == Compiler::Gnu {
                "int"
            } else {
                "short"
            }
        );
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

#[test]
#[ignore = "requires Clang with the Windows ARM64 cross target"]
fn clang_arm64_windows_data_model_macros_match() {
    use std::process::Command;
    let target = Target::Aarch64PcWindowsMsvc;
    let out = Command::new("clang")
        .args([
            "--target=aarch64-pc-windows-msvc",
            "-std=gnu11",
            "-dM",
            "-E",
            "-x",
            "c",
            "-",
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let native = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("#define ")?.split_once(' '))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let macros = target.predefined_macros();
    for name in [
        "_WIN32",
        "_WIN64",
        "_M_ARM64",
        "__aarch64__",
        "__AARCH64EL__",
        "__SIZEOF_LONG__",
        "__SIZEOF_INT128__",
        "__SIZEOF_WCHAR_T__",
        "__WCHAR_WIDTH__",
        "__SIZEOF_LONG_DOUBLE__",
        "__LDBL_MANT_DIG__",
        "__BIGGEST_ALIGNMENT__",
    ] {
        assert_eq!(macros.get(name), native.get(name), "{name}");
    }
    for name in ["_M_X64", "_M_AMD64", "__LP64__", "__CHAR_UNSIGNED__"] {
        assert_eq!(macros.get(name), native.get(name), "{name}");
    }
}

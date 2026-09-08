use std::path::Path;
use toucan::{Compiler, CompilerProfile, Config, LanguageMode, Target};

#[test]
fn preprocessed_version_branches_and_caller_overrides_are_consistent() {
    for profile in CompilerProfile::ALL
        .into_iter()
        .flat_map(|p| LanguageMode::ALL.map(|m| p.with_language_mode(m)))
    {
        let windows = profile.target() == Target::X86_64PcWindowsMsvc;
        let mut config = Config::with_profile(profile);
        let source = r#"
#ifdef __GNUC__
enum { gcc_version=__GNUC__*10000+__GNUC_MINOR__*100+__GNUC_PATCHLEVEL__ };
#else
enum { gcc_version=0 };
#endif
#ifdef __clang__
enum { clang_version=__clang_major__*10000+__clang_minor__*100+__clang_patchlevel__ };
#else
enum { clang_version=0 };
#endif
#ifdef __STDC__
enum { standard_marker=1 };
#else
enum { standard_marker=0 };
#endif
"#;
        let a = toucan::parse_source(Path::new("versions.h"), source, &config).unwrap();
        config.analysis.retain_code = true;
        let b = toucan::parse_source(Path::new("versions.h"), source, &config).unwrap();
        assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit()));
        let integer = |name| {
            toucan::semantic::evaluate_integer(a.unit(), name)
                .unwrap()
                .value
        };
        assert_eq!(
            integer("gcc_version"),
            if windows {
                0
            } else if profile.compiler() == Compiler::Gnu {
                130300
            } else {
                40201
            }
        );
        assert_eq!(
            integer("clang_version"),
            if profile.compiler() == Compiler::Clang {
                180103
            } else {
                0
            }
        );
        assert_eq!(integer("standard_marker"), u128::from(!windows));
        config
            .preprocessor
            .defines
            .insert("__GNUC__".into(), "99".into());
        config
            .preprocessor
            .defines
            .insert("__GNUC_MINOR__".into(), "8".into());
        config
            .preprocessor
            .defines
            .insert("__GNUC_PATCHLEVEL__".into(), "7".into());
        config
            .preprocessor
            .defines
            .insert("__clang_major__".into(), "77".into());
        let a=toucan::parse_source(Path::new("overrides.h"),"enum { override_gnu=__GNUC__*10000+__GNUC_MINOR__*100+__GNUC_PATCHLEVEL__, override_clang=__clang_major__ };",&config).unwrap();
        assert_eq!(
            toucan::semantic::evaluate_integer(a.unit(), "override_gnu")
                .unwrap()
                .value,
            990807
        );
        assert_eq!(
            toucan::semantic::evaluate_integer(a.unit(), "override_clang")
                .unwrap()
                .value,
            77
        );
        assert_eq!(a.unit().compiler, profile.compiler());
        toucan::parse_source(Path::new("undef.h"),"#undef __GNUC__\n#ifdef __GNUC__\n#error compiler macro should stay undefined\n#endif\nint value;",&config).unwrap();
    }
}

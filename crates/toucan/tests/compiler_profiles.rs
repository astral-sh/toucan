use std::path::Path;
use toucan::{Compiler, CompilerProfile, Config, Target, parse_source};

#[test]
fn profiles_validate_and_round_trip_without_changing_physical_targets() {
    assert_eq!(CompilerProfile::ALL.len(), 15);
    for (index, target) in Target::ALL.into_iter().enumerate() {
        assert!(
            CompilerProfile::ALL.contains(&CompilerProfile::default_for(target)),
            "{index}: {target}"
        );
        assert_eq!(
            Config::new(target).profile(),
            CompilerProfile::default_for(target)
        );
        assert_eq!(
            CompilerProfile::new(target, Compiler::Gnu).is_ok(),
            target.is_linux() && !target.is_armv7()
        );
    }
    for profile in CompilerProfile::ALL {
        assert_eq!(
            serde_json::from_str::<CompilerProfile>(&serde_json::to_string(&profile).unwrap())
                .unwrap(),
            profile
        );
    }
    assert!(
        serde_json::from_str::<CompilerProfile>(
            r#"{"target":"X86_64AppleDarwin","compiler":"gcc"}"#
        )
        .is_err()
    );
    assert!("msvc".parse::<Compiler>().is_err());
}

#[test]
fn profile_macros_keep_linux_types_and_respect_caller_overrides() {
    for profile in CompilerProfile::ALL {
        let mut config = Config::with_profile(profile);
        let clang = profile.compiler() == Compiler::Clang;
        assert_eq!(config.preprocessor.defines.contains_key("__clang__"), clang);
        assert_eq!(
            config
                .preprocessor
                .defines
                .contains_key("__CLANG_ATOMIC_INT_LOCK_FREE"),
            clang
        );
        assert_eq!(config.target(), profile.target());
        assert_eq!(config.compiler(), profile.compiler());
        let source = format!(
            "_Static_assert(sizeof(__INT_FAST16_TYPE__)=={},\"fast16\"); _Static_assert(sizeof(__INT_FAST32_TYPE__)=={},\"fast32\");",
            if clang {
                2
            } else {
                profile.target().pointer_width() / 8
            },
            if clang {
                4
            } else {
                profile.target().pointer_width() / 8
            }
        );
        let mut source = source;
        if matches!(
            profile.target(),
            Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        ) {
            source.push_str("_Static_assert(_Generic((__INT64_TYPE__)0,long:1,default:0),\"Linux int64\"); _Static_assert(sizeof(long double)==16,\"Linux extended precision\");");
        }
        if profile.target() == Target::I686UnknownLinuxGnu {
            source.push_str("_Static_assert(_Generic((__INT64_TYPE__)0,long long:1,default:0),\"i686 int64\"); _Static_assert(sizeof(long double)==12,\"i686 extended precision\");");
        }
        if profile.target().is_armv7() {
            source.push_str("_Static_assert(_Generic((__INT64_TYPE__)0,long long:1,default:0),\"ARMv7 int64\"); _Static_assert(sizeof(long double)==8,\"ARMv7 double precision\"); _Static_assert(__ARM_PCS_VFP==1,\"hard-float ABI\");");
        }
        let compilation = parse_source(Path::new("profile.h"), &source, &config).unwrap();
        assert_eq!(compilation.unit().profile().unwrap(), profile);
        config.preprocessor.defines.remove("__clang__");
        config
            .preprocessor
            .defines
            .insert("__INT_FAST16_TYPE__".into(), "int".into());
        parse_source(Path::new("override.h"),"#ifdef __clang__\n#error caller undef ignored\n#endif\n_Static_assert(sizeof(__INT_FAST16_TYPE__)==4,\"override\");",&config).unwrap();
    }
}

#[test]
fn macro_evaluation_and_reports_preserve_compiler_identity() {
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, compiler).unwrap();
        let compilation = parse_source(
            Path::new("enum.h"),
            "enum E { A=0, B=1ULL<<40 };\n#define A_SIZE sizeof(A)\n",
            &Config::with_profile(profile),
        )
        .unwrap();
        let (source, report) = compilation
            .bindings(&toucan::BindingOptions {
                allowlist: vec!["A_SIZE".into()],
                ..Default::default()
            })
            .unwrap();
        assert!(
            source.contains(if compiler == Compiler::Clang {
                " = 4;"
            } else {
                " = 8;"
            }),
            "{source}"
        );
        assert_eq!(report.compiler, compiler);
        assert_eq!(
            serde_json::to_value(&report).unwrap()["compiler"],
            compiler.to_string()
        );
    }
}

use toucan_semantic::{Analysis, AnalysisOptions, analyze_with_profile, checked::X86Feature};
use toucan_target::{Compiler, CompilerProfile, Target};

fn profiles() -> impl Iterator<Item = CompilerProfile> {
    CompilerProfile::ALL.into_iter().filter(|profile| {
        matches!(
            profile.target(),
            Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::X86_64AppleDarwin
                | Target::X86_64PcWindowsMsvc
        )
    })
}

fn check(source: &str, profile: CompilerProfile) -> Result<Analysis, toucan_semantic::Error> {
    let plain = analyze_with_profile(source, profile, &AnalysisOptions::default());
    let kept = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&plain, &kept) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => assert_eq!((a.offset, &a.message), (b.offset, &b.message)),
        _ => panic!("analysis mismatch {profile:?}: {source}: {plain:?} / {kept:?}"),
    }
    kept
}

#[test]
fn optional_features_follow_written_order_and_preserve_the_baseline() {
    for profile in profiles() {
        for (name, feature) in [
            ("lzcnt", X86Feature::Lzcnt),
            ("bmi", X86Feature::Bmi),
            ("bmi2", X86Feature::Bmi2),
        ] {
            for (options, enabled) in [
                (name.to_owned(), true),
                (format!("no-{name}"), false),
                (format!("{name},no-{name}"), false),
                (format!("no-{name},{name}"), true),
            ] {
                let source =
                    format!("__attribute__((target(\"{options}\"))) int f(void){{return 1;}}");
                let analysis = check(&source, profile).unwrap();
                let target = analysis.unit().function_options[&0].target().unwrap();
                assert_eq!(target.enables(feature), enabled);
                for baseline in [X86Feature::Mmx, X86Feature::Sse, X86Feature::Sse2] {
                    assert!(target.enables(baseline));
                }
                analysis.unit().validate_function_options().unwrap();
            }
        }
    }
}

fn calls() -> Vec<(String, bool, bool)> {
    let mut cases = Vec::new();
    for feature in ["lzcnt", "bmi", "bmi2", "lzcnt,bmi,bmi2"] {
        for inline in [false, true] {
            let prefix = format!(
                "__attribute__((target(\"{feature}\"){})) {} int g(int x){{return x;}}",
                if inline { ",always_inline" } else { "" },
                if inline { "inline" } else { "" }
            );
            for (caller, body, expected) in [
                (String::new(), "return g(x);", !inline),
                (String::new(), "if(0)return g(x);return x;", true),
                (String::new(), "return sizeof(g(x));", true),
                (
                    format!("__attribute__((target(\"{feature}\")))"),
                    "return g(x);",
                    true,
                ),
            ] {
                cases.push((
                    format!("{prefix}{caller} int f(int x){{{body}}}"),
                    expected,
                    expected,
                ));
            }
        }
    }
    cases.push(("int g(int);int f(int x){return g(x);}__attribute__((target(\"bmi2\"),always_inline)) int g(int x){return x;}".into(), false, true));
    cases.push(("int g(int);int f(int x){{extern int g(int) __attribute__((target(\"bmi2\"),always_inline));}return g(x);}int g(int x){return x;}".into(), false, true));
    cases.push(("int g(int);int f(int x){return g(x);}int h(int x){extern int g(int) __attribute__((target(\"bmi2\"),always_inline));return x;}int g(int x){return x;}".into(), false, true));
    cases
}

#[test]
fn inlining_preserves_feature_requirements_and_later_gnu_annotations() {
    for profile in profiles() {
        for (source, gnu, clang) in calls() {
            let result = check(&source, profile);
            assert_eq!(
                result.is_ok(),
                if profile.compiler() == Compiler::Gnu {
                    gnu
                } else {
                    clang
                },
                "{profile:?}: {source}: {result:?}"
            );
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang; checks x86-64 target code generation"]
fn inlining_constraints_match_c_compilers() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("target.c");
    for profile in profiles() {
        if profile.compiler() == Compiler::Gnu
            && !cfg!(all(target_arch = "x86_64", target_os = "linux"))
        {
            continue;
        }
        for optimization in ["-O0", "-O2"] {
            for (source, gnu, clang) in calls() {
                std::fs::write(&input, &source).unwrap();
                let compiler = if profile.compiler() == Compiler::Gnu {
                    std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into())
                } else {
                    "clang".into()
                };
                let mut command = std::process::Command::new(compiler);
                if profile.compiler() == Compiler::Clang {
                    command.args(["-target", profile.target().triple()]);
                }
                let result = command
                    .args(["-std=gnu11", optimization, "-S"])
                    .arg(&input)
                    .arg("-o")
                    .arg(directory.path().join("target.s"))
                    .output()
                    .unwrap();
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&result).unwrap(),
                    if profile.compiler() == Compiler::Gnu {
                        gnu
                    } else {
                        clang
                    },
                    "{profile:?} {optimization}: {source}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
        }
    }
}

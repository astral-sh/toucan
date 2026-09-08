use toucan_semantic::{
    AnalysisOptions, analyze_with_profile, evaluate_arithmetic, evaluate_integer,
};
use toucan_target::{CompilerProfile, LanguageMode, Target};

#[test]
fn language_modes_preserve_identifiers_extensions_and_retained_parity() {
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            let standard = mode == LanguageMode::C11;
            let assembly = profile.target() != Target::X86_64PcWindowsMsvc;
            for (source, accepted) in [
                (
                    "int asm;int typeof;int f(void){asm=1;return typeof+asm;}",
                    standard,
                ),
                ("void asm(const char*);void f(void){asm(\"\");}", standard),
                ("typedef int typeof;typeof x;", standard),
                (
                    "struct S{int asm,typeof;};int f(struct S*s){return s->asm+s->typeof;}",
                    standard,
                ),
                ("void f(void){asm(\"\");}", !standard && assembly),
                ("int x asm(\"named\");", !standard),
                ("int x;typeof(x) y;", !standard),
                ("void f(void){__asm__(\"\");}", assembly),
                ("int x __asm__(\"named\");__typeof__(x) y;", true),
                ("int f(void){return ({__auto_type x=3;x;});}", true),
            ] {
                let plain = analyze_with_profile(source, profile, &Default::default());
                let retained = analyze_with_profile(
                    source,
                    profile,
                    &AnalysisOptions {
                        retain_code: true,
                        ..Default::default()
                    },
                );
                match (plain, retained) {
                    (Ok(a), Ok(b)) => {
                        assert!(accepted, "{profile:?}: {source}");
                        assert_eq!(a.unit().language_mode, mode);
                        assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit()));
                    }
                    (Err(a), Err(b)) => {
                        assert!(!accepted, "{profile:?}: {source}: {a}");
                        assert_eq!((a.offset, a.message), (b.offset, b.message));
                    }
                    other => panic!("parity: {profile:?}: {source}: {other:?}"),
                }
            }
        }
    }
}

#[test]
fn constant_evaluation_fragments_inherit_the_unit_mode() {
    for profile in CompilerProfile::ALL {
        let standard = profile.with_language_mode(LanguageMode::C11);
        let analysis = analyze_with_profile(
            "enum {asm=3,typeof=5};typedef int T;",
            standard,
            &Default::default(),
        )
        .unwrap();
        assert_eq!(
            evaluate_integer(analysis.unit(), "asm+typeof")
                .unwrap()
                .value,
            8
        );
        assert_eq!(
            evaluate_integer(analysis.unit(), "sizeof(T)")
                .unwrap()
                .value,
            4
        );
        evaluate_arithmetic(analysis.unit(), "(double)(asm+typeof)").unwrap();
        let analysis =
            analyze_with_profile("typedef int typeof;", standard, &Default::default()).unwrap();
        assert_eq!(
            evaluate_integer(analysis.unit(), "sizeof(typeof)")
                .unwrap()
                .value,
            4
        );
        let analysis = analyze_with_profile("", profile, &Default::default()).unwrap();
        assert_eq!(
            evaluate_integer(analysis.unit(), "sizeof(typeof(1))")
                .unwrap()
                .value,
            4
        );
    }
}

#[test]
#[ignore = "requires GCC and Clang"]
fn keyword_modes_match_native_compilers() {
    use std::process::Command;
    use toucan_target::Compiler;
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("modes.c");
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
            let profile =
                CompilerProfile::new(target, if gnu { Compiler::Gnu } else { Compiler::Clang })
                    .unwrap();
            for mode in LanguageMode::ALL {
                let standard = mode == LanguageMode::C11;
                let profile = profile.with_language_mode(mode);
                for (source, expected) in [
                    (
                        "int asm;int typeof;int f(void){return asm+typeof;}",
                        standard,
                    ),
                    ("void asm(const char*);void f(void){asm(\"\");}", standard),
                    ("typedef int typeof;typeof x;", standard),
                    ("int x asm(\"named\");", !standard),
                    ("int x;typeof(x) y;", !standard),
                    ("int x __asm__(\"named\");__typeof__(x) y;", true),
                    ("int f(void){return ({__auto_type x=3;x;});}", true),
                    ("int n=_Alignof(void);", true),
                ] {
                    std::fs::write(&input, source).unwrap();
                    let mut invocation = Command::new(command);
                    invocation.args([
                        format!("-std={mode}"),
                        "-Werror=implicit-function-declaration".into(),
                        "-fsyntax-only".into(),
                    ]);
                    if cross {
                        invocation.arg(format!("--target={target}"));
                    }
                    let output = invocation.arg(&input).output().unwrap();
                    assert_eq!(
                        toucan_test_support::compiler_acceptance(&output),
                        Ok(expected),
                        "{command}: {profile:?}: {source}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert_eq!(
                        analyze_with_profile(source, profile, &Default::default()).is_ok(),
                        expected,
                        "{profile:?}: {source}"
                    );
                }
            }
        }
    }
}

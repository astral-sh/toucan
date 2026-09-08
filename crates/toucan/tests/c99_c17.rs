use std::path::Path;
use toucan::{Compiler, CompilerProfile, Config, LanguageMode, Target};

const MODES: [(LanguageMode, &str); 4] = [
    (LanguageMode::C99, "199901L"),
    (LanguageMode::Gnu99, "199901L"),
    (LanguageMode::C17, "201710L"),
    (LanguageMode::Gnu17, "201710L"),
];

#[test]
fn standard_versions_select_header_branches_without_changing_the_abi() {
    for profile in CompilerProfile::ALL {
        for (mode, version) in MODES {
            let profile = profile.with_language_mode(mode);
            let config = Config::with_profile(profile);
            let unicode = profile.compiler() == Compiler::Clang || mode != LanguageMode::C99;
            assert_eq!(config.preprocessor.defines["__STDC_VERSION__"], version);
            assert_eq!(
                config.preprocessor.defines.contains_key("__STDC_UTF_16__"),
                unicode
            );
            assert_eq!(
                config.preprocessor.defines.contains_key("__STDC_UTF_32__"),
                unicode
            );
            assert_eq!(
                config
                    .preprocessor
                    .defines
                    .contains_key("__GNUC_STDC_INLINE__"),
                profile.target() != Target::X86_64PcWindowsMsvc
            );
            assert!(
                !config
                    .preprocessor
                    .defines
                    .contains_key("__GNUC_GNU_INLINE__")
            );
            let source = format!(
                "#if __STDC_VERSION__ != {version}\n#error wrong mode\n#endif\n#define VERSION __STDC_VERSION__\nstruct S{{char a;long b;}};int f(int *restrict);\n"
            );
            let parsed = toucan::parse_source(Path::new("mode.h"), &source, &config).unwrap();
            let (bindings, report) = parsed.bindings(&Default::default()).unwrap();
            assert_eq!(report.language_mode, mode);
            assert!(bindings.contains("pub const VERSION:"));
            assert_eq!(parsed.unit().profile().unwrap(), profile);
        }
    }
}

#[test]
#[ignore = "requires native C compiler and rustc; supports TOUCAN_TEST_RUST_TOOLCHAIN"]
fn generated_c99_and_c17_bindings_call_inline_helpers_and_round_trip_structs() {
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let compiler = std::env::var("CC").unwrap_or_else(|_| "cc".into());
    let mut rustc = Command::new("rustc");
    if let Ok(toolchain) = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
        rustc.arg(format!("+{toolchain}"));
    }
    let version = rustc.args(["--version", "--verbose"]).output().unwrap();
    assert!(version.status.success(), "{version:?}");
    let version = String::from_utf8(version.stdout).unwrap();
    let host = version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap();
    let target = Target::parse(host).unwrap();
    let native = Command::new(&compiler).arg("--version").output().unwrap();
    assert!(native.status.success(), "{native:?}");
    let family = if String::from_utf8_lossy(&native.stdout)
        .to_ascii_lowercase()
        .contains("clang")
    {
        Compiler::Clang
    } else {
        Compiler::Gnu
    };
    let profile = CompilerProfile::new(target, family).unwrap();
    let source = "#define MODE_VERSION __STDC_VERSION__\ntypedef struct ModeValue{unsigned long version;int sum;} ModeValue;\nstatic inline int mode_sum(int n,const int *restrict values){int total=0;for(int i=0;i<n;i++)total+=values[i];return total;} // C99 loop and comment\nModeValue mode_value(int,const int*);ModeValue mode_round_trip(ModeValue);\n";
    let implementation = "ModeValue mode_value(int n,const int*values){return (ModeValue){.version=__STDC_VERSION__,.sum=mode_sum(n,values)};} ModeValue mode_round_trip(ModeValue value){value.sum+=1;return value;}\n";
    let input = directory.path().join("mode.c");
    let object = directory.path().join("mode.o");
    let rust = directory.path().join("main.rs");
    let executable = directory.path().join("probe");
    for (mode, version) in MODES {
        let parsed = toucan::parse_source(
            Path::new("mode.h"),
            source,
            &Config::with_profile(profile.with_language_mode(mode)),
        )
        .unwrap();
        let (bindings, report) = parsed
            .bindings(&toucan::BindingOptions {
                rust_target: "1.64".parse().unwrap(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(report.language_mode, mode);
        std::fs::write(&input, format!("{source}{implementation}")).unwrap();
        let result = Command::new(&compiler)
            .args([format!("-std={mode}"), "-Werror".into(), "-c".into()])
            .arg(&input)
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{compiler} {mode}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let version = version.trim_end_matches('L');
        std::fs::write(&rust,format!("{bindings}\nfn main(){{assert_eq!(MODE_VERSION,{version});unsafe{{let a=[3,7,11];let v=mode_value(3,a.as_ptr());assert_eq!((v.version,v.sum),({version},21));let w=mode_round_trip(v);assert_eq!((w.version,w.sum),({version},22));}}}}\n")).unwrap();
        let mut command = Command::new("rustc");
        if let Ok(toolchain) = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
            command.arg(format!("+{toolchain}"));
        }
        let result = command
            .args(["--edition=2021", "-C"])
            .arg(format!("link-arg={}", object.display()))
            .arg(&rust)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = Command::new(&executable).output().unwrap();
        assert!(result.status.success(), "{mode}: {result:?}");
    }
}

use std::path::Path;
use std::process::Command;

use toucan::{BindingOptions, Compiler, CompilerProfile, Config, LanguageMode, Target};

#[test]
#[ignore = "requires native C and Rust compilers; supports TOUCAN_TEST_RUST_TOOLCHAIN"]
fn generated_c90_bindings_call_compiled_definitions_and_preserve_constants() {
    let mut rustc = Command::new("rustc");
    let toolchain = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN").ok();
    if let Some(toolchain) = &toolchain {
        rustc.arg(format!("+{toolchain}"));
    }
    let version = rustc.args(["--version", "--verbose"]).output().unwrap();
    assert!(version.status.success(), "{version:?}");
    let version = String::from_utf8(version.stdout).unwrap();
    let host: Target = version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap()
        .parse()
        .unwrap();
    let compiler = std::env::var("CC").unwrap_or_else(|_| "cc".into());
    let version = Command::new(&compiler).arg("--version").output().unwrap();
    assert!(version.status.success(), "{version:?}");
    let clang = String::from_utf8_lossy(&version.stdout)
        .to_ascii_lowercase()
        .contains("clang");
    let profile = CompilerProfile::new(
        host,
        if clang {
            Compiler::Clang
        } else {
            Compiler::Gnu
        },
    )
    .unwrap();
    let source = "#define LARGE 9223372036854775808\ntypedef struct Pair { int a,b; } Pair;\nstatic int add(a,b) int a; {return a+b;}\nstatic int bridge(void){return delayed(17,20);}\nint delayed(int,int);\nint c90_call(void);\nPair c90_pair(Pair);\n";
    let implementation = "int delayed(int a,int b){return add(a,b);}\nint c90_call(void){return bridge();}\nPair c90_pair(Pair p){p.a+=c90_call();p.b+=2;return p;}\n";
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("c90.c");
    let object = directory.path().join("c90.o");
    std::fs::write(&input, format!("{source}{implementation}")).unwrap();
    for mode in [LanguageMode::C90, LanguageMode::Gnu90] {
        let config = Config::with_profile(profile.with_language_mode(mode));
        let compilation = toucan::parse_source(Path::new("c90.c"), source, &config).unwrap();
        let (bindings, report) = compilation
            .bindings(&BindingOptions {
                allowlist: vec!["c90_*".into(), "Pair".into(), "LARGE".into()],
                rust_target: "1.64".parse().unwrap(),
                ..Default::default()
            })
            .unwrap();
        assert!(report.skipped_macros.is_empty(), "{report:?}");
        let output = Command::new(&compiler)
            .arg(format!("-std={mode}"))
            .arg("-c")
            .arg(&input)
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(true),
            "{output:?}"
        );
        let rust = directory.path().join("main.rs");
        let executable = directory.path().join("c90-probe");
        std::fs::write(&rust, format!("{bindings}\nfn main(){{assert_eq!(LARGE,1u64<<63);unsafe{{assert_eq!(c90_call(),37);let p=c90_pair(Pair{{a:1,b:2}});assert_eq!((p.a,p.b),(38,4));}}}}\n")).unwrap();
        let mut rustc = Command::new("rustc");
        if let Some(toolchain) = &toolchain {
            rustc.arg(format!("+{toolchain}"));
        }
        let output = rustc
            .args(["--edition=2021", "-C"])
            .arg(format!("link-arg={}", object.display()))
            .arg(&rust)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(true),
            "{output:?}"
        );
        let output = Command::new(&executable).output().unwrap();
        assert!(output.status.success(), "{output:?}");
    }
}

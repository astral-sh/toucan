#[cfg(unix)]
#[test]
#[ignore = "requires native C compiler and rustc; supports TOUCAN_TEST_RUST_TOOLCHAIN"]
fn bindings_call_omitted_conditionals_with_single_evaluation() {
    use std::{path::Path, process::Command};
    use toucan::{Compiler, CompilerProfile, Config, LanguageMode, Target};
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
    let header = "#define OMITTED_VALUE (7 ?: 9)\n#define OMITTED_ROUNDED (16777217.0f ?: 0.0)\ntypedef struct Choice{int value;int calls;}Choice;static __inline__ int take(int value,int*calls){++*calls;return value;}Choice choose(int,int);Choice increment(volatile int*,int);Choice vla_choice(int);double float_choice(float,double);\n";
    let implementation = "Choice choose(int x,int y){Choice result={0,0};result.value=take(x,&result.calls) ?: take(y,&result.calls);return result;}Choice increment(volatile int*p,int fallback){Choice result;result.value=(*p)++ ?: fallback;result.calls=*p;return result;}Choice vla_choice(int n){int a[2][n];int(*p)[n]=a;Choice result;result.value=sizeof(*(p++ ?: a));result.calls=p==a+1;return result;}double float_choice(float x,double fallback){return x ?: fallback;}\n";
    let input = directory.path().join("conditional.c");
    let object = directory.path().join("conditional.o");
    let rust = directory.path().join("main.rs");
    let executable = directory.path().join("probe");
    for mode in LanguageMode::ALL {
        let parsed = toucan::parse_source(
            Path::new("conditional.h"),
            header,
            &Config::with_profile(profile.with_language_mode(mode)),
        )
        .unwrap();
        let (bindings, _) = parsed
            .bindings(&toucan::BindingOptions {
                rust_target: "1.64".parse().unwrap(),
                ..Default::default()
            })
            .unwrap();
        assert!(bindings.contains("pub const OMITTED_VALUE:"), "{bindings}");
        assert!(
            bindings.contains("pub const OMITTED_ROUNDED:"),
            "{bindings}"
        );
        std::fs::write(&input, format!("{header}{implementation}")).unwrap();
        std::fs::write(&rust,format!("{bindings}\nfn main(){{assert_eq!(OMITTED_VALUE,7);assert_eq!(OMITTED_ROUNDED,16777216.0);unsafe{{for (x,y,v,c) in [(7,9,7,1),(0,9,9,2)]{{let r=choose(x,y);assert_eq!((r.value,r.calls),(v,c));}}for (x,v,c) in [(7,7,8),(0,9,1)]{{let mut x=x;let r=increment(&mut x,9);assert_eq!((r.value,r.calls,x),(v,c,c));}}let r=vla_choice(3);assert_eq!((r.value,r.calls),(12,1));assert_eq!(float_choice(1.5,9.0),1.5);assert_eq!(float_choice(0.0,9.0),9.0);}}}}\n")).unwrap();
        for optimization in ["-O0", "-O2"] {
            let output = Command::new(&compiler)
                .arg(format!("-std={mode}"))
                .args([optimization, "-c"])
                .arg(&input)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler} {mode} {optimization}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let mut rustc = Command::new("rustc");
            if let Ok(toolchain) = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
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
            assert!(
                output.status.success(),
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(&executable).output().unwrap();
            assert!(output.status.success(), "{mode} {optimization}: {output:?}");
        }
    }
}

#[test]
fn preprocessing_rejects_omitted_operands_in_if_expressions() {
    use std::path::Path;
    use toucan::{CompilerProfile, Config, LanguageMode};
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let config = Config::with_profile(profile.with_language_mode(mode));
            for expression in ["1 ?: 2", "0 && (1 ?: 2)", "0 ?: 0 ?: 3"] {
                let source = format!("#if {expression}\nint yes;\n#endif\n");
                assert!(
                    toucan::parse_source(Path::new("conditional.h"), &source, &config).is_err()
                );
            }
            toucan::parse_source(
                Path::new("conditional.h"),
                "#if 1 ? 1 : 0\nint yes;\n#endif\n",
                &config,
            )
            .unwrap();
        }
    }
}

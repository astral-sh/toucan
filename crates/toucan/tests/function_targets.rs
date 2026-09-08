use toucan::{Compiler, CompilerProfile, Config, Target};

const HEADER: &str = r#"
__attribute__((target("mmx"),min_vector_width(128))) int add(int, int);
__attribute__((target("no-mmx"),min_vector_width(64))) int callback(int (*)(int), int);
"#;

#[test]
fn function_targets_preserve_the_ordinary_external_call_abi() {
    for profile in CompilerProfile::ALL.into_iter().filter(|profile| {
        matches!(
            profile.target(),
            Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::X86_64AppleDarwin
                | Target::X86_64PcWindowsMsvc
        )
    }) {
        let compile = |source| {
            toucan::parse_source(
                std::path::Path::new("api.h"),
                source,
                &Config::with_profile(profile),
            )
            .unwrap()
        };
        let annotated = compile(HEADER);
        let ordinary = compile("int add(int,int); int callback(int (*)(int),int);");
        let options = toucan::BindingOptions::default();
        assert_eq!(
            annotated.bindings(&options).unwrap().0,
            ordinary.bindings(&options).unwrap().0
        );
        let mut invalid = annotated.unit().clone();
        invalid.function_options.insert(
            usize::MAX,
            invalid.function_options.values().next().unwrap().clone(),
        );
        assert!(
            toucan_bindings::generate(&invalid, &options)
                .unwrap_err()
                .to_string()
                .contains("invalid function")
        );
    }
}

#[test]
#[ignore = "requires native x86-64 Linux GCC/Clang and rustc; supports TOUCAN_TEST_RUST_TOOLCHAIN"]
fn generated_calls_and_callbacks_cross_function_target_boundaries() {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let implementation = format!(
        "{HEADER}\nint add(int a,int b){{__builtin_ia32_emms();return a+b;}} int callback(int(*f)(int),int x){{return f(x)+3;}}\n"
    );
    std::fs::write(directory.path().join("api.c"), &implementation).unwrap();
    std::fs::write(
        directory.path().join("main.rs"),
        r#"
#![allow(non_camel_case_types,non_snake_case,dead_code)]
include!("bindings.rs");
unsafe extern "C" fn twice(x:i32)->i32{x*2}
fn main(){unsafe{assert_eq!(add(-17,29),12);assert_eq!(callback(Some(twice),-5),-7);}}
"#,
    )
    .unwrap();
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, compiler).unwrap();
        let config = Config::with_profile(profile);
        toucan::parse_source(std::path::Path::new("api.c"), &implementation, &config).unwrap();
        let compilation =
            toucan::parse_source(std::path::Path::new("api.h"), HEADER, &config).unwrap();
        let options = toucan::BindingOptions {
            rust_target: "1.64".parse().unwrap(),
            ..Default::default()
        };
        std::fs::write(
            directory.path().join("bindings.rs"),
            compilation.bindings(&options).unwrap().0,
        )
        .unwrap();
        let cc = if compiler == Compiler::Gnu {
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into())
        } else {
            "clang".into()
        };
        for optimization in ["-O0", "-O2"] {
            let output = std::process::Command::new(&cc)
                .current_dir(directory.path())
                .args(["-std=gnu11", optimization, "-c", "api.c", "-o", "api.o"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let mut rustc = std::process::Command::new("rustc");
            if let Ok(toolchain) = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
                rustc.arg(format!("+{toolchain}"));
            }
            let output = rustc
                .current_dir(directory.path())
                .args([
                    "--edition=2021",
                    "main.rs",
                    "-C",
                    "link-arg=api.o",
                    "-o",
                    "probe",
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = std::process::Command::new(directory.path().join("probe"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn sparse_target_metadata_serializes_without_changing_empty_outputs() {
    let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    let options = toucan::AnalysisOptions {
        retain_code: true,
        ..Default::default()
    };
    let source = r#"__attribute__((target("mmx"),always_inline)) inline int add(int x){return x+1;} int call(int x){return add(x);}"#;
    let analysis = toucan::semantic::analyze_with_profile(source, profile, &options).unwrap();
    let unit = serde_json::to_value(analysis.unit()).unwrap();
    assert_eq!(unit["function_options"]["0"]["target"]["options"][0], "Mmx");
    let code = serde_json::to_value(analysis.checked().unwrap()).unwrap();
    assert_eq!(code["function_options"].as_object().unwrap().len(), 1);
    let calls = code["inline_targets"].as_object().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls.values().next().unwrap()["stage"], "CodeGeneration");
    let analysis =
        toucan::semantic::analyze_with_profile("int f(void);", profile, &options).unwrap();
    assert!(
        serde_json::to_value(analysis.unit())
            .unwrap()
            .get("function_options")
            .is_none()
    );
    let code = serde_json::to_value(analysis.checked().unwrap()).unwrap();
    assert!(code.get("function_options").is_none());
    assert!(code.get("inline_targets").is_none());
}

#[test]
fn minimum_vector_width_serializes_as_an_optional_declaration_hint() {
    let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    let analysis = toucan::semantic::analyze_with_profile(
        "__attribute__((min_vector_width(128), min_vector_width(256))) int f(void);",
        profile,
        &toucan::AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    )
    .unwrap();
    let unit = serde_json::to_value(analysis.unit()).unwrap();
    assert_eq!(unit["function_options"]["0"]["minimum_vector_width"], 128);
    let checked = serde_json::to_value(analysis.checked().unwrap()).unwrap();
    let sites = checked["function_options"].as_object().unwrap();
    let site = sites.values().next().unwrap();
    assert_eq!(site["minimum_vector_width"][0]["value"], 128);
    assert_eq!(site["minimum_vector_width"][1]["value"], 256);
}

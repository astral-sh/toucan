use std::path::Path;
use toucan::{BindingOptions, CompilerProfile, Config, RustTarget};

const HEADER: &str = r#"
struct __attribute__((packed)) S { char lead; int member; };
extern struct S value;
extern int over __attribute__((aligned(32)));
extern int under __attribute__((aligned(1)));
#define ALIGN_FIELD __alignof__(value.member)
#define ALIGN_OVER _Alignof(over)
#define ALIGN_UNDER __alignof__(under)
#define ALIGN_POINTER __alignof__(*(&over))
"#;

#[test]
fn macro_queries_keep_object_alignment_and_size_t_integer_metadata() {
    for profile in CompilerProfile::ALL {
        let config = Config::with_profile(profile);
        let compilation = toucan::parse_source(Path::new("alignment.h"), HEADER, &config).unwrap();
        let options = BindingOptions {
            allowlist: vec!["ALIGN_*".into()],
            rust_target: RustTarget::RUST_1_64,
            ..Default::default()
        };
        let (source, report) = compilation.bindings(&options).unwrap();
        for (name, value) in [("ALIGN_FIELD", 1), ("ALIGN_OVER", 32), ("ALIGN_UNDER", 1)] {
            let expression = compilation
                .preprocessed()
                .expand_object_macro(name)
                .unwrap()
                .unwrap();
            let constant =
                toucan::semantic::evaluate_integer(compilation.unit(), &expression).unwrap();
            assert_eq!(constant.value, value);
            assert!(!constant.signed);
            assert_eq!(u64::from(constant.bits), profile.target().pointer_width());
            assert!(source.contains(&format!("pub const {name}:")));
        }
        assert_eq!(report.integer_macros, 4);
        assert!(!source.contains("pub static"));
    }
}

#[test]
#[ignore = "requires a native Rust compiler; run with --include-ignored"]
fn generated_alignment_constants_compile_on_the_requested_rust_target() {
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let rustc = std::env::var("TOUCAN_TEST_RUSTC").unwrap_or_else(|_| "rustc".into());
    let host = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => toucan::Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => toucan::Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => toucan::Target::X86_64AppleDarwin,
        ("aarch64", "macos") => toucan::Target::Aarch64AppleDarwin,
        ("x86_64", "windows") => toucan::Target::X86_64PcWindowsMsvc,
        _ => return,
    };
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|profile| profile.target() == host)
    {
        let compilation = toucan::parse_source(
            Path::new("alignment.h"),
            HEADER,
            &Config::with_profile(profile),
        )
        .unwrap();
        let (source, _) = compilation
            .bindings(&BindingOptions {
                allowlist: vec!["ALIGN_*".into()],
                rust_target: RustTarget::RUST_1_64,
                ..Default::default()
            })
            .unwrap();
        let source = format!(
            "{source}\nconst _: () = assert!(ALIGN_FIELD==1 && ALIGN_OVER==32 && ALIGN_UNDER==1);\n"
        );
        let input = directory.path().join("bindings.rs");
        std::fs::write(&input, source).unwrap();
        let output = Command::new(&rustc)
            .args(["--edition=2021", "--crate-type=lib", "-Dwarnings"])
            .arg(&input)
            .arg("-o")
            .arg(directory.path().join("bindings.rlib"))
            .output()
            .unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(true),
            "{profile:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

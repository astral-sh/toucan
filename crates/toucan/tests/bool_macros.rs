use std::path::Path;
use toucan::{BindingOptions, Config, MacroType, RustTarget, Target};

const HEADER: &str = include_str!("fixtures/bool_macros.h");
const BOOL_NAMES: &[&str] = &[
    "B_FALSE",
    "B_TRUE",
    "B_ALIAS",
    "B_GENERIC",
    "B_KEEP",
    "B_OVERRIDE",
    "B_GROUP_KEEP",
    "B_GROUP_OTHER",
    "B_QUERY",
    "type",
];
fn parse(target: Target) -> toucan::Compilation {
    let mut config = Config::new(target);
    config.preprocessor.defines.clear();
    toucan::parse_source(Path::new("booleans.h"), HEADER, &config).unwrap()
}
fn cases() -> [(BindingOptions, Vec<&'static str>); 3] {
    [
        (BindingOptions::default(), BOOL_NAMES.to_vec()),
        (
            BindingOptions {
                macro_type: MacroType::Unsigned,
                macro_type_overrides: [
                    ("B_KEEP".into(), MacroType::C),
                    ("B_GROUP*".into(), MacroType::C),
                    ("B_GROUP_OTHER".into(), MacroType::Unsigned),
                    ("type".into(), MacroType::C),
                ]
                .into(),
                ..Default::default()
            },
            vec!["B_KEEP", "B_GROUP_KEEP", "type"],
        ),
        (
            BindingOptions {
                macro_type_overrides: [
                    ("B*".into(), MacroType::Unsigned),
                    ("B_GROUP*".into(), MacroType::C),
                    ("B_GROUP_OTHER".into(), MacroType::Unsigned),
                    ("B_KEEP".into(), MacroType::C),
                    ("type".into(), MacroType::Unsigned),
                ]
                .into(),
                ..Default::default()
            },
            vec!["B_KEEP", "B_GROUP_KEEP"],
        ),
    ]
}

#[test]
fn policies_preserve_boolean_identity_and_enum_alias_integer_types() {
    for target in Target::ALL {
        let compilation = parse(target);
        let enumeration =
            toucan::semantic::evaluate_integer(compilation.unit(), "ENUM_TRUE").unwrap();
        assert_eq!(
            (enumeration.bits, enumeration.signed, enumeration.rank),
            (32, true, 3)
        );
        for (options, booleans) in cases() {
            let (source, report) = compilation.bindings(&options).unwrap();
            for name in BOOL_NAMES {
                let rust_name = if *name == "type" { "r#type" } else { name };
                let expected = if booleans.contains(name) {
                    "bool"
                } else {
                    "u32"
                };
                assert!(
                    source.contains(&format!(
                        "pub const {rust_name}: ::core::primitive::{expected} ="
                    )),
                    "{target} {name}: {source}"
                );
                let normalization = report.macro_types.iter().find(|p| p.c_name == *name);
                if expected == "bool" {
                    assert!(normalization.is_none());
                } else {
                    let projection = normalization.unwrap();
                    assert_eq!(
                        (
                            projection.c_bits,
                            projection.c_signed,
                            projection.rust_bits,
                            projection.rust_signed
                        ),
                        (8, false, 32, false)
                    );
                }
            }
            for name in [
                "COMPARE",
                "LOGICAL",
                "NEGATE",
                "CONDITIONAL",
                "ENUM_ALIAS",
                "true",
            ] {
                let rust_name = if name == "true" { "r#true" } else { name };
                let expected = if options.macro_type == MacroType::Unsigned {
                    "u32"
                } else {
                    "i32"
                };
                assert!(
                    source.contains(&format!(
                        "pub const {rust_name}: ::core::primitive::{expected} ="
                    )),
                    "{name}: {source}"
                );
            }
            for name in ["ENUM_TRUE", "ENUM_FALSE", "SELF"] {
                assert!(!source.contains(&format!("pub const {name}: ::core::primitive::bool")));
                assert!(
                    report
                        .enum_constants
                        .iter()
                        .any(|e| e.emitted.iter().any(|v| v.c_name == name))
                );
            }
            assert_eq!(
                report.renamed_macros.get("r#type").map(String::as_str),
                Some("type")
            );
            assert!(
                report.skipped_macros.is_empty(),
                "{:?}",
                report.skipped_macros
            );
            assert_eq!(
                toucan::semantic::evaluate_integer(compilation.unit(), "ENUM_TRUE").unwrap(),
                enumeration
            );
        }
    }
}

#[test]
fn packed_enum_bool_initializers_keep_int_macro_aliases() {
    for profile in toucan::CompilerProfile::ALL {
        let mut config = Config::with_profile(profile);
        config.preprocessor.defines.clear();
        let compilation = toucan::parse_source(
            Path::new("packed.h"),
            "enum __attribute__((packed)) P { VALUE = (_Bool)1 };\n#define PACKED_ALIAS VALUE\n",
            &config,
        )
        .unwrap();
        let value = toucan::semantic::evaluate_integer(compilation.unit(), "VALUE").unwrap();
        assert_eq!((value.bits, value.signed, value.rank), (32, true, 3));
        let (source, report) = compilation.bindings(&BindingOptions::default()).unwrap();
        assert!(source.contains("pub const PACKED_ALIAS: ::core::primitive::i32 = 1;"));
        assert!(!source.contains("pub const VALUE: ::core::primitive::bool"));
        assert!(report.skipped_macros.is_empty());
    }
}

#[test]
#[ignore = "requires native GCC, Clang, Python and Rust; run with --include-ignored"]
fn generated_boolean_consumers_match_independent_c_probes() {
    use std::process::Command;
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let directory = tempfile::tempdir().unwrap();
    let compilation = parse(target);
    let scripts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts");
    let generator = r#"import sys,json
from pathlib import Path
sys.path.insert(0,sys.argv[1])
import verify_corpus as v
p=Path(sys.argv[2]);v.PROBES['bool_macros']=[]
c,r,coverage=v.generate_probes('bool_macros',p/'api.h',(p/'bindings.rs').read_text(),json.loads((p/'report.json').read_text()),run_ffi=False)
(p/'probe.c').write_text(c);(p/'probe.rs').write_text(r)
assert coverage['integer_constants']==20,coverage
"#;
    std::fs::write(directory.path().join("api.h"), HEADER).unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let version = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        version.status.success() && !String::from_utf8_lossy(&version.stdout).contains("clang")
    );
    for (mut options, booleans) in cases() {
        options.rust_target = RustTarget::RUST_1_64;
        let (source, report) = compilation.bindings(&options).unwrap();
        std::fs::write(directory.path().join("bindings.rs"), source).unwrap();
        std::fs::write(
            directory.path().join("report.json"),
            serde_json::to_vec(&report).unwrap(),
        )
        .unwrap();
        let result = Command::new("python3")
            .args(["-c", generator])
            .arg(&scripts)
            .arg(directory.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let mut rust = std::fs::read_to_string(directory.path().join("probe.rs")).unwrap();
        for name in BOOL_NAMES {
            let rust_name = if *name == "type" { "r#type" } else { name };
            let ty = if booleans.contains(name) {
                "bool"
            } else {
                "u32"
            };
            rust.push_str(&format!("const _: {ty} = b::{rust_name};\n"));
        }
        std::fs::write(directory.path().join("probe.rs"), rust).unwrap();
        let compiler = std::env::var("TOUCAN_TEST_RUSTC").unwrap_or_else(|_| "rustc".into());
        let result = Command::new(compiler)
            .current_dir(directory.path())
            .args(["--edition=2021", "probe.rs", "-o", "rust-probe"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let expected = Command::new(directory.path().join("rust-probe"))
            .output()
            .unwrap();
        assert!(expected.status.success());
        for cc in [&gcc, "clang"] {
            let result = Command::new(cc)
                .current_dir(directory.path())
                .args(["-std=gnu11", "probe.c", "-o", "c-probe"])
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let actual = Command::new(directory.path().join("c-probe"))
                .output()
                .unwrap();
            assert!(actual.status.success());
            assert_eq!(actual.stdout, expected.stdout, "{cc}");
        }
    }
}

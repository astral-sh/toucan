use toucan_bindings::{BindingSelection, Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

const SOURCE: &str = "\
typedef enum { POINT_COMPRESSED=2, POINT_UNCOMPRESSED=4, POINT_HYBRID=6 } point_conversion_form_t;
typedef point_conversion_form_t OtherAlias;
typedef enum Named { NAMED_ZERO=0, NAMED_ONE=1 } NamedAlias;
typedef enum { FIRST_ZERO=0, FIRST_ONE=1 } First, Second;
enum { ANON_ZERO=0, ANON_ONE=1 };
enum Bits { BIT_ZERO=0, BIT_ONE=1 };
struct Flags { enum Bits value:1; };
";

#[test]
fn selection_uses_enum_tags_and_first_anonymous_typedefs() {
    for target in Target::ALL {
        let unit = analyze(SOURCE, target).unwrap();
        for (pattern, expected) in [
            ("point_conversion_form_t", Some("point_conversion_form_t")),
            ("point_conversion*", Some("point_conversion_form_t")),
            ("OtherAlias", None),
            ("Named", Some("Named")),
            ("NamedAlias", None),
            ("First", Some("First")),
            ("Second", None),
            ("POINT_COMPRESSED", None),
            ("FIRST_ZERO", None),
        ] {
            let bindings = generate(
                &unit,
                &Options {
                    rustified_enum_patterns: vec![pattern.into()],
                    ..Options::default()
                },
            )
            .unwrap();
            let enums = bindings
                .source
                .lines()
                .filter_map(|line| line.strip_prefix("pub enum "))
                .map(|name| name.strip_suffix(" {").unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                enums,
                expected.into_iter().collect::<Vec<_>>(),
                "{target:?}: {pattern}"
            );
            if expected == Some("point_conversion_form_t") {
                assert!(bindings.source.contains("POINT_COMPRESSED = 2,"));
                assert!(
                    bindings
                        .source
                        .contains("pub type OtherAlias = point_conversion_form_t;")
                );
                assert!(!bindings.source.contains("pub enum Bits"));
            }
        }
    }
}

#[test]
fn anonymous_constants_can_select_an_enum_without_adding_unselected_roots() {
    let unit = analyze(SOURCE, Target::X86_64UnknownLinuxGnu).unwrap();
    let options = Options {
        rustified_enum_patterns: vec!["ANON_ZERO".into()],
        allowlist: vec!["ANON_ZERO".into()],
        ..Options::default()
    };
    let source = generate(&unit, &options).unwrap().source;
    assert_eq!(source.matches("pub enum ").count(), 1);
    assert!(source.contains("ANON_ONE = 1,"));
    assert!(!source.contains("pub const ANON_ONE"));
    assert!(!source.contains("pub struct Flags"));
    let selected = Options {
        allowlist: Vec::new(),
        selection: Some(Box::new(BindingSelection {
            constants: ["ANON_ZERO".into()].into_iter().collect(),
            ..BindingSelection::default()
        })),
        ..options.clone()
    };
    assert_eq!(generate(&unit, &selected).unwrap().source, source);
    let empty = Options {
        selection: Some(Box::default()),
        ..selected
    };
    assert!(
        !generate(&unit, &empty)
            .unwrap()
            .source
            .contains("pub enum ")
    );
    let options = Options {
        allowlist: vec!["NOT_PRESENT".into()],
        ..options
    };
    assert!(
        !generate(&unit, &options)
            .unwrap()
            .source
            .contains("pub enum ")
    );
}

#[test]
fn only_selected_enum_bitfields_require_the_integer_representation() {
    let unit = analyze(SOURCE, Target::X86_64UnknownLinuxGnu).unwrap();
    let options = Options {
        rustified_enum_patterns: vec!["point_conversion_form_t".into()],
        ..Options::default()
    };
    generate(&unit, &options).unwrap();
    let options = Options {
        rustified_enum_patterns: vec!["Bits".into()],
        ..options
    };
    assert!(
        generate(&unit, &options)
            .unwrap_err()
            .to_string()
            .contains("enum bitfields require the integer enum representation")
    );
}

#[test]
#[ignore = "requires native C compilers and rustc; run with --include-ignored"]
fn selected_point_conversion_values_roundtrip_through_c() {
    use std::process::Command;
    use toucan_bindings::RustTarget;
    use toucan_test_support::{compiler_acceptance, link_c_object};

    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    // The anonymous typedef and discriminants are the public ec.h shape used
    // by AWS-LC's rust_wrapper.h builder.
    let header = "typedef enum { POINT_CONVERSION_COMPRESSED=2, POINT_CONVERSION_UNCOMPRESSED=4, POINT_CONVERSION_HYBRID=6 } point_conversion_form_t;\nenum Other { OTHER_ZERO=0, OTHER_ONE=1 };\npoint_conversion_form_t echo_form(point_conversion_form_t value);\nenum Other echo_other(enum Other value);\n";
    let unit = analyze(header, target).unwrap();
    let source = generate(
        &unit,
        &Options {
            rustified_enum_patterns: vec!["point_conversion_form_t".into()],
            rust_target: RustTarget::RUST_1_64,
            ..Options::default()
        },
    )
    .unwrap()
    .source;
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("bindings.rs"), source).unwrap();
    std::fs::write(directory.path().join("probe.c"), format!("{header}\npoint_conversion_form_t echo_form(point_conversion_form_t value) {{ return value; }}\nenum Other echo_other(enum Other value) {{ return value; }}\n")).unwrap();
    std::fs::write(directory.path().join("main.rs"), r#"
#![allow(dead_code, non_camel_case_types, non_upper_case_globals)]
include!("bindings.rs");
fn main() {
    use point_conversion_form_t::*;
    for form in [POINT_CONVERSION_COMPRESSED, POINT_CONVERSION_UNCOMPRESSED, POINT_CONVERSION_HYBRID] {
        assert_eq!(unsafe { echo_form(form) }, form);
    }
    let other: Other = 1;
    assert_eq!(unsafe { echo_other(other) }, other);
}
"#).unwrap();
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        "clang".into(),
    ] {
        let object = directory.path().join("probe.o");
        let output = Command::new(&compiler)
            .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c"])
            .arg(directory.path().join("probe.c"))
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        assert_eq!(
            compiler_acceptance(&output),
            Ok(true),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let executable = directory.path().join("probe");
        let mut command = Command::new("rustc");
        if let Some(toolchain) = std::env::var_os("TOUCAN_TEST_RUST_TOOLCHAIN") {
            command.arg(format!("+{}", toolchain.to_string_lossy()));
        }
        command
            .args(["--edition=2021", "-Dwarnings"])
            .arg(directory.path().join("main.rs"))
            .arg("-o")
            .arg(&executable);
        link_c_object(&mut command, &object);
        let output = command.output().unwrap();
        assert_eq!(
            compiler_acceptance(&output),
            Ok(true),
            "rustc: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(Command::new(executable).status().unwrap().success());
    }
}

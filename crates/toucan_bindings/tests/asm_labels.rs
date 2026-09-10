use toucan_bindings::{Options, generate};
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

const HEADER: &str = r#"
int call(void) __asm__("native_call");
extern int value __asm__("native_value");
int same(void) __asm__("same");
int ordinary(void);
int call(void);
extern int value;
"#;

#[test]
fn assembler_labels_preserve_exact_symbols_on_darwin() {
    for target in Target::ALL {
        let analysis = analyze_with_options(
            HEADER,
            target,
            &AnalysisOptions {
                retain_object_values: true,
                ..Default::default()
            },
        )
        .unwrap();
        let output = generate(
            analysis.unit(),
            &Options {
                additional_objects: [(
                    "value_alias".into(),
                    analysis.object_values().unwrap().entries()[0].clone(),
                )]
                .into(),
                ..Default::default()
            },
        )
        .unwrap()
        .source;
        let marker = if matches!(
            target,
            Target::X86_64AppleDarwin | Target::Aarch64AppleDarwin
        ) {
            "\\u{1}"
        } else {
            ""
        };
        for symbol in ["native_call", "native_value"] {
            assert!(
                output.contains(&format!("#[link_name = \"{marker}{symbol}\"]")),
                "{target}: {output}"
            );
        }
        if !marker.is_empty() {
            assert!(output.contains("#[link_name = \"\\u{1}same\"]"));
        }
        assert!(output.contains(&format!(
            "#[link_name = \"{marker}native_value\"]\n    pub static mut value_alias:"
        )));
        assert!(!output.contains("#[link_name = \"\\u{1}ordinary\"]"));
    }
}

#[test]
fn explicit_link_overrides_and_prefixes_keep_their_existing_policy() {
    let unit = analyze(HEADER, Target::Aarch64AppleDarwin).unwrap();
    let output = generate(
        &unit,
        &Options {
            link_name_prefix: Some("prefix_".into()),
            link_name_overrides: [("value".into(), "custom_value".into())].into(),
            ..Default::default()
        },
    )
    .unwrap()
    .source;
    assert!(output.contains("#[link_name = \"prefix_call\"]"));
    assert!(output.contains("#[link_name = \"custom_value\"]"));
    assert!(!output.contains("\\u{1}"));
}

#[test]
fn builtin_aliases_distinguish_implicit_names_from_explicit_labels() {
    for target in [Target::X86_64AppleDarwin, Target::Aarch64AppleDarwin] {
        for (labels, expected) in [
            (["", "", ""], "malloc"),
            ([" __asm__(\"malloc\")", "", ""], "\\u{1}malloc"),
            (["", " __asm__(\"other_malloc\")", ""], "\\u{1}other_malloc"),
        ] {
            let unit = analyze(
                &labels
                    .map(|label| format!("void *__builtin_malloc(unsigned long){label};"))
                    .join("\n"),
                target,
            )
            .unwrap();
            let output = generate(&unit, &Options::default()).unwrap().source;
            assert!(
                output.contains(&format!("#[link_name = \"{expected}\"]")),
                "{target}: {output}"
            );
        }
    }
}

#[test]
#[ignore = "requires a native C compiler and rustc; run with --include-ignored"]
fn generated_assembler_labels_link_to_native_functions_and_objects() {
    use std::process::Command;

    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Target::X86_64UnknownLinuxGnu,
        ("linux", "aarch64") => Target::Aarch64UnknownLinuxGnu,
        ("macos", "x86_64") => Target::X86_64AppleDarwin,
        ("macos", "aarch64") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let directory = tempfile::tempdir().unwrap();
    let unit = analyze(HEADER, target).unwrap();
    let output = generate(&unit, &Options::default()).unwrap();
    std::fs::write(directory.path().join("bindings.rs"), output.source).unwrap();
    std::fs::write(
        directory.path().join("native.c"),
        format!("{HEADER}\nint value=7; int call(void){{return value;}} int same(void){{return 11;}} int ordinary(void){{return 13;}}"),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("consumer.rs"),
        r#"
include!("bindings.rs");
fn main() { unsafe {
    assert_eq!(call(), 7);
    value = 17;
    assert_eq!(call(), 17);
    assert_eq!(same(), 11);
    assert_eq!(ordinary(), 13);
} }
"#,
    )
    .unwrap();
    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let output = Command::new(compiler)
        .current_dir(directory.path())
        .args(["-c", "native.c", "-o", "native.o"])
        .output()
        .unwrap();
    assert_eq!(
        toucan_test_support::compiler_acceptance(&output),
        Ok(true),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new("rustc")
        .current_dir(directory.path())
        .args([
            "--edition=2024",
            "consumer.rs",
            "-C",
            "link-arg=native.o",
            "-o",
            "consumer",
        ])
        .output()
        .unwrap();
    assert_eq!(
        toucan_test_support::compiler_acceptance(&output),
        Ok(true),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(directory.path().join("consumer"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

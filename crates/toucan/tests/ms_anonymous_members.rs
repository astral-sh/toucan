use std::{path::Path, process::Command};

use toucan::{BindingOptions, Config, RustTarget, Target};

const HEADER: &str = r#"
typedef struct Named {int value;} Alias;
typedef struct {int item;} Anonymous;
typedef union {int bits; float number;} Choice;
struct FromAlias {Alias; int field;};
struct FromTag {struct Named; int field;};
struct FromAnonymous {Anonymous; int field;};
struct FromUnion {Choice; int field;};
"#;

fn bindings(target: Target) -> String {
    let compilation =
        toucan::parse_source(Path::new("members.h"), HEADER, &Config::new(target)).unwrap();
    let (bindings, _) = compilation
        .bindings(&BindingOptions {
            allowlist: ["Named", "Alias", "Anonymous", "Choice", "From*"]
                .map(str::to_owned)
                .to_vec(),
            rust_target: RustTarget::RUST_1_64,
            ..Default::default()
        })
        .unwrap();
    bindings
}

#[test]
fn windows_anonymous_fields_emit_named_record_storage() {
    let windows = bindings(Target::X86_64PcWindowsMsvc);
    assert_eq!(windows.matches("pub __anonymous_0:").count(), 4);
    assert!(!bindings(Target::X86_64UnknownLinuxGnu).contains("pub __anonymous_0:"));
}

#[test]
#[ignore = "requires native Clang and Rust; supports TOUCAN_TEST_RUST_TOOLCHAIN"]
fn generated_layouts_match_native_and_windows_cross_target_c_assertions() {
    let toolchain = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN").ok();
    let mut rustc = Command::new("rustc");
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
    let mut targets = vec![Target::X86_64UnknownLinuxGnu, Target::X86_64PcWindowsMsvc];
    if !targets.contains(&host) {
        targets.push(host);
    }
    for target in targets {
        let windows = target == Target::X86_64PcWindowsMsvc;
        let directory = tempfile::tempdir().unwrap();
        let c = directory.path().join("members.c");
        let mut source = HEADER.to_owned();
        for name in ["FromAlias", "FromTag", "FromAnonymous", "FromUnion"] {
            source.push_str(&format!(
                "_Static_assert(sizeof(struct {name})=={},\"size\");\n\
                 _Static_assert(__alignof__(struct {name})==4,\"alignment\");\n\
                 _Static_assert(__builtin_offsetof(struct {name},field)=={},\"offset\");\n",
                if windows { 8 } else { 4 },
                if windows { 4 } else { 0 },
            ));
        }
        std::fs::write(&c, source).unwrap();
        let output = Command::new("clang")
            .arg(format!("--target={}", target.triple()))
            .args(["-std=gnu11", "-fsyntax-only"])
            .arg(&c)
            .output()
            .unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(true),
            "{output:?}"
        );

        let rust = directory.path().join("members.rs");
        let executable = directory.path().join("members-test");
        let mut generated = bindings(target);
        if target != host {
            // The target guard deliberately forbids executing cross-target
            // bindings on this host. Their C layouts are asserted above.
            assert!(generated.contains("these C bindings were generated for a different target"));
            continue;
        }
        if windows {
            generated.push_str(
                r#"
#[test]
fn stored_anonymous_values() {
    let first = FromAlias {__anonymous_0: Named {value: 7}, field: 9};
    let second = FromAnonymous {__anonymous_0: Anonymous {item: 11}, field: 13};
    let third = FromUnion {__anonymous_0: Choice {bits: 17}, field: 19};
    assert_eq!((first.__anonymous_0.value, first.field), (7,9));
    assert_eq!((second.__anonymous_0.item, second.field), (11,13));
    assert_eq!((unsafe {third.__anonymous_0.bits}, third.field), (17,19));
}
"#,
            );
        }
        std::fs::write(&rust, generated).unwrap();
        let mut rustc = Command::new("rustc");
        if let Some(toolchain) = &toolchain {
            rustc.arg(format!("+{toolchain}"));
        }
        let output = rustc
            .args(["--edition=2021", "--test", "-Dwarnings"])
            .arg(&rust)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{target:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(&executable).output().unwrap();
        assert!(
            output.status.success(),
            "{target:?}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

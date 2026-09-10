use std::path::Path;
use std::process::Command;

use toucan::{BindingOptions, CompilerProfile, Config, Target, parse_source};

const HEADER: &str = r#"
#define CAT(a,b) a##b
#define STRING(a) #a
#define VALUE$ 7
#define SPELL$ STRING(price$usd)
#define TYPE_SIZE$ sizeof(__int128$)
#if !defined(VALUE$) || CAT(VALUE,$) != 7
#error dollar macro expansion failed
#endif
typedef int __int128$;
struct S$ { int field$; int __toucan_c_6669656c6424; unsigned bits$:3; int __toucan_c_7365745f6269747324; };
enum E$ { ITEM$ = 9 };
extern int price$usd;
int fn$(__int128$ value);
int __toucan_c_666e24(int value);
"#;

#[test]
fn dollar_names_survive_preprocessing_analysis_and_binding_generation() {
    for profile in CompilerProfile::ALL {
        let compilation = parse_source(
            Path::new("dollar.h"),
            HEADER,
            &Config::with_profile(profile),
        )
        .unwrap();
        let (source, report) = compilation.bindings(&BindingOptions::default()).unwrap();
        assert!(source.contains("pub fn __toucan_c_666e24_("), "{source}");
        assert!(source.contains("#[link_name = \"fn$\"]"));
        assert!(source.contains("#[link_name = \"price$usd\"]"));
        assert!(source.contains("pub __toucan_c_6669656c6424_:"));
        assert!(source.contains("fn __toucan_c_7365745f6269747324_("));
        assert!(source.contains("pub const __toucan_c_56414c554524:"));
        assert!(source.contains("pub const __toucan_c_545950455f53495a4524:"));
        assert!(source.contains("price$usd"));
        assert!(
            report
                .skipped_macros
                .iter()
                .all(|entry| entry.name != "TYPE_SIZE$")
        );
    }
}

#[test]
#[ignore = "requires native GCC, Clang, and rustc"]
fn generated_dollar_names_link_to_native_symbols_without_collisions() {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Target::X86_64UnknownLinuxGnu,
        ("linux", "aarch64") => Target::Aarch64UnknownLinuxGnu,
        ("macos", "x86_64") => Target::X86_64AppleDarwin,
        ("macos", "aarch64") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let compilation = parse_source(Path::new("dollar.h"), HEADER, &Config::new(target)).unwrap();
    let (bindings, _) = compilation.bindings(&BindingOptions::default()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("bindings.rs"), bindings).unwrap();
    std::fs::write(directory.path().join("native.c"), format!("{HEADER}\nint price$usd=7; int fn$(__int128$ value){{return value+price$usd;}} int __toucan_c_666e24(int value){{return value+100;}}\n")).unwrap();
    std::fs::write(
        directory.path().join("consumer.rs"),
        r#"
include!("bindings.rs");
fn main() { unsafe {
    assert_eq!(__toucan_c_666e24_(7), 14);
    __toucan_c_707269636524757364 = 9;
    assert_eq!(__toucan_c_666e24_(10), 19);
    assert_eq!(__toucan_c_666e24(10), 110);
    assert_eq!(__toucan_c_56414c554524, 7);
    assert_eq!(__toucan_c_545950455f53495a4524, 4);
    let mut value: __toucan_c_5324 = core::mem::zeroed();
    value.__toucan_c_6669656c6424_ = 17;
    value.__toucan_c_6669656c6424 = 23;
    value.__toucan_c_7365745f6269747324_(5);
    assert_eq!(value.__toucan_c_6269747324(), 5);
    assert_eq!(value.__toucan_c_6669656c6424_, 17);
    assert_eq!(value.__toucan_c_6669656c6424, 23);
} }
"#,
    )
    .unwrap();
    for compiler in ["gcc", "clang"] {
        let output = Command::new(compiler)
            .current_dir(directory.path())
            .args(["-std=c11", "-Werror", "-c", "native.c", "-o", "native.o"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{compiler}: {}",
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
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(directory.path().join("consumer"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
}

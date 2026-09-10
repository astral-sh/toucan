use std::process::Command;

use toucan_bindings::{Options, generate};
use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};
use toucan_test_support::{compiler_acceptance, link_c_object};

const STORAGE: &str = r#"
union Empty {};
union Aligned {} __attribute__((aligned(8)));
struct Holder { char head; union Empty empty; char tail; };
union Nested { union Empty empty; };
void inspect(const union Empty *, const union Aligned *, const struct Holder *);
"#;

#[test]
fn empty_unions_keep_target_storage_and_pointer_dependencies() {
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile(STORAGE, profile, &AnalysisOptions::default()).unwrap();
        let source = generate(
            analysis.unit(),
            &Options {
                allowlist: vec!["inspect".into(), "Nested".into()],
                ..Options::default()
            },
        )
        .unwrap()
        .source;
        let size = if profile.target().is_windows() { 4 } else { 0 };
        let aligned_size = if profile.target().is_windows() { 8 } else { 0 };
        for (name, bytes) in [("Empty", size), ("Aligned", aligned_size)] {
            assert!(
                source.contains(&format!(
                    "pub union {name} {{\n    _private: ::core::mem::MaybeUninit<[::core::primitive::u8; {bytes}]>,\n}}"
                )),
                "{profile:?}: {source}"
            );
        }
        assert!(source.contains("pub empty: Empty,"));
        assert!(source.contains("pub union Nested {\n    pub empty: Empty,\n}"));
    }
}

#[test]
fn empty_union_call_boundaries_follow_the_target_abi() {
    for profile in CompilerProfile::ALL {
        for declaration in [
            "int consume(int, union Empty, int);",
            "union Aligned produce(void);",
            "void nested(struct Holder);",
            "union Nested produce_nested(void);",
            "typedef int (*Callback)(union Empty);",
        ] {
            let input = format!("{STORAGE}\n{declaration}");
            let analysis =
                analyze_with_profile(&input, profile, &AnalysisOptions::default()).unwrap();
            let result = generate(analysis.unit(), &Options::default());
            if matches!(
                profile.target(),
                Target::I686UnknownLinuxGnu | Target::Aarch64PcWindowsMsvc
            ) {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("empty unions cannot cross an FFI call by value on"),
                    "{input}"
                );
            } else {
                result.unwrap();
            }
        }
    }
}

#[test]
#[ignore = "requires native GCC, Clang, and rustc; run with --include-ignored"]
fn empty_union_storage_calls_and_callbacks_match_c() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let header = format!(
        "{STORAGE}\n\
         int consume(int, union Empty, int);\n\
         union Empty produce(void);\n\
         int consume_aligned(int, union Aligned, int);\n\
         union Aligned produce_aligned(void);\n\
         typedef int (*Callback)(int, union Empty, int);\n\
         int invoke(Callback);"
    );
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("native.c"),
        format!(
            "{header}\n\
             _Static_assert(sizeof(union Empty)==0, \"size\");\n\
             _Static_assert(_Alignof(union Empty)==1, \"alignment\");\n\
             _Static_assert(sizeof(union Aligned)==0, \"aligned size\");\n\
             _Static_assert(_Alignof(union Aligned)==8, \"aligned alignment\");\n\
             void inspect(const union Empty *e, const union Aligned *a, const struct Holder *h) {{\
                 if (h->head!=2 || h->tail!=3 || sizeof(*h)!=2) __builtin_trap();\
             }}\n\
             int consume(int a, union Empty e, int b) {{ return a+b; }}\n\
             union Empty produce(void) {{ return (union Empty){{}}; }}\n\
             int consume_aligned(int a, union Aligned e, int b) {{ return a+b; }}\n\
             union Aligned produce_aligned(void) {{ return (union Aligned){{}}; }}\n\
             int invoke(Callback cb) {{ return cb(5, produce(), 8); }}"
        ),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("consumer.rs"),
        r#"
#![allow(dead_code)]
include!("bindings.rs");
unsafe extern "C" fn callback(a: i32, _: Empty, b: i32) -> i32 { a + b }
fn main() { unsafe {
    let empty = produce();
    let aligned = produce_aligned();
    let holder = Holder { head: 2, empty, tail: 3 };
    inspect(&empty, &aligned, &holder);
    assert_eq!(consume(2, empty, 3), 5);
    assert_eq!(consume_aligned(3, aligned, 4), 7);
    assert_eq!(invoke(Some(callback)), 13);
} }
"#,
    )
    .unwrap();
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let Ok(profile) = CompilerProfile::new(target, compiler) else {
            continue;
        };
        let analysis = analyze_with_profile(&header, profile, &AnalysisOptions::default()).unwrap();
        std::fs::write(
            directory.path().join("bindings.rs"),
            generate(analysis.unit(), &Options::default())
                .unwrap()
                .source,
        )
        .unwrap();
        let output = Command::new(match compiler {
            Compiler::Gnu => "gcc",
            Compiler::Clang => "clang",
        })
        .current_dir(directory.path())
        .args(["-O2", "-c", "native.c", "-o", "native.o"])
        .output()
        .unwrap();
        assert_eq!(compiler_acceptance(&output), Ok(true), "{output:?}");
        let mut rustc = Command::new("rustc");
        rustc.current_dir(directory.path()).args([
            "--edition=2024",
            "-O",
            "consumer.rs",
            "-o",
            "consumer",
        ]);
        link_c_object(&mut rustc, &directory.path().join("native.o"));
        let output = rustc.output().unwrap();
        assert_eq!(compiler_acceptance(&output), Ok(true), "{output:?}");
        let output = Command::new(directory.path().join("consumer"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{profile:?}: {output:?}");
    }
}

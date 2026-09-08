use std::path::Path;

use toucan::{AnalysisOptions, Compiler, CompilerProfile, Config, semantic::analyze_with_profile};

const CASES: &[(&str, bool)] = &[
    ("__attribute__((nodebug)) int f(void){return 1;}", true),
    ("int f(void) __attribute__((__nodebug__));", true),
    ("int v __attribute__((nodebug));", true),
    (
        "void f(void){int x __attribute__((nodebug))=1;(void)x;}",
        true,
    ),
    ("typedef int T __attribute__((nodebug)); T v;", true),
    ("typedef int (*F)(void) __attribute__((nodebug));", true),
    ("int f(int x __attribute__((nodebug(1)))){return x;}", true),
    ("struct S {int x __attribute__((nodebug(1)));};", true),
    ("struct __attribute__((nodebug(1))) S {int x;};", true),
    ("enum __attribute__((nodebug)) E{A};", true),
    ("enum E{A __attribute__((nodebug(1)))};", true),
    (
        "int f(void){return sizeof(int __attribute__((nodebug(1))));}",
        true,
    ),
    ("int f(void) __attribute__((nodebug(1)));", false),
    ("int v __attribute__((nodebug(unknown)));", false),
    ("void f(void){int v __attribute__((nodebug(1)));}", false),
    ("typedef int T __attribute__((nodebug(1)));", false),
    ("int *__attribute__((nodebug(1))) p;", false),
    ("void f(int *__attribute__((nodebug(1))) p);", true),
    ("void f(int (*p)(void) __attribute__((nodebug(1))));", false),
    (
        "struct S {int (*p)(void) __attribute__((nodebug(1)));};",
        false,
    ),
    (
        "int f(void){return sizeof(int (* __attribute__((nodebug(1))))(void));}",
        true,
    ),
];

#[test]
fn debug_attributes_preserve_compiler_constraints_and_semantic_parity() {
    for profile in CompilerProfile::ALL {
        for &(source, clang_accepts) in CASES {
            let plain = analyze_with_profile(source, profile, &AnalysisOptions::default());
            let retained = analyze_with_profile(
                source,
                profile,
                &AnalysisOptions {
                    retain_code: true,
                    ..Default::default()
                },
            );
            assert_eq!(
                plain.is_ok(),
                profile.compiler() == Compiler::Gnu || clang_accepts,
                "{profile:?}: {source}: {plain:?}"
            );
            match (plain, retained) {
                (Ok(plain), Ok(retained)) => assert_eq!(
                    format!("{:?}", plain.unit()),
                    format!("{:?}", retained.unit())
                ),
                (Err(plain), Err(retained)) => {
                    assert_eq!(
                        (plain.offset, &plain.message),
                        (retained.offset, &retained.message)
                    );
                    assert!(plain.message.contains("nodebug takes no arguments"));
                }
                (plain, retained) => panic!("{source}: {plain:?} {retained:?}"),
            }
        }
    }
}

#[test]
fn debug_attributes_do_not_change_generated_bindings() {
    let source = "typedef int T __attribute__((nodebug)); extern T value __attribute__((nodebug)); int f(T) __attribute__((__nodebug__));";
    let unannotated = source
        .replace(" __attribute__((nodebug))", "")
        .replace(" __attribute__((__nodebug__))", "");
    for profile in CompilerProfile::ALL {
        let config = Config::with_profile(profile);
        let compile = |source: &str| {
            toucan::parse_source(Path::new("api.h"), source, &config)
                .unwrap()
                .bindings(&Default::default())
                .unwrap()
                .0
        };
        assert_eq!(compile(source), compile(&unannotated));
    }
}

#[test]
#[ignore = "requires Clang's five target backends and native GNU GCC on Linux"]
fn debug_attribute_subjects_match_compiler_diagnostics() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nodebug.c");
    for profile in CompilerProfile::ALL {
        let mut command = if profile.compiler() == Compiler::Clang {
            let mut command = std::process::Command::new("clang");
            command.args(["-target", profile.target().triple()]);
            command
        } else if cfg!(target_os = "linux")
            && profile.target().triple() == format!("{}-unknown-linux-gnu", std::env::consts::ARCH)
        {
            std::process::Command::new(std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()))
        } else {
            continue;
        };
        command.args(["-std=gnu11", "-fsyntax-only"]).arg(&path);
        for &(source, clang_accepts) in CASES {
            std::fs::write(&path, source).unwrap();
            let result = command.output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&result),
                Ok(profile.compiler() == Compiler::Gnu || clang_accepts),
                "{profile:?} {source}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

#[test]
#[ignore = "requires Clang with LLVM IR and debug information support"]
fn clang_nodebug_suppresses_debug_information_without_removing_the_definition() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("debug.c");
    let ir = directory.path().join("debug.ll");
    std::fs::write(&path, "int visible_value=1; __attribute__((nodebug)) int hidden_value=2; int visible(void){return visible_value;} __attribute__((nodebug)) int hidden(void){return hidden_value;}").unwrap();
    let result = std::process::Command::new("clang")
        .args(["-std=gnu11", "-g", "-O0", "-S", "-emit-llvm"])
        .arg(&path)
        .arg("-o")
        .arg(&ir)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let ir = std::fs::read_to_string(ir).unwrap();
    assert!(ir.contains("@hidden()"));
    assert!(ir.contains("@hidden_value ="));
    assert!(ir.contains("!DISubprogram(name: \"visible\""));
    assert!(ir.contains("!DIGlobalVariable(name: \"visible_value\""));
    assert!(!ir.contains("!DISubprogram(name: \"hidden\""));
    assert!(!ir.contains("!DIGlobalVariable(name: \"hidden_value\""));
}

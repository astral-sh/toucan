use std::process::Command;

use toucan_bindings::{Options, generate};
use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};
use toucan_test_support::{compiler_acceptance, link_c_object};

#[test]
fn flexible_and_zero_length_arrays_require_pointers_at_call_boundaries() {
    for target in Target::ALL {
        for compiler in [Compiler::Gnu, Compiler::Clang] {
            let Ok(profile) = CompilerProfile::new(target, compiler) else {
                continue;
            };
            for tail in ["float tail[]", "char tail[0]"] {
                let records = format!(
                    "struct S {{ float value; {tail}; }}; typedef struct S Alias;\n\
                     struct Outer {{ struct S records[2]; }};\n\
                     union U {{ struct S record; float value; }};"
                );
                for declaration in [
                    "void call(struct S);",
                    "struct S call(void);",
                    "Alias call(Alias);",
                    "void call(struct Outer);",
                    "union U call(void);",
                    "typedef void (*Callback)(struct S);",
                    "typedef struct S (*Callback)(void);",
                    "struct Callbacks { void (*call)(struct S); };",
                ] {
                    let input = format!("{records}\n{declaration}");
                    let analysis =
                        analyze_with_profile(&input, profile, &AnalysisOptions::default()).unwrap();
                    let error = generate(analysis.unit(), &Options::default()).unwrap_err();
                    assert!(
                        error.to_string().contains(
                            "records containing flexible or zero-length arrays cannot cross an FFI call by value"
                        ),
                        "{profile:?}: {input}: {error}"
                    );
                }
            }
        }
    }
}

const POINTER_HEADER: &str = r#"
struct Flexible { float value; float tail[]; };
struct Zero { float value; char tail[0]; };
extern struct Flexible flexible_storage;
extern struct Zero zero_storage;
struct Indirect { struct Flexible *flexible; struct Zero *zero; };
struct Fixed { float values[2]; };
void update(struct Flexible *, struct Zero *);
typedef void (*Callback)(struct Flexible *, struct Zero *);
void invoke(Callback, struct Flexible *, struct Zero *);
struct Indirect indirect(struct Indirect);
struct Fixed fixed(struct Fixed);
void arrays(float values[], char empty[0]);
"#;

#[test]
fn zero_length_array_storage_and_indirect_calls_remain_supported() {
    for target in Target::ALL {
        for compiler in [Compiler::Gnu, Compiler::Clang] {
            let Ok(profile) = CompilerProfile::new(target, compiler) else {
                continue;
            };
            let analysis =
                analyze_with_profile(POINTER_HEADER, profile, &AnalysisOptions::default()).unwrap();
            let source = generate(analysis.unit(), &Options::default())
                .unwrap()
                .source;
            assert!(source.contains("pub tail: [::core::primitive::f32; 0]"));
            assert!(source.contains("pub tail: [::core::ffi::c_char; 0]"));
            assert!(source.contains("pub static mut flexible_storage: Flexible;"));
            assert!(source.contains("pub static mut zero_storage: Zero;"));
            assert!(source.contains("pub fn indirect(arg0: Indirect) -> Indirect;"));
            assert!(source.contains("pub fn fixed(arg0: Fixed) -> Fixed;"));
            assert!(source.contains(
                "pub fn arrays(arg0: *mut ::core::primitive::f32, arg1: *mut ::core::ffi::c_char);"
            ));
        }
    }
}

#[test]
#[ignore = "requires native GCC, Clang, and rustc; run with --include-ignored"]
fn pointer_calls_and_callbacks_match_native_compilers() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("native.c"),
        format!(
            "{POINTER_HEADER}\n\
             void update(struct Flexible *f, struct Zero *z) {{ f->value += 1; z->value += 2; }}\n\
             void invoke(Callback cb, struct Flexible *f, struct Zero *z) {{ cb(f, z); }}\n\
             struct Indirect indirect(struct Indirect value) {{ return value; }}\n\
             struct Fixed fixed(struct Fixed value) {{ value.values[0] += 3; return value; }}"
        ),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("consumer.rs"),
        r#"
#![allow(dead_code)]
include!("bindings.rs");
unsafe extern "C" fn callback(f: *mut Flexible, z: *mut Zero) {
    unsafe { (*f).value += 4.0; (*z).value += 8.0; }
}
fn main() { unsafe {
    let mut f = Flexible { value: 2.5, tail: [] };
    let mut z = Zero { value: 3.5, tail: [] };
    update(&mut f, &mut z);
    invoke(Some(callback), &mut f, &mut z);
    assert_eq!(f.value, 7.5);
    assert_eq!(z.value, 13.5);
    let out = indirect(Indirect { flexible: &mut f, zero: &mut z });
    assert_eq!(out.flexible, &mut f as *mut _);
    assert_eq!(out.zero, &mut z as *mut _);
    assert_eq!(fixed(Fixed { values: [1.0, 2.0] }).values, [4.0, 2.0]);
} }
"#,
    )
    .unwrap();
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let Ok(profile) = CompilerProfile::new(target, compiler) else {
            continue;
        };
        let analysis =
            analyze_with_profile(POINTER_HEADER, profile, &AnalysisOptions::default()).unwrap();
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

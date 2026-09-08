use std::path::Path;

use toucan::{BindingOptions, Compiler, CompilerProfile, Config, RustTarget, Target};

const HEADER: &str = r#"
typedef void Opaque __attribute__((aligned(32)));
typedef int Callback(int) __attribute__((aligned(32)));
struct Api { Opaque *context; Callback *callback; };
Opaque *identity(Opaque *);
int apply(struct Api, int);
Callback *get_callback(void);
unsigned long long c_void_size(void);
unsigned long long c_function_size(void);
#define SIZE_VOID sizeof(Opaque)
#define SIZE_FUNCTION sizeof(Callback)
#define ALIGN_VOID __alignof__(Opaque)
#define ALIGN_FUNCTION __alignof__(Callback)
"#;

fn generate(profile: CompilerProfile) -> String {
    let compilation =
        toucan::parse_source(Path::new("api.h"), HEADER, &Config::with_profile(profile)).unwrap();
    let (source, report) = compilation
        .bindings(&BindingOptions {
            rust_target: RustTarget::RUST_1_64,
            allowlist: [
                "Opaque",
                "Callback",
                "Api",
                "identity",
                "apply",
                "get_callback",
                "c_*",
                "SIZE_*",
                "ALIGN_*",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(report.integer_macros, 4);
    source
}

#[test]
fn aligned_non_object_aliases_keep_pointer_bindings_and_c_query_constants() {
    for profile in CompilerProfile::ALL {
        let source = generate(profile);
        assert!(source.contains("pub type Opaque = ::core::ffi::c_void;"));
        assert!(source.contains("pub type Callback = unsafe extern \"C\" fn("));
        assert!(source.contains("pub const SIZE_VOID:"));
        assert!(source.contains("pub const SIZE_FUNCTION:"));
        assert!(!source.contains("align(32)"));
    }
}

#[test]
#[ignore = "requires native C and Rust compilers"]
fn native_non_object_alias_pointers_and_callbacks() {
    use std::process::Command;
    let host = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("api.h"), HEADER).unwrap();
    let c = r#"#include "api.h"
Opaque *identity(Opaque *p){return p;}
int apply(struct Api a,int n){return a.callback(n)+(a.context!=0);}
static int callback(int n) __attribute__((aligned(1)));
static int callback(int n){return n*3;}
Callback *get_callback(void){return callback;}
unsigned long long c_void_size(void){return SIZE_VOID;}
unsigned long long c_function_size(void){return SIZE_FUNCTION;}
"#;
    std::fs::write(temp.path().join("api.c"), c).unwrap();
    let rustc = std::env::var_os("TOUCAN_TEST_RUSTC").unwrap_or_else(|| "rustc".into());
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|profile| profile.target() == host)
    {
        let bindings = generate(profile);
        std::fs::write(temp.path().join("bindings.rs"), bindings).unwrap();
        let rust = r#"include!("bindings.rs");
#[repr(align(32))] struct Context([u8;32]);
unsafe extern "C" fn callback(n:i32)->i32{n+7}
fn main(){unsafe{
    let mut context=Context([0;32]);
    let p=context.0.as_mut_ptr().cast::<Opaque>();
    assert_eq!(identity(p),p);
    assert_eq!(apply(Api{context:p,callback:Some(callback)},5),13);
    assert_eq!(get_callback().unwrap()(4),12);
    assert_eq!(c_void_size(),1);assert_eq!(c_function_size(),1);
    assert_eq!(SIZE_VOID,1);assert_eq!(SIZE_FUNCTION,1);
    assert_eq!(core::mem::size_of::<Api>(),16);
    assert_eq!(core::mem::align_of::<Api>(),8);
}}
"#;
        std::fs::write(temp.path().join("main.rs"), rust).unwrap();
        let compiler = if profile.compiler() == Compiler::Gnu {
            "gcc"
        } else {
            "clang"
        };
        for c_optimization in [0, 2] {
            let output = Command::new(compiler)
                .current_dir(temp.path())
                .args(["-std=gnu11", "-c", "api.c", "-o", "api.o"])
                .arg(format!("-O{c_optimization}"))
                .output()
                .unwrap();
            assert!(
                toucan_test_support::compiler_acceptance(&output).unwrap(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            for rust_optimization in [0, 3] {
                let mut command = Command::new(&rustc);
                toucan_test_support::link_c_object(&mut command, &temp.path().join("api.o"));
                let output = command
                    .current_dir(temp.path())
                    .args(["--edition=2021", "main.rs", "-o", "consumer"])
                    .arg("-C")
                    .arg(format!("opt-level={rust_optimization}"))
                    .output()
                    .unwrap();
                assert!(
                    toucan_test_support::compiler_acceptance(&output).unwrap(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    Command::new(temp.path().join("consumer"))
                        .status()
                        .unwrap()
                        .success()
                );
            }
        }
    }
}

use toucan_bindings::{Options, RustTarget, generate};
use toucan_semantic::analyze_with_profile;
use toucan_target::{Compiler, CompilerProfile};

fn declarations(size: &str) -> String {
    format!(
        "void*__builtin_malloc({size});void*__builtin_calloc({size},{size});void*__builtin_realloc(void*,{size});void __builtin_free(void*);"
    )
}

#[test]
fn explicit_builtin_declarations_link_to_the_library() {
    for profile in CompilerProfile::ALL {
        let size = if profile.target().long_width() == profile.target().pointer_width() {
            "unsigned long"
        } else {
            "unsigned long long"
        };
        let unit = analyze_with_profile(&declarations(size), profile, &Default::default()).unwrap();
        let output = generate(
            unit.unit(),
            &Options {
                rust_target: if profile.target().is_armv7() {
                    RustTarget::stable(78).unwrap()
                } else {
                    RustTarget::RUST_1_64
                },
                ..Default::default()
            },
        )
        .unwrap();
        for symbol in ["malloc", "calloc", "realloc", "free"] {
            assert!(
                output
                    .source
                    .contains(&format!("#[link_name = \"{symbol}\"]")),
                "{}",
                output.source
            );
            assert!(
                output
                    .source
                    .contains(&format!("pub fn __builtin_{symbol}("))
            );
        }
    }
}

#[test]
#[ignore = "requires native Unix C compilers and rustc"]
fn generated_allocation_calls_and_c_builtin_wrappers_run() {
    use std::process::Command;
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => toucan_target::Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => toucan_target::Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => toucan_target::Target::X86_64AppleDarwin,
        ("aarch64", "macos") => toucan_target::Target::Aarch64AppleDarwin,
        _ => return,
    };
    let mut compilers = vec![("clang".to_owned(), Compiler::Clang)];
    if std::env::consts::OS == "linux" {
        compilers.push((
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            Compiler::Gnu,
        ));
    }
    let rustc = std::env::var("TOUCAN_TEST_RUSTC").unwrap_or_else(|_| "rustc".into());
    let directory = tempfile::tempdir().unwrap();
    let c = directory.path().join("allocation.c");
    std::fs::write(
        &c,
        r#"
unsigned long exercise(unsigned long n){
    unsigned char*p=__builtin_malloc(n);if(!p)return 0;
    for(unsigned long i=0;i<n;i++)p[i]=(unsigned char)(i+1);
    unsigned char*q=__builtin_realloc(p,n+8);if(!q){__builtin_free(p);return 0;}
    unsigned long result=q[n-1];__builtin_free(q);
    unsigned long*z=__builtin_calloc(n,sizeof(unsigned long));if(!z)return 0;
    for(unsigned long i=0;i<n;i++)if(z[i]){__builtin_free(z);return 0;}
    __builtin_free(z);__builtin_free((void*)0);return result+n;
}
"#,
    )
    .unwrap();
    for (cc, compiler) in compilers {
        if compiler == Compiler::Gnu {
            let identity = Command::new(&cc).arg("--version").output().unwrap();
            assert!(identity.status.success());
            assert!(
                !String::from_utf8_lossy(&identity.stdout)
                    .to_ascii_lowercase()
                    .contains("clang"),
                "TOUCAN_GCC must select GNU GCC"
            );
        }
        let profile = CompilerProfile::new(target, compiler).unwrap();
        let implementation = std::fs::read_to_string(&c).unwrap();
        let plain = analyze_with_profile(&implementation, profile, &Default::default()).unwrap();
        let retained = analyze_with_profile(
            &implementation,
            profile,
            &toucan_semantic::AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            format!("{:?}", plain.unit()),
            format!("{:?}", retained.unit())
        );
        let header = declarations("unsigned long") + "unsigned long exercise(unsigned long);";
        let unit = analyze_with_profile(&header, profile, &Default::default()).unwrap();
        let bindings = generate(
            unit.unit(),
            &Options {
                rust_target: RustTarget::RUST_1_64,
                ..Default::default()
            },
        )
        .unwrap();
        let rust = directory.path().join("main.rs");
        std::fs::write(
            &rust,
            format!(
                "{}\n{}",
                bindings.source,
                r#"
fn main(){unsafe{
    assert_eq!(exercise(32),64);
    let p=__builtin_malloc(8) as *mut u8;assert!(!p.is_null());
    for i in 0..8 {p.add(i).write((i+1) as u8);}
    let q=__builtin_realloc(p.cast(),16) as *mut u8;assert!(!q.is_null());
    for i in 0..8 {assert_eq!(q.add(i).read(),(i+1) as u8);}
    __builtin_free(q.cast());
    let z=__builtin_calloc(8,1) as *mut u8;assert!(!z.is_null());
    for i in 0..8 {assert_eq!(z.add(i).read(),0);}
    __builtin_free(z.cast());__builtin_free(core::ptr::null_mut());
}}
"#
            ),
        )
        .unwrap();
        for optimization in ["0", "2"] {
            let object = directory.path().join("allocation.o");
            let output = Command::new(&cc)
                .args(["-std=c11", "-c", "-fPIC", &format!("-O{optimization}")])
                .arg(&c)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(true),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let archive = directory.path().join("liballocation.a");
            assert!(
                Command::new("ar")
                    .arg("crs")
                    .arg(&archive)
                    .arg(&object)
                    .status()
                    .unwrap()
                    .success()
            );
            for rust_optimization in ["0", "3"] {
                let executable = directory.path().join("run");
                let output = Command::new(&rustc)
                    .args([
                        "--edition=2021",
                        "-A",
                        "non_snake_case",
                        "-C",
                        &format!("opt-level={rust_optimization}"),
                    ])
                    .arg(&rust)
                    .arg("-L")
                    .arg(directory.path())
                    .arg("-lstatic=allocation")
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(Command::new(&executable).status().unwrap().success());
            }
        }
    }
}

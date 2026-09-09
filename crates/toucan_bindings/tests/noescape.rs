use toucan_bindings::{Options, RustTarget, generate};
use toucan_semantic::{ParameterContractsId, TypeKind, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};
const HEADER: &str = r#"
typedef int Transform(int *p __attribute__((noescape)));
int invoke(Transform *transform, int *p);
Transform *get_transform(void);
typedef void Diverge(void) __attribute__((noreturn));
_Noreturn void standard_noreturn(void);
int typed_noreturn(void) __attribute__((noreturn));
"#;
#[test]
fn contracts_have_callback_safety_comments_without_changing_the_rust_abi() {
    for profile in CompilerProfile::ALL {
        let a = analyze_with_profile(HEADER, profile, &Default::default()).unwrap();
        let bindings = generate(
            a.unit(),
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
        assert_eq!(
            bindings
                .source
                .contains("C noescape: implementers must not retain derived references"),
            profile.compiler() == Compiler::Clang
        );
        assert!(bindings.source.contains("extern \"C\" fn"));
        assert_eq!(
            bindings
                .source
                .contains("C noreturn: implementers must not return"),
            profile.compiler() == Compiler::Clang
        );
        assert!(bindings.source.contains("standard_noreturn()"));
        assert!(
            bindings
                .source
                .contains("typed_noreturn() -> ::core::ffi::c_int")
        );
        assert!(!bindings.source.contains("-> !"));
    }
}
#[test]
fn invalid_contract_ids_fail_before_output() {
    let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap();
    let a = analyze_with_profile(HEADER, profile, &Default::default()).unwrap();
    let mut unit = a.unit().clone();
    let TypeKind::Function(f) = &mut unit.typedefs.get_mut("Transform").unwrap().kind else {
        panic!()
    };
    f.parameter_contracts = ParameterContractsId::new(32);
    assert!(
        generate(&unit, &Options::default())
            .unwrap_err()
            .0
            .contains("invalid parameter-contract ID")
    );
}
#[test]
#[ignore = "requires native Clang and rustc; TOUCAN_TEST_RUSTC can select Rust 1.64"]
fn generated_callbacks_cross_c_without_retaining_pointers() {
    use std::process::Command;
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Target::X86_64UnknownLinuxGnu,
        ("linux", "aarch64") => Target::Aarch64UnknownLinuxGnu,
        ("macos", "x86_64") => Target::X86_64AppleDarwin,
        ("macos", "aarch64") => Target::Aarch64AppleDarwin,
        ("windows", "x86_64") => Target::X86_64PcWindowsMsvc,
        _ => return,
    };
    let profile = CompilerProfile::new(target, Compiler::Clang).unwrap();
    let a = analyze_with_profile(HEADER, profile, &Default::default()).unwrap();
    let bindings = generate(
        a.unit(),
        &Options {
            rust_target: RustTarget::RUST_1_64,
            ..Default::default()
        },
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let c = dir.path().join("test.c");
    let rust = dir.path().join("main.rs");
    let obj = dir.path().join("test.o");
    let executable = dir
        .path()
        .join(format!("test{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&c,format!("{HEADER}\nstatic int add(int*p __attribute__((noescape))){{*p+=7;return *p;}}int invoke(Transform*t,int*p){{return t(p);}}Transform*get_transform(void){{return add;}}")).unwrap();
    std::fs::write(&rust,format!("#![allow(non_camel_case_types,dead_code)]\n{}\nunsafe extern \"C\" fn double(p:*mut i32)->i32{{*p*=2;*p}}\nfn main(){{unsafe{{let mut n=4;assert_eq!(invoke(Some(double),&mut n),8);assert_eq!(n,8);let f=get_transform().unwrap();assert_eq!(f(&mut n),15);assert_eq!(n,15);}}}}",bindings.source)).unwrap();
    let rustc = std::env::var("TOUCAN_TEST_RUSTC").unwrap_or_else(|_| "rustc".into());
    for c_opt in ["-O0", "-O2"] {
        let out = Command::new("clang")
            .arg(c_opt)
            .arg("-c")
            .arg(&c)
            .arg("-o")
            .arg(&obj)
            .output()
            .unwrap();
        assert!(
            toucan_test_support::compiler_acceptance(&out).unwrap(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        for opt in ["0", "3"] {
            let out = Command::new(&rustc)
                .args([
                    "--edition=2021",
                    "-C",
                    &format!("opt-level={opt}"),
                    "-C",
                    &format!("link-arg={}", obj.display()),
                ])
                .arg(&rust)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let out = Command::new(&executable).output().unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

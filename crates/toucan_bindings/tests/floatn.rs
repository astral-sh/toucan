use toucan_bindings::{Options, RustTarget, generate};
use toucan_semantic::analyze_with_profile;
use toucan_target::{Compiler, CompilerProfile, Target};

const HEADER: &str = r#"
typedef _Float32 F32; typedef _Float64 F64; typedef _Float32x F32x;
struct Pair {F32 a;F64 b;};
F32 sum32(F32,F32);F64 sum64(F64,F64);F32x sum32x(F32x,F32x);
F32 stack32(F32,F32,F32,F32,F32,F32,F32,F32,F32,F32);
F64 stack64(F64,F64,F64,F64,F64,F64,F64,F64,F64,F64);
F32 callback32(F32(*)(F32,F32),F32,F32);
F64 callback64(F64(*)(F64,F64),F64,F64);
F32x callback32x(F32x(*)(F32x,F32x),F32x,F32x);
struct Pair pair(struct Pair);
struct Pair callback_pair(struct Pair(*)(struct Pair),struct Pair);
int variadic_check(void);
"#;

#[test]
fn interchange_storage_maps_to_proved_rust_primitives() {
    for p in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Gnu)
    {
        let a = analyze_with_profile(HEADER, p, &Default::default()).unwrap();
        for target in [RustTarget::RUST_1_64, RustTarget::stable(96).unwrap()] {
            let b = generate(
                a.unit(),
                &Options {
                    rust_target: target,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(b.source.contains("pub type F32 = ::core::primitive::f32;"));
            assert!(b.source.contains("pub type F64 = ::core::primitive::f64;"));
            assert!(b.source.contains("pub type F32x = ::core::primitive::f64;"));
            assert!(b.source.contains("pub fn callback_pair"));
        }
        for (source, reason) in [
            ("_Float64x f(_Float64x);", "_Float64x"),
            (
                "_Atomic(_Float32) f(_Atomic(_Float32));",
                "separate ABI proof",
            ),
            ("_Complex _Float32 f(_Complex _Float32);", "complex"),
            (
                "typedef _Float32 V __attribute__((vector_size(16)));V f(V);",
                "vectors",
            ),
        ] {
            let a = analyze_with_profile(source, p, &Default::default()).unwrap();
            let e = generate(a.unit(), &Default::default()).unwrap_err();
            assert!(e.to_string().contains(reason), "{source}: {e}");
        }
        let a=analyze_with_profile("_Float32 *p;_Atomic(_Float32) *q;typedef _Float32 V __attribute__((vector_size(16)));V *v;",p,&Default::default()).unwrap();
        generate(a.unit(), &Default::default()).unwrap();
    }
}

#[test]
#[ignore = "requires native GNU Linux C compiler and rustc"]
fn generated_floatn_calls_match_native_gnu() {
    use std::process::Command;
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        _ => return,
    };
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(identity.status.success());
    assert!(
        !String::from_utf8_lossy(&identity.stdout)
            .to_ascii_lowercase()
            .contains("clang")
    );
    let rustc = std::env::var("TOUCAN_TEST_RUSTC").unwrap_or_else(|_| "rustc".into());
    let p = CompilerProfile::new(target, Compiler::Gnu).unwrap();
    let a = analyze_with_profile(HEADER, p, &Default::default()).unwrap();
    let b = generate(
        a.unit(),
        &Options {
            rust_target: RustTarget::RUST_1_64,
            ..Default::default()
        },
    )
    .unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = d.path().join("floatn.c");
    let implementation = r#"
F32 sum32(F32 a,F32 b){return a+b;}F64 sum64(F64 a,F64 b){return a+b;}F32x sum32x(F32x a,F32x b){return a+b;}
F32 stack32(F32 a,F32 b,F32 c,F32 d,F32 e,F32 f,F32 g,F32 h,F32 i,F32 j){return a+i+j;}
F64 stack64(F64 a,F64 b,F64 c,F64 d,F64 e,F64 f,F64 g,F64 h,F64 i,F64 j){return a+i+j;}
F32 callback32(F32(*f)(F32,F32),F32 a,F32 b){return f(a,b);}
F64 callback64(F64(*f)(F64,F64),F64 a,F64 b){return f(a,b);}
F32x callback32x(F32x(*f)(F32x,F32x),F32x a,F32x b){return f(a,b);}
struct Pair pair(struct Pair p){p.a*=2;p.b*=3;return p;}
struct Pair callback_pair(struct Pair(*f)(struct Pair),struct Pair p){return f(p);}
static int variadic(int n,...){__builtin_va_list a;F32 f;F64 d;F32x x;__builtin_va_start(a,n);f=__builtin_va_arg(a,F32);d=__builtin_va_arg(a,F64);x=__builtin_va_arg(a,F32x);__builtin_va_end(a);return f==1.25f32&&d==2.5f64&&x==3.75f32x;}
int variadic_check(void){return variadic(3,1.25f32,2.5f64,3.75f32x);}
"#;
    std::fs::write(&c, format!("{HEADER}\n{implementation}")).unwrap();
    let source = format!("{HEADER}\n{implementation}");
    let kept = analyze_with_profile(
        &source,
        p,
        &toucan_semantic::AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    )
    .unwrap();
    let plain = analyze_with_profile(&source, p, &Default::default()).unwrap();
    assert_eq!(format!("{:?}", kept.unit()), format!("{:?}", plain.unit()));
    let rust = d.path().join("main.rs");
    std::fs::write(&rust,format!("{}\n{}",b.source,r#"
unsafe extern "C" fn mul32(a:f32,b:f32)->f32{a*b}
unsafe extern "C" fn mul64(a:f64,b:f64)->f64{a*b}
unsafe extern "C" fn rust_pair(p:Pair)->Pair{Pair{a:p.a*5.0,b:p.b*7.0}}
fn main(){unsafe{
 assert_eq!(sum32(1.25,2.5),3.75);assert_eq!(sum64(1.25,2.5),3.75);assert_eq!(sum32x(1.25,2.5),3.75);
 assert_eq!(stack32(1.,2.,3.,4.,5.,6.,7.,8.,9.,10.),20.);assert_eq!(stack64(1.,2.,3.,4.,5.,6.,7.,8.,9.,10.),20.);
 assert_eq!(callback32(Some(mul32),1.5,2.5),3.75);assert_eq!(callback64(Some(mul64),1.5,2.5),3.75);assert_eq!(callback32x(Some(mul64),1.5,2.5),3.75);
 let p=pair(Pair{a:1.25,b:2.5});assert_eq!((p.a,p.b),(2.5,7.5));let p=callback_pair(Some(rust_pair),Pair{a:1.25,b:2.5});assert_eq!((p.a,p.b),(6.25,17.5));assert_eq!(variadic_check(),1);
}}
"#)).unwrap();
    for co in ["0", "2"] {
        let object = d.path().join("floatn.o");
        let out = Command::new(&gcc)
            .args(["-std=gnu11", &format!("-O{co}"), "-c"])
            .arg(&c)
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&out),
            Ok(true),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        for ro in ["0", "3"] {
            let binary = d.path().join("calls");
            let out = Command::new(&rustc)
                .args(["--edition=2021", "-C", &format!("opt-level={ro}"), "-C"])
                .arg(format!("link-arg={}", object.display()))
                .arg(&rust)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let out = Command::new(&binary).output().unwrap();
            assert!(out.status.success(), "C{co}/Rust{ro}: {out:?}");
        }
    }
}

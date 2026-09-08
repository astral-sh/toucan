use toucan_semantic::{
    AnalysisOptions, ArithmeticConstant, FloatKind, FloatingFormat, Type, TypeKind,
    analyze_with_profile, evaluate_arithmetic, evaluate_integer,
};
use toucan_target::{Compiler, CompilerProfile, Target};

fn parity(source: &str, profile: CompilerProfile, accepted: bool) {
    let plain = analyze_with_profile(source, profile, &AnalysisOptions::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (plain, retained) {
        (Ok(a), Ok(b)) => {
            assert!(accepted, "unexpected acceptance {profile:?}: {source}");
            assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit()));
        }
        (Err(a), Err(b)) => {
            assert!(!accepted, "{profile:?}: {source}: {a}");
            assert_eq!((a.offset, a.message), (b.offset, b.message));
        }
        other => panic!("parity {profile:?}: {source}: {other:?}"),
    }
}

fn availability(profile: CompilerProfile) -> Vec<(&'static str, bool)> {
    let gnu = profile.compiler() == Compiler::Gnu;
    let x86_linux = matches!(
        profile.target(),
        Target::X86_64UnknownLinuxGnu | Target::X86_64UnknownLinuxMusl
    );
    let linux = matches!(
        profile.target(),
        Target::X86_64UnknownLinuxGnu
            | Target::X86_64UnknownLinuxMusl
            | Target::Aarch64UnknownLinuxGnu
            | Target::Aarch64UnknownLinuxMusl
    );
    vec![
        (
            "typedef __typeof__(1.0Q) Q; typedef __typeof__(1.0Qi) C; _Static_assert(sizeof(Q)==16&&_Alignof(Q)==16&&sizeof(C)==32&&_Alignof(C)==16,\"layout\");_Static_assert(sizeof(_Atomic(Q))==16&&_Alignof(_Atomic(Q))==16&&sizeof(_Atomic(C))==32&&_Alignof(_Atomic(C))==16,\"atomic\");",
            true,
        ),
        ("__float128 x;", x86_linux),
        ("_Float128 x;", gnu),
        ("__float128 _Complex z;", x86_linux && !gnu),
        ("_Float128 _Complex z;", gnu),
        ("_Float128 x=1.0f128;", gnu),
        ("__typeof__(1.0Q) x=1.0Q;", true),
        ("__typeof__(1.0Qi) z=1.0iQ;", true),
        (
            "__typeof__(1.0Q) f(__typeof__(1.0Q) x){return x+1.0Q;}",
            true,
        ),
        ("__typeof__(1.0Qi) z=__builtin_complex(1.0Q,2.0Q);", true),
        ("typedef float __attribute__((mode(TF))) F;F x;", linux),
        (
            "typedef _Complex float __attribute__((mode(TC))) C;C x;",
            linux,
        ),
        (
            "typedef _Complex float __attribute__((mode(TF))) F;",
            linux && !gnu,
        ),
        ("typedef float __attribute__((mode(TC))) C;", false),
        ("typedef int __attribute__((mode(TF))) F;", false),
        ("float __attribute__((mode(TF))) *p;", false),
        ("typedef float __attribute__((mode(TF))) A[2];", false),
        ("float __attribute__((mode(TF))) f(void);", false),
        ("void f(float __attribute__((mode(TF))) *p);", false),
        ("void f(float x __attribute__((mode(TF))));", linux),
        (
            "typedef _Complex double __attribute__((mode(SC))) C;_Static_assert(__builtin_types_compatible_p(C,float _Complex),\"same\");",
            true,
        ),
        (
            "typedef _Complex float __attribute__((mode(DC))) C;_Static_assert(__builtin_types_compatible_p(C,double _Complex),\"same\");",
            true,
        ),
    ]
}

#[test]
fn spellings_literals_and_machine_modes_have_separate_availability() {
    for profile in CompilerProfile::ALL {
        let gnu = profile.compiler() == Compiler::Gnu;
        let x86_linux = matches!(
            profile.target(),
            Target::X86_64UnknownLinuxGnu | Target::X86_64UnknownLinuxMusl
        );
        let linux = matches!(
            profile.target(),
            Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        );
        for (source, accepted) in availability(profile) {
            parity(source, profile, accepted);
        }
        let expected = if gnu
            && matches!(
                profile.target(),
                Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
            ) {
            "long double"
        } else {
            "__typeof__(1.0Q)"
        };
        parity(
            &format!(
                "_Static_assert(__builtin_types_compatible_p(__typeof__(1.0Q),{expected}),\"identity\");"
            ),
            profile,
            true,
        );
        if gnu {
            parity(
                "_Float128 x;_Static_assert(__builtin_types_compatible_p(__typeof__(x+1.0L),_Float128),\"rank\");",
                profile,
                true,
            );
        }
        if linux {
            let expected = if x86_linux {
                "__typeof__(1.0Q)"
            } else {
                "long double"
            };
            parity(
                &format!(
                    "typedef float __attribute__((mode(TF))) F;_Static_assert(__builtin_types_compatible_p(F,{expected}),\"mode\");"
                ),
                profile,
                true,
            );
        }
        let expected = if gnu { 16 } else { 4 };
        parity(
            &format!(
                "_Static_assert(sizeof(float __attribute__((mode(TF))))=={expected},\"type mode\");"
            ),
            profile,
            linux || !gnu,
        );
        parity(
            "enum{N=sizeof(float __attribute__((mode(TF)))*)};",
            profile,
            !gnu,
        );
    }
}

#[test]
fn builtin_typedef_shadowing_preserves_prior_types_and_parameter_scopes() {
    for profile in CompilerProfile::ALL {
        if profile.compiler() != Compiler::Gnu {
            continue;
        }
        for source in [
            "typedef int __float128;__float128 x;_Static_assert(sizeof(x)==4,\"shadow\");",
            "void f(int __float128){__float128=1;} void g(void){typedef int __float128;__float128 x=2;}",
            "int f(void){int __float128=1; {typedef char __float128;__float128 x=0;}return __float128;}",
        ] {
            parity(source, profile, true);
        }
        parity(
            "int __float128=1; int f(void){return __float128;}",
            profile,
            !profile.target().is_x86_64(),
        );
        if matches!(
            profile.target(),
            Target::X86_64UnknownLinuxGnu | Target::X86_64UnknownLinuxMusl
        ) {
            parity(
                "__float128 a;typedef int __float128;__float128 b;_Static_assert(sizeof(a)==16 && sizeof(b)==4,\"replacement\");",
                profile,
                true,
            );
            parity(
                "__float128 a;void f(int __float128){__float128=1;}__float128 b;_Static_assert(sizeof(a)==16 && sizeof(b)==16,\"scope\");",
                profile,
                true,
            );
            let unit = analyze_with_profile("__float128 a;", profile, &AnalysisOptions::default())
                .unwrap();
            assert!(!unit.unit().typedefs.contains_key("__float128"));
            assert_eq!(
                evaluate_integer(unit.unit(), "sizeof(__float128)")
                    .unwrap()
                    .value,
                16
            );
            for source in [
                "void f(int __float128);__float128 value;",
                "void (*callback)(int __float128);__float128 value;",
            ] {
                parity(source, profile, true);
            }
            let unit = analyze_with_profile(
                "__float128 a;typedef int __float128;",
                profile,
                &Default::default(),
            )
            .unwrap();
            assert_eq!(
                evaluate_integer(unit.unit(), "sizeof(__float128)")
                    .unwrap()
                    .value,
                4
            );
            assert_eq!(evaluate_integer(unit.unit(), "sizeof a").unwrap().value, 16);
        }
    }
}

#[test]
fn target_encodings_layouts_and_conversions_preserve_binary128_identity() {
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile("", profile, &AnalysisOptions::default()).unwrap();
        let q_kind = if profile.compiler() == Compiler::Gnu
            && matches!(
                profile.target(),
                Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
            ) {
            FloatKind::LongDouble
        } else {
            FloatKind::FLOAT128
        };
        for (source, bits) in [
            ("1.0Q", 0x3fff0000000000000000000000000000u128),
            ("-0.0Q", 1u128 << 127),
            (
                "0x1.123456789abcdef0123456789abcp0Q",
                0x3fff123456789abcdef0123456789abc,
            ),
            ("1.0Q+0x1p-112Q", 0x3fff0000000000000000000000000001),
            ("1.0Q+0x1p-113Q", 0x3fff0000000000000000000000000000),
            ("1.0Q/3.0Q", 0x3ffd5555555555555555555555555555),
        ] {
            let ArithmeticConstant::Floating(value) = evaluate_arithmetic(analysis.unit(), source)
                .unwrap_or_else(|e| panic!("{profile:?}: {source}: {e}"))
            else {
                panic!("floating")
            };
            assert_eq!(value.format(), FloatingFormat::Binary128);
            assert_eq!(value.kind(), q_kind);
            assert_eq!(value.to_bits(), bits, "{source}");
        }
        let ArithmeticConstant::Complex(value) =
            evaluate_arithmetic(analysis.unit(), "__builtin_complex(-0.0Q,2.0Q)").unwrap()
        else {
            panic!("complex")
        };
        assert_eq!(value.kind(), q_kind);
        assert_eq!(value.real().to_bits(), 1u128 << 127);
        assert_eq!(
            value.imaginary().to_bits(),
            0x40000000000000000000000000000000
        );
        for (ty, size) in [
            (Type::new(TypeKind::Float(FloatKind::FLOAT128)), 16),
            (Type::new(TypeKind::Complex(FloatKind::FLOAT128)), 32),
        ] {
            assert_eq!(analysis.unit().layout(&ty).unwrap().size_bits / 8, size);
            assert_eq!(analysis.unit().alignment(&ty).unwrap(), 16);
            let atomic = Type::new(TypeKind::Atomic(Box::new(ty)));
            assert_eq!(analysis.unit().layout(&atomic).unwrap().size_bits / 8, size);
            assert_eq!(analysis.unit().alignment(&atomic).unwrap(), 16);
        }
    }
}

#[test]
fn retained_arithmetic_keeps_real_and_complex_domains_and_atomic_access() {
    use toucan_semantic::checked::{AtomicAccess, Binary, Conversion, ExprKind};
    let source = "typedef __typeof__(1.0Q) Q; typedef __typeof__(1.0Qi) C; Q real; C complex; _Atomic(C) shared; double narrow; void f(void){complex=real+complex; shared+=real; narrow=real;}";
    for profile in CompilerProfile::ALL {
        parity(source, profile, true);
        let analysis = analyze_with_profile(
            source,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let code = analysis.checked().unwrap();
        let mut mixed = 0;
        let mut atomic = 0;
        let mut narrowing = 0;
        for (_, expression) in code.expressions() {
            match expression.kind() {
                ExprKind::Binary {
                    operator: Binary::Plus,
                    left,
                    right,
                    ..
                } => {
                    assert!(matches!(
                        code.ty(left.effective_type()).unwrap().kind,
                        TypeKind::Float(_)
                    ));
                    assert!(matches!(
                        code.ty(right.effective_type()).unwrap().kind,
                        TypeKind::Complex(_)
                    ));
                    assert!(matches!(
                        code.ty(expression.ty()).unwrap().kind,
                        TypeKind::Complex(_)
                    ));
                    mixed += 1;
                }
                ExprKind::Binary {
                    operator: Binary::AssignPlus,
                    left,
                    right,
                    ..
                } => {
                    assert_eq!(
                        expression.atomic_access(),
                        Some(AtomicAccess::ReadModifyWrite)
                    );
                    assert!(
                        left.conversions()
                            .iter()
                            .any(|step| step.kind() == Conversion::AtomicLoad)
                    );
                    assert!(matches!(
                        code.ty(right.effective_type()).unwrap().kind,
                        TypeKind::Float(_)
                    ));
                    atomic += 1;
                }
                ExprKind::Binary {
                    operator: Binary::Assign,
                    right,
                    ..
                } if code.ty(right.effective_type()).unwrap().kind
                    == TypeKind::Float(FloatKind::Double) =>
                {
                    assert!(
                        right
                            .conversions()
                            .iter()
                            .any(|step| step.kind() == Conversion::Assignment)
                    );
                    narrowing += 1;
                }
                _ => {}
            }
        }
        assert_eq!((mixed, atomic, narrowing), (1, 1, 1));
    }
}

#[test]
#[ignore = "requires native compilers and Clang cross targets"]
fn native_binary128_availability_and_encodings() {
    use std::process::Command;
    let host = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => panic!("unsupported native oracle host"),
    };
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("float128.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let gcc_version = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(gcc_version.status.success());
    let gcc_is_clang = String::from_utf8_lossy(&gcc_version.stdout)
        .to_ascii_lowercase()
        .contains("clang");
    for (compiler, targets) in [
        (gcc.as_str(), vec![(host, false)]),
        (
            "clang",
            Target::ALL.iter().map(|target| (*target, true)).collect(),
        ),
    ] {
        for (target, cross) in targets {
            let gnu = compiler != "clang" && !gcc_is_clang;
            // GNU Darwin is outside the shipped profile set; Clang still checks
            // every cross target and the native constant bytes below.
            let profile =
                CompilerProfile::new(target, if gnu { Compiler::Gnu } else { Compiler::Clang });
            if let Ok(profile) = profile {
                for (source, expected) in availability(profile).into_iter().chain([
                    ("typedef int __float128;__float128 value;", gnu),
                    ("void f(int __float128){__float128=1;}", gnu),
                ]) {
                    std::fs::write(&input, format!("{source}\n")).unwrap();
                    let mut command = Command::new(compiler);
                    command.args(["-std=c11", "-fsyntax-only"]);
                    if cross {
                        command.arg(format!("--target={target}"));
                    }
                    let output = command.arg(&input).output().unwrap();
                    assert_eq!(
                        toucan_test_support::compiler_acceptance(&output),
                        Ok(expected),
                        "{compiler} {target:?}: {source}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
        }
    }
    let source = r#"#include <stdio.h>
#include <string.h>
_Static_assert(sizeof(1.0Q)==16,"binary128 size");
static __typeof__(1.0Q) values[]={1.0Q,-0.0Q,0x1.123456789abcdef0123456789abcp0Q,1.0Q+0x1p-112Q,1.0Q+0x1p-113Q,1.0Q/3.0Q};
static __typeof__(1.0Qi) pair=__builtin_complex(-0.0Q,2.0Q);
int main(void){unsigned char bytes[16];for(unsigned i=0;i<sizeof(values)/sizeof(values[0]);i++){memcpy(bytes,&values[i],16);for(int j=15;j>=0;j--)printf("%02x",bytes[j]);puts("");}memcpy(bytes,&pair,16);for(int j=15;j>=0;j--)printf("%02x",bytes[j]);puts("");memcpy(bytes,(char*)&pair+16,16);for(int j=15;j>=0;j--)printf("%02x",bytes[j]);puts("");return 0;}
"#;
    std::fs::write(&input, source).unwrap();
    for compiler in [gcc.as_str(), "clang"] {
        if compiler != "clang"
            && !gcc_is_clang
            && !matches!(
                host,
                Target::X86_64UnknownLinuxGnu
                    | Target::X86_64UnknownLinuxMusl
                    | Target::Aarch64UnknownLinuxGnu
                    | Target::Aarch64UnknownLinuxMusl
            )
        {
            continue; // No GNU Darwin compiler profile is advertised.
        }
        for opt in ["-O0", "-O2"] {
            let binary = directory.path().join("values");
            let output = Command::new(compiler)
                .args(["-std=c11", opt])
                .arg(&input)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(true),
                "{compiler} {opt}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(&binary).output().unwrap();
            assert!(output.status.success(), "{compiler} {opt}: {output:?}");
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                "3fff0000000000000000000000000000\n80000000000000000000000000000000\n3fff123456789abcdef0123456789abc\n3fff0000000000000000000000000001\n3fff0000000000000000000000000000\n3ffd5555555555555555555555555555\n80000000000000000000000000000000\n40000000000000000000000000000000\n"
            );
        }
    }
}

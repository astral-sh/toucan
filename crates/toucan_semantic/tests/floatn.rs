use toucan_semantic::{
    Analysis, AnalysisOptions, ArithmeticConstant, FloatKind, FloatingFormat, TypeKind,
    analyze_with_profile, evaluate_arithmetic, evaluate_integer,
};
use toucan_target::{Compiler, CompilerProfile, LanguageMode, Target};

fn profiles() -> impl Iterator<Item = CompilerProfile> {
    CompilerProfile::ALL
        .into_iter()
        .flat_map(|p| LanguageMode::ALL.map(|mode| p.with_language_mode(mode)))
}
fn check(source: &str, profile: CompilerProfile) -> Result<Analysis, toucan_semantic::Error> {
    let plain = analyze_with_profile(source, profile, &Default::default());
    let kept = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&plain, &kept) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => assert_eq!((&a.message, a.offset), (&b.message, b.offset)),
        _ => panic!("{profile:?}: {source}: {plain:?} {kept:?}"),
    }
    kept
}
const TYPES: &[(&str, FloatKind, u64)] = &[
    ("_Float32", FloatKind::FLOAT32, 4),
    ("_Float64", FloatKind::FLOAT64, 8),
    ("_Float32x", FloatKind::FLOAT32X, 8),
    ("_Float64x", FloatKind::FLOAT64X, 16),
];

#[test]
fn gnu_types_are_distinct_with_target_storage() {
    for p in profiles() {
        for &(name, kind, size) in TYPES {
            let source = format!(
                "{name} value; _Static_assert(sizeof({name})=={size} && _Alignof({name})=={size},\"layout\");"
            );
            let result = check(&source, p);
            assert_eq!(
                result.is_ok(),
                p.compiler() == Compiler::Gnu,
                "{p:?}: {name}: {result:?}"
            );
            if let Ok(a) = result {
                assert_eq!(a.unit().declarations[0].ty.kind, TypeKind::Float(kind));
            }
        }
        if p.compiler() == Compiler::Clang {
            let a = check("", p).unwrap();
            assert!(
                a.unit()
                    .layout(&toucan_semantic::Type::new(TypeKind::Float(
                        FloatKind::FLOAT64X
                    )))
                    .is_err()
            );
        }
        if p.compiler() == Compiler::Gnu {
            check("_Static_assert(!__builtin_types_compatible_p(_Float32,float),\"distinct\");_Static_assert(!__builtin_types_compatible_p(_Float64,double),\"distinct\");_Static_assert(!__builtin_types_compatible_p(_Float32x,double),\"distinct\");_Static_assert(!__builtin_types_compatible_p(_Float64,_Float32x),\"distinct\");_Static_assert(!__builtin_types_compatible_p(_Float64x,long double),\"distinct\");",p).unwrap();
            assert!(check("float f(float); _Float32 f(_Float32);", p).is_err());
        }
    }
}

#[test]
fn equal_precision_arithmetic_preserves_interchange_and_extended_priority() {
    for p in profiles().filter(|p| p.compiler() == Compiler::Gnu) {
        for (a, b, result) in [
            ("float", "_Float32", "_Float32"),
            ("double", "_Float32", "double"),
            ("double", "_Float64", "_Float64"),
            ("double", "_Float32x", "double"),
            ("_Float64", "_Float32x", "_Float64"),
            ("long double", "_Float64x", "long double"),
            ("_Float128", "_Float64x", "_Float128"),
        ] {
            check(&format!("_Static_assert(__builtin_types_compatible_p(__typeof__(({a})0+({b})0),{result}),\"rank\");_Static_assert(__builtin_types_compatible_p(__typeof__(({b})0+({a})0),{result}),\"reverse\");"),p).unwrap();
        }
        let a = check(
            "void var(int,...);void old();void f(_Float32 x,_Float32x y){var(0,x,y);old(x,y);}",
            p,
        )
        .unwrap();
        let code = a.checked().unwrap();
        let mut calls = 0;
        for (_, e) in code.expressions() {
            if let toucan_semantic::checked::ExprKind::Call { arguments, .. } = e.kind() {
                let tail = &arguments[arguments.len() - 2..];
                for (arg, kind) in tail.iter().zip([FloatKind::FLOAT32, FloatKind::FLOAT32X]) {
                    assert_eq!(
                        code.ty(arg.effective_type()).unwrap().kind,
                        TypeKind::Float(kind)
                    );
                }
                calls += 1;
            }
        }
        assert_eq!(calls, 2);
        check("void old();void old(_Float32);", p).unwrap();
    }
}

#[test]
fn constants_round_without_losing_nominal_type() {
    for p in profiles().filter(|p| p.compiler() == Compiler::Gnu) {
        let a = check("", p).unwrap();
        for (expression, kind, format, bits) in [
            (
                "0.1f32",
                FloatKind::FLOAT32,
                FloatingFormat::Binary32,
                0x3dcccccd,
            ),
            (
                "-0.0f32",
                FloatKind::FLOAT32,
                FloatingFormat::Binary32,
                0x80000000,
            ),
            (
                "1.0f32+0x1p-24f32",
                FloatKind::FLOAT32,
                FloatingFormat::Binary32,
                0x3f800000,
            ),
            (
                "(_Float32)0x8000008000000001ULL",
                FloatKind::FLOAT32,
                FloatingFormat::Binary32,
                0x5f000001,
            ),
            (
                "0.1f64",
                FloatKind::FLOAT64,
                FloatingFormat::Binary64,
                0x3fb999999999999a,
            ),
            (
                "-0.0f32x",
                FloatKind::FLOAT32X,
                FloatingFormat::Binary64,
                0x8000000000000000,
            ),
            (
                "1.0f64/3.0f64",
                FloatKind::FLOAT64,
                FloatingFormat::Binary64,
                0x3fd5555555555555,
            ),
            (
                "1.0f32x+0x1p-53f32x",
                FloatKind::FLOAT32X,
                FloatingFormat::Binary64,
                0x3ff0000000000000,
            ),
        ] {
            let ArithmeticConstant::Floating(value) =
                evaluate_arithmetic(a.unit(), expression).unwrap()
            else {
                panic!()
            };
            assert_eq!(
                (value.kind(), value.format(), value.to_bits()),
                (kind, format, bits),
                "{p:?}: {expression}"
            );
        }
        let ArithmeticConstant::Floating(value) = evaluate_arithmetic(a.unit(), "1.0f64x").unwrap()
        else {
            panic!()
        };
        let arm = matches!(
            p.target(),
            Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
        );
        assert_eq!(value.kind(), FloatKind::FLOAT64X);
        assert_eq!(
            (value.format(), value.to_bits()),
            if arm {
                (
                    FloatingFormat::Binary128,
                    0x3fff0000000000000000000000000000,
                )
            } else {
                (FloatingFormat::X87, 0x3fff8000000000000000)
            }
        );
        assert_eq!(evaluate_integer(a.unit(), "(int)1.0f32").unwrap().value, 1);
        assert!(evaluate_integer(a.unit(), "(int)0x1p64f64").is_err());
    }
}

#[test]
fn clang_names_and_legacy_header_typedefs_remain_ordinary_types() {
    for p in profiles() {
        let a=check("typedef float _Float32;typedef double _Float64;typedef double _Float32x;typedef long double _Float64x;_Float32 f(_Float32 x){return x;}",p).unwrap();
        assert_eq!(
            evaluate_integer(a.unit(), "sizeof(_Float32)")
                .unwrap()
                .value,
            4
        );
        if p.compiler() == Compiler::Clang {
            check("int _Float32;int f(int _Float64){typedef int _Float32x;_Float32x n=_Float64;return n;}",p).unwrap();
            check("void f(void){typedef int _Float32;_Float32 n=1;}", p).unwrap();
            check("void f(void){typedef float _Float32;_Float32 n=1;}", p).unwrap();
            assert!(check("double x=1.0f32;", p).is_err());
        }
    }
}

#[test]
fn complex_atomic_and_vector_storage_keep_corresponding_real_types() {
    for p in profiles().filter(|p| p.compiler() == Compiler::Gnu) {
        for &(name, kind, size) in TYPES {
            let source = format!(
                "typedef _Complex {name} C;_Static_assert(sizeof(C)=={size}*2,\"complex\");_Atomic({name}) object;C f(C x,{name} y){{return x+y;}}"
            );
            let a = check(&source, p).unwrap();
            assert_eq!(a.unit().typedefs["C"].kind, TypeKind::Complex(kind));
        }
        check("typedef _Float32 V __attribute__((vector_size(16)));V f(V x,_Float32 n){return x+n;}typedef float F __attribute__((vector_size(16)));_Static_assert(!__builtin_types_compatible_p(V,F),\"distinct vectors\");",p).unwrap();
        assert!(check("typedef _Float32 V __attribute__((vector_size(16)));typedef float F __attribute__((vector_size(16)));F f(V x){return x;}",p).is_err());
        for kind in ["_Float64", "_Float32x", "_Float64x", "_Float128"] {
            assert!(check(&format!("typedef float V __attribute__((vector_size(16)));V f(V x,{kind} n){{return x+n;}}"),p).is_err());
        }
    }
}

#[test]
#[ignore = "requires GNU Linux and Clang cross compiler syntax checks"]
fn floatn_spelling_and_constraints_match_compilers() {
    use std::process::Command;
    let host = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Some(Target::X86_64UnknownLinuxGnu),
        ("aarch64", "linux") => Some(Target::Aarch64UnknownLinuxGnu),
        _ => None,
    };
    let mut compilers = Target::ALL
        .into_iter()
        .map(|t| ("clang".to_owned(), Compiler::Clang, t, true))
        .collect::<Vec<_>>();
    if let Some(t) = host {
        compilers.push((
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            Compiler::Gnu,
            t,
            false,
        ));
    }
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("floatn.c");
    for (cc, compiler, target, cross) in compilers {
        let identity = Command::new(&cc).arg("--version").output().unwrap();
        assert!(identity.status.success());
        if compiler == Compiler::Gnu {
            assert!(
                !String::from_utf8_lossy(&identity.stdout)
                    .to_ascii_lowercase()
                    .contains("clang")
            );
        }
        for mode in LanguageMode::ALL {
            let p = CompilerProfile::new(target, compiler)
                .unwrap()
                .with_language_mode(mode);
            for &(name, _, size) in TYPES {
                for (source, accepted) in [
                    (
                        format!("{name} value;_Static_assert(sizeof({name})=={size},\"layout\");"),
                        compiler == Compiler::Gnu,
                    ),
                    (
                        format!("{name} value=1.0{};", name.replacen("_Float", "f", 1)),
                        compiler == Compiler::Gnu,
                    ),
                    (
                        format!("void old();void old({name});"),
                        compiler == Compiler::Gnu,
                    ),
                    (
                        format!("_Complex {name} f(_Complex {name} a){{return a+a;}}"),
                        compiler == Compiler::Gnu,
                    ),
                    (
                        format!(
                            "typedef {name} V __attribute__((vector_size(16)));V f(V a){{return a+a;}}"
                        ),
                        compiler == Compiler::Gnu,
                    ),
                    (
                        format!("int {name};int f(void){{return {name};}}"),
                        compiler == Compiler::Clang,
                    ),
                ] {
                    std::fs::write(&input, &source).unwrap();
                    let mut cmd = Command::new(&cc);
                    cmd.arg(format!("-std={mode}")).arg("-fsyntax-only");
                    if cross {
                        cmd.arg(format!("--target={target}"));
                    }
                    let out = cmd.arg(&input).output().unwrap();
                    assert_eq!(
                        toucan_test_support::compiler_acceptance(&out),
                        Ok(accepted),
                        "{cc}/{p:?}:{source}: {}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                    assert_eq!(check(&source, p).is_ok(), accepted, "{p:?}:{source}");
                }
            }
        }
    }
}

#[test]
#[ignore = "requires native GNU Linux compiler"]
fn gnu_constant_object_bits_match_target_evaluation() {
    use std::process::Command;
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        _ => return,
    };
    let cc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&cc).arg("--version").output().unwrap();
    assert!(identity.status.success());
    assert!(
        !String::from_utf8_lossy(&identity.stdout)
            .to_ascii_lowercase()
            .contains("clang")
    );
    let expressions = [
        "0.1f32",
        "-0.0f32",
        "1.0f32+0x1p-24f32",
        "(_Float32)0x8000008000000001ULL",
        "0.1f64",
        "1.0f64/3.0f64",
        "-0.0f32x",
        "1.0f32x+0x1p-53f32x",
        "1.0f64x",
        "-0.0f64x",
        "1.0f64x/3.0f64x",
    ];
    let a = check("", CompilerProfile::new(target, Compiler::Gnu).unwrap()).unwrap();
    let mut source = String::from("#include <stdio.h>\n#include <string.h>\nint main(void){\n");
    let mut expected = String::new();
    for (i, e) in expressions.iter().enumerate() {
        let ArithmeticConstant::Floating(value) = evaluate_arithmetic(a.unit(), e).unwrap() else {
            panic!()
        };
        let bytes = match value.format() {
            FloatingFormat::Binary32 => 4,
            FloatingFormat::Binary64 => 8,
            FloatingFormat::X87 => 10,
            FloatingFormat::Binary128 => 16,
            _ => panic!(),
        };
        source.push_str(&format!("{{static __typeof__({e}) v{i}=({e});unsigned char b[{bytes}];int j;memcpy(b,&v{i},{bytes});for(j={bytes}-1;j>=0;j--)printf(\"%02x\",b[j]);puts(\"\");}}\n"));
        expected.push_str(&format!(
            "{:0width$x}\n",
            value.to_bits(),
            width = bytes * 2
        ));
    }
    source.push_str("return 0;}\n");
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("bits.c");
    std::fs::write(&input, source).unwrap();
    for opt in ["-O0", "-O2"] {
        let binary = d.path().join("bits");
        let out = Command::new(&cc)
            .args(["-std=gnu11", opt])
            .arg(&input)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&out),
            Ok(true),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let out = Command::new(&binary).output().unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);
    }
}

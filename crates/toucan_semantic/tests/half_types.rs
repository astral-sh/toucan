use toucan_semantic::{
    Analysis, AnalysisOptions, ArithmeticConstant, Error, FloatKind, FloatingFormat, TypeKind,
    analyze_with_profile, evaluate_arithmetic,
};
use toucan_target::{Compiler, CompilerProfile, Target};

fn supports_narrow_types(profile: CompilerProfile) -> bool {
    // Both Clang targeting i686 and GCC -m32 reject _Float16 and __bf16.
    profile.target() != Target::I686UnknownLinuxGnu
}

fn check(source: &str, profile: CompilerProfile) -> Result<Analysis, Error> {
    let plain = analyze_with_profile(source, profile, &AnalysisOptions::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&plain, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => {
            assert_eq!(a.offset, b.offset);
            assert_eq!(a.message, b.message);
        }
        _ => panic!("{profile:?}: {plain:?} {retained:?}"),
    }
    retained
}

const SOURCE: &str = r#"
 typedef _Float16 H; typedef __bf16 B;
 H half; B brain; H array[3];
 struct Pair {char c;H half;B brain;char t;};
 typedef H HV __attribute__((vector_size(16)));
 typedef B BV __attribute__((vector_size(16)));
 _Static_assert(sizeof(H)==2 && _Alignof(H)==2,"half");
 _Static_assert(sizeof(B)==2 && _Alignof(B)==2,"brain");
 _Static_assert(sizeof(struct Pair)==8,"record");
 _Static_assert(sizeof(HV)==16 && _Alignof(BV)==16,"vectors");
 _Static_assert(!__builtin_types_compatible_p(H,B),"distinct");
 _Static_assert(!__builtin_types_compatible_p(H,float),"not float");
 _Static_assert(_Generic(half+brain,H:1,default:0),"common half");
 _Static_assert(_Generic(brain+1,B:1,default:0),"integer conversion");
 _Static_assert(_Generic(half+1.0f,float:1,default:0),"common float");
 _Static_assert(_Generic(+brain,B:1,default:0),"unary");
 void variadic(int,...);
 H sink(H h,B b){variadic(1,h,b,1.0f); h+=b; ++b; return h+b;}
 HV vector_add(HV a,HV b){return a+b;}
 BV brain_add(BV a,BV b){return a+b;}
 float cast(H h,B b){return (float)h+(float)b;}
 _Atomic(H) atomic_half; _Atomic(B) atomic_brain;
 void store(void){atomic_half=1;atomic_brain=2;}
 H initialized=1.00048828125; B initialized_brain=1.00390625;
 H literal=1.5f16;
"#;

#[test]
fn half_types_keep_distinct_storage_and_nominal_arithmetic_types() {
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|profile| supports_narrow_types(*profile))
    {
        let a = check(SOURCE, profile).unwrap_or_else(|e| panic!("{profile:?}: {e}"));
        let u = a.unit();
        assert_eq!(
            u.resolve(&u.typedefs["H"]).unwrap().kind,
            TypeKind::Float(FloatKind::FLOAT16)
        );
        assert_eq!(
            u.resolve(&u.typedefs["B"]).unwrap().kind,
            TypeKind::Float(FloatKind::BFloat16)
        );
        let code = a.checked().unwrap();
        let (_, call) = code
            .expressions()
            .find(|(_, expression)| {
                matches!(expression.kind(), toucan_semantic::checked::ExprKind::Call { arguments, .. } if arguments.len() == 4)
            })
            .unwrap();
        let toucan_semantic::checked::ExprKind::Call { arguments, .. } = call.kind() else {
            unreachable!()
        };
        for (arg, kind) in
            arguments[1..]
                .iter()
                .zip([FloatKind::FLOAT16, FloatKind::BFloat16, FloatKind::Double])
        {
            assert_eq!(
                u.resolve(code.ty(arg.effective_type()).unwrap())
                    .unwrap()
                    .kind,
                TypeKind::Float(kind)
            );
        }
    }
}

#[test]
fn apfloat_conversions_preserve_narrow_bits_and_integer_ranges() {
    let values = [
        ("0.0", 0x0000, 0x0000),
        ("-0.0", 0x8000, 0x8000),
        ("1.0", 0x3c00, 0x3f80),
        ("-2.0", 0xc000, 0xc000),
        ("1.00048828125", 0x3c00, 0x3f80),
        ("1.00390625", 0x3c04, 0x3f80),
        ("65504.0", 0x7bff, 0x4780),
        ("0x1p-24", 0x0001, 0x3380),
        ("0x1p-25", 0x0000, 0x3300),
        ("0x1p-133", 0x0000, 0x0001),
        ("0x1p-134", 0x0000, 0x0000),
        ("__builtin_inf()", 0x7c00, 0x7f80),
        ("-__builtin_inf()", 0xfc00, 0xff80),
        ("__builtin_nan(\"0\")", 0x7e00, 0x7fc0),
    ];
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|profile| supports_narrow_types(*profile))
    {
        let a = check("", profile).unwrap();
        let u = a.unit();
        for (expression, half, brain) in values {
            for (ty, kind, format, bits) in [
                (
                    "_Float16",
                    FloatKind::FLOAT16,
                    FloatingFormat::Binary16,
                    half,
                ),
                (
                    "__bf16",
                    FloatKind::BFloat16,
                    FloatingFormat::BFloat16,
                    brain,
                ),
            ] {
                let ArithmeticConstant::Floating(v) =
                    evaluate_arithmetic(u, &format!("({ty})({expression})")).unwrap()
                else {
                    panic!("float")
                };
                assert_eq!(
                    (v.kind(), v.format(), v.to_bits()),
                    (kind, format, bits),
                    "{profile:?}: {ty} {expression}"
                );
            }
        }
        for source in [
            "(int)(_Float16)__builtin_inf()",
            "(unsigned char)(__bf16)256",
            "(_Float16)65520.0",
        ] {
            assert!(evaluate_arithmetic(u, source).is_err(), "{source}");
        }
        assert!(
            matches!(evaluate_arithmetic(u,"(int)(__bf16)-3.75").unwrap(),ArithmeticConstant::Integer(v) if v.signed_value()==-3)
        );
        assert!(
            matches!(evaluate_arithmetic(u,"(double)(_Float16)1.00048828125").unwrap(),ArithmeticConstant::Floating(v) if v.to_bits()==0x3ff0_0000_0000_0000)
        );
    }
}

#[test]
fn constant_arithmetic_respects_the_compiler_excess_precision_boundary() {
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|profile| supports_narrow_types(*profile))
    {
        let a = check("", profile).unwrap();
        for source in [
            "(_Float16)1+(_Float16)0x1p-11-(_Float16)1",
            "(__bf16)1+(__bf16)0x1p-8-(__bf16)1",
            "1 ? 0.0f16 : (__bf16)0.0",
        ] {
            let result = evaluate_arithmetic(a.unit(), source);
            if profile.compiler() == Compiler::Gnu {
                assert!(result.unwrap_err().message.contains("excess precision"));
            } else {
                assert!(
                    matches!(result.unwrap(),ArithmeticConstant::Floating(v) if v.to_bits()==0)
                );
            }
        }
        let source = "_Float16 x=(_Float16)1+(_Float16)0x1p-11;";
        assert_eq!(
            check(source, profile).is_ok(),
            profile.compiler() == Compiler::Clang
        );
        assert!(evaluate_arithmetic(a.unit(), "(float)(_Float16)1 + (float)(__bf16)2").is_ok());
        let literal = evaluate_arithmetic(a.unit(), "(float)1.00048828125f16");
        if profile.compiler() == Compiler::Gnu {
            assert!(literal.unwrap_err().message.contains("excess precision"));
        } else {
            assert!(
                matches!(literal.unwrap(),ArithmeticConstant::Floating(v) if v.to_bits()==0x3f80_0000)
            );
        }
    }
}

#[test]
fn invalid_narrow_type_operations_keep_constraint_diagnostics() {
    for profile in CompilerProfile::ALL {
        for source in [
            "int __bf16;",
            "unsigned __bf16 n;",
            "struct S{_Float16 n:3;};",
            "_Float16 *a; __bf16 *b; void f(void){a=b;}",
            "void f(_Float16 x){(void)(x%2);}",
            "void f(__bf16 x){switch(x){}}",
        ] {
            assert!(check(source, profile).is_err(), "{profile:?}: {source}");
        }
        for source in ["__fp16 x;", "__bf16 x=1.0bf16;"] {
            assert!(check(source, profile).is_err());
        }
    }
}

#[test]
fn i686_rejects_narrow_scalar_types_but_accepts_ordinary_floats() {
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(Target::I686UnknownLinuxGnu, compiler).unwrap();
        for (name, message) in [
            ("_Float16", "_Float16 is unavailable on i686 GNU Linux"),
            ("__bf16", "__bf16 is unavailable on i686 GNU Linux"),
        ] {
            let source = format!("typedef {name} Narrow;");
            assert!(
                check(&source, profile)
                    .unwrap_err()
                    .message
                    .contains(message),
                "{profile:?}: {source}"
            );
        }
        check("typedef float Single; typedef double Double;", profile).unwrap();
    }
}

#[test]
#[ignore = "requires Clang's five target backends and native GNU GCC on Linux"]
fn declarations_and_constraints_match_compiler_profiles() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("half.c");
    std::fs::write(&path, SOURCE).unwrap();
    for profile in CompilerProfile::ALL {
        let mut command = if profile.compiler() == Compiler::Clang {
            let mut c = std::process::Command::new("clang");
            c.args(["-target", profile.target().triple()]);
            c
        } else if profile.target().triple()
            == format!("{}-unknown-linux-gnu", std::env::consts::ARCH)
            && cfg!(target_os = "linux")
        {
            std::process::Command::new(std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()))
        } else {
            continue;
        };
        let result = command
            .args(["-std=gnu11", "-fsyntax-only"])
            .arg(&path)
            .output()
            .unwrap();
        if !supports_narrow_types(profile) {
            let errors = String::from_utf8_lossy(&result.stderr);
            assert!(
                !result.status.success(),
                "{profile:?}: narrow scalars accepted"
            );
            assert!(
                errors.contains("_Float16") && errors.contains("__bf16"),
                "{errors}"
            );
            continue;
        }
        assert!(
            result.status.success(),
            "{profile:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
#[ignore = "requires native Linux GCC and Clang; compares C object bytes to APFloat"]
fn narrow_cast_bits_match_native_c_objects() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let target = match std::env::consts::ARCH {
        "x86_64" => toucan_target::Target::X86_64UnknownLinuxGnu,
        "aarch64" => toucan_target::Target::Aarch64UnknownLinuxGnu,
        _ => return,
    };
    let expressions = [
        "0.0",
        "-0.0",
        "1.0",
        "-2.0",
        "1.00048828125",
        "1.00390625",
        "65504.0",
        "0x1p-24",
        "0x1p-25",
        "0x1p-133",
        "0x1p-134",
        "__builtin_inf()",
        "-__builtin_inf()",
        "__builtin_nan(\"0\")",
    ];
    let directory = tempfile::tempdir().unwrap();
    let mut source = String::from("#include <stdio.h>\n#include <string.h>\n");
    for (ty, name) in [("_Float16", "h"), ("__bf16", "b")] {
        source.push_str(&format!("static const {ty} {name}[]={{"));
        for expression in expressions {
            source.push_str(&format!("({ty})({expression}),"));
        }
        source.push_str("};\n");
    }
    source.push_str("int main(void){unsigned short word;for(unsigned i=0;i<sizeof(h)/sizeof(*h);i++){memcpy(&word,&h[i],2);printf(\"%x \",word);}for(unsigned i=0;i<sizeof(b)/sizeof(*b);i++){memcpy(&word,&b[i],2);printf(\"%x \",word);}return 0;}");
    let input = directory.path().join("constants.c");
    std::fs::write(&input, source).unwrap();
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(target, compiler).unwrap();
        let a = check("", profile).unwrap();
        let expected = ["_Float16", "__bf16"]
            .into_iter()
            .flat_map(|ty| {
                expressions
                    .into_iter()
                    .map(move |expression| format!("({ty})({expression})"))
            })
            .map(|s| match evaluate_arithmetic(a.unit(), &s).unwrap() {
                ArithmeticConstant::Floating(v) => v.to_bits(),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        let cc = if compiler == Compiler::Gnu {
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into())
        } else {
            "clang".into()
        };
        let identity = std::process::Command::new(&cc)
            .arg("--version")
            .output()
            .unwrap();
        assert!(identity.status.success());
        if compiler == Compiler::Gnu {
            assert!(!String::from_utf8_lossy(&identity.stdout).contains("clang"));
        }
        for optimization in ["-O0", "-O2"] {
            let binary = directory.path().join("probe");
            let output = std::process::Command::new(&cc)
                .args(["-std=gnu11", optimization])
                .arg(&input)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = std::process::Command::new(binary).output().unwrap();
            assert!(output.status.success());
            let actual = String::from_utf8(output.stdout)
                .unwrap()
                .split_whitespace()
                .map(|s| u128::from_str_radix(s, 16).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{cc} {optimization}");
        }
    }
}

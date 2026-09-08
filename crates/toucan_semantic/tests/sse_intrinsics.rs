use toucan_semantic::checked::{Builtin, ExprKind, ImmediateStage, UseContext, X86Intrinsic};
use toucan_semantic::{
    Analysis, AnalysisOptions, FloatKind, IntegerKind, Type, TypeKind, analyze,
    analyze_with_options,
};
use toucan_target::Target;

fn intrinsics() -> impl Iterator<Item = X86Intrinsic> {
    include_str!("fixtures/sse_builtin_names.txt")
        .lines()
        .map(|name| X86Intrinsic::from_name(name).unwrap())
}
fn scalar(ty: &Type) -> &'static str {
    match ty.kind {
        TypeKind::Void => "void",
        TypeKind::Integer(IntegerKind::Char) => "char",
        TypeKind::Integer(IntegerKind::Short) => "short",
        TypeKind::Integer(IntegerKind::Int) => "int",
        TypeKind::Integer(IntegerKind::UnsignedInt) => "unsigned int",
        TypeKind::Integer(IntegerKind::LongLong) => "long long",
        TypeKind::Integer(IntegerKind::UnsignedLongLong) => "unsigned long long",
        TypeKind::Float(FloatKind::Float) => "float",
        TypeKind::Float(FloatKind::Double) => "double",
        _ => panic!("unexpected scalar {ty:?}"),
    }
}
fn spelling(ty: &Type) -> String {
    let base = match &ty.kind {
        TypeKind::Pointer(inner) => format!("{} *", spelling(inner)),
        TypeKind::Vector { element, lanes, .. } => {
            format!("v{lanes}_{}", scalar(element).replace(' ', "_"))
        }
        _ => scalar(ty).to_owned(),
    };
    if ty.qualifiers.is_const {
        format!("const {base}")
    } else {
        base
    }
}
fn prelude() -> String {
    let mut source = String::new();
    for (name, size) in [
        ("char", 1),
        ("short", 2),
        ("int", 4),
        ("long long", 8),
        ("float", 4),
        ("double", 8),
    ] {
        for bytes in [8, 16] {
            source.push_str(&format!(
                "typedef {name} v{}_{} __attribute__((vector_size({bytes})));\n",
                bytes / size,
                name.replace(' ', "_")
            ));
        }
    }
    source
}
fn source(profile: Target, exact_gcc: bool, unavailable: bool) -> String {
    let mut source = prelude();
    for (index, intrinsic) in intrinsics().enumerate() {
        let signature = intrinsic.signature(profile);
        if signature.is_none() != unavailable {
            continue;
        }
        let signature = signature
            .unwrap_or_else(|| intrinsic.signature(Target::X86_64UnknownLinuxGnu).unwrap());
        let parameters = signature
            .parameters()
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{} a{i}", spelling(t)))
            .collect::<Vec<_>>()
            .join(",");
        let arguments = signature
            .parameters()
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if intrinsic
                    .immediate_constraints(profile)
                    .iter()
                    .any(|c| c.argument() == i)
                {
                    "0".into()
                } else {
                    format!("a{i}")
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        let call = format!("{}({arguments})", intrinsic.name());
        let parameters = if parameters.is_empty() {
            "void"
        } else {
            &parameters
        };
        source.push_str(&format!(
            "{} f{index}({parameters}){{",
            spelling(signature.result())
        ));
        if !matches!(signature.result().kind, TypeKind::Void) && !unavailable {
            source.push_str(&format!(
                "_Static_assert(_Generic({call},{}:1,default:0),\"result type\");",
                spelling(signature.result())
            ));
        }
        source.push_str(&format!(
            "{}{call};}}\n",
            if matches!(signature.result().kind, TypeKind::Void) {
                ""
            } else {
                "return "
            }
        ));
        if exact_gcc {
            let parameters = signature
                .parameters()
                .iter()
                .map(spelling)
                .collect::<Vec<_>>()
                .join(",");
            let parameters = if parameters.is_empty() {
                "void"
            } else {
                &parameters
            };
            source.push_str(&format!("_Static_assert(__builtin_types_compatible_p(__typeof__({}),{}({parameters})),\"formal signature\");\n",intrinsic.name(),spelling(signature.result())));
        }
    }
    source
}
fn check(source: &str, target: Target) -> Result<Analysis, toucan_semantic::Error> {
    let ordinary = analyze(source, target);
    let retained = analyze_with_options(
        source,
        target,
        &AnalysisOptions {
            retain_code: true,
            ..AnalysisOptions::default()
        },
    );
    match (&ordinary, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{a:?}"), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => assert_eq!((&a.message, a.offset), (&b.message, b.offset)),
        _ => panic!("retention changed acceptance: {ordinary:?}, {retained:?}"),
    }
    retained
}
#[test]
fn sse_signatures_and_compiler_availability_are_retained() {
    assert_eq!(intrinsics().count(), 228);
    for (target, expected) in [
        (Target::X86_64UnknownLinuxGnu, 228),
        (Target::X86_64AppleDarwin, 154),
        (Target::X86_64PcWindowsMsvc, 154),
    ] {
        assert_eq!(
            intrinsics()
                .filter(|i| i.signature(target).is_some())
                .count(),
            expected
        );
        let source = source(target, false, false);
        let analysis = check(&source, target).unwrap();
        let code = analysis.checked().unwrap();
        for (_, expression) in code.expressions() {
            if let ExprKind::BuiltinCall {
                builtin: Builtin::X86(intrinsic),
                arguments,
                ..
            } = expression.kind()
            {
                let signature = intrinsic.signature(target).unwrap();
                assert_eq!(code.ty(expression.ty()).unwrap(), signature.result());
                for (argument, parameter) in arguments.iter().zip(signature.parameters()) {
                    assert_eq!(argument.context(), UseContext::Value);
                    assert_eq!(code.ty(argument.effective_type()).unwrap(), parameter);
                }
            }
        }
        if target != Target::X86_64UnknownLinuxGnu {
            let error = check(&source_for_unavailable(target), target).unwrap_err();
            assert!(error.message.contains("unavailable"));
        }
    }
    for target in [Target::Aarch64UnknownLinuxGnu, Target::Aarch64AppleDarwin] {
        let error = check("void f(void){__builtin_ia32_sfence();}", target).unwrap_err();
        assert!(error.message.contains("x86-64 target"));
    }
}
fn source_for_unavailable(target: Target) -> String {
    source(target, false, true)
}

#[test]
fn byte_shift_and_shuffle_requirements_preserve_encoding_rules() {
    let prefetch =
        X86Intrinsic::Prefetch.conditional_immediate_constraints(Target::X86_64UnknownLinuxGnu)[0];
    assert_eq!(prefetch.condition(), (3, 1));
    assert_eq!(
        (
            prefetch.requirement().argument(),
            prefetch.requirement().minimum(),
            prefetch.requirement().maximum()
        ),
        (2, 2, 3)
    );
    let shift = X86Intrinsic::Pslldqi128.immediate_constraints(Target::X86_64UnknownLinuxGnu)[0];
    assert_eq!(
        (
            shift.minimum(),
            shift.maximum(),
            shift.multiple_of(),
            shift.stage()
        ),
        (0, 2040, 8, ImmediateStage::AfterInlining)
    );
    let source = format!(
        "{}void f(v4_float f,v2_double d,v4_short s){{__builtin_ia32_pshufw(s,256);__builtin_ia32_shufps(f,f,255);__builtin_ia32_shufpd(d,d,3);}}",
        prelude()
    );
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::X86_64AppleDarwin,
        Target::X86_64PcWindowsMsvc,
    ] {
        check(&source, target).unwrap();
    }
    for (name, ty, args) in [
        ("shufps", "v4_float", "v,v,256"),
        ("shufpd", "v2_double", "v,v,4"),
        ("pshufw", "v4_short", "v,1.0"),
    ] {
        let source = format!(
            "{}void f({ty} v){{__builtin_ia32_{name}({args});}}",
            prelude()
        );
        check(&source, Target::X86_64UnknownLinuxGnu).unwrap();
        for target in [Target::X86_64AppleDarwin, Target::X86_64PcWindowsMsvc] {
            assert!(check(&source, target).is_err());
        }
    }
}
fn compiler_input(command: &mut std::process::Command, source: &str) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
#[test]
#[ignore = "requires genuine GCC and Clang with all five target backends; run with --include-ignored"]
fn all_sse_formal_signatures_match_gcc_and_cross_target_clang() {
    use std::process::Command;
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let architecture = Command::new(&gcc).arg("-dumpmachine").output().unwrap();
    if String::from_utf8_lossy(&architecture.stdout).starts_with("x86_64") {
        let source = source(Target::X86_64UnknownLinuxGnu, true, false);
        let temp = tempfile::tempdir().unwrap();
        let output = compiler_input(
            Command::new(&gcc)
                .args([
                    "-std=gnu11",
                    "-pedantic-errors",
                    "-O2",
                    "-c",
                    "-x",
                    "c",
                    "-",
                    "-o",
                ])
                .arg(temp.path().join("sse.o")),
            &source,
        );
        assert!(
            output.status.success(),
            "GCC: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for target in Target::ALL {
        let source = source(Target::X86_64AppleDarwin, false, false);
        let supported = matches!(
            target,
            Target::X86_64UnknownLinuxGnu | Target::X86_64AppleDarwin | Target::X86_64PcWindowsMsvc
        );
        let temp = tempfile::tempdir().unwrap();
        let output = compiler_input(
            Command::new("clang")
                .args([
                    "-target",
                    target.triple(),
                    "-std=gnu11",
                    "-pedantic-errors",
                    "-O2",
                    "-c",
                    "-x",
                    "c",
                    "-",
                    "-o",
                ])
                .arg(temp.path().join("sse.o")),
            &source,
        );
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(supported),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[ignore = "executes compiler reference code on native x86-64; run with --include-ignored"]
fn native_sse_arithmetic_masks_and_memory_operations() {
    use std::process::Command;
    if std::env::consts::ARCH != "x86_64" {
        return;
    }
    let source = format!(
        r#"{}
int main(void){{
    v4_float a={{1,4,9,16}}, b={{20,30,40,50}};
    v4_float roots=__builtin_ia32_sqrtps(a);
    for(int i=0;i<4;i++)if(roots[i]!=(float)(i+1))return 1;
    v4_float selected=__builtin_ia32_shufps(a,b,27);
    if(selected[0]!=16 || selected[1]!=9 || selected[2]!=30 || selected[3]!=20)return 2;
    v4_int mask=(v4_int)__builtin_ia32_cmpltps(a,b);
    for(int i=0;i<4;i++)if(mask[i]!=-1)return 3;
    v4_int integers=__builtin_ia32_cvttps2dq((v4_float){{1.9f,-2.9f,0,123.9f}});
    if(integers[0]!=1 || integers[1]!=-2 || integers[2]!=0 || integers[3]!=123)return 4;
    v8_short packed=__builtin_ia32_packssdw128((v4_int){{32768,-32769,3,4}},(v4_int){{5,6,7,8}});
    if(packed[0]!=32767 || packed[1]!=-32768 || packed[7]!=8)return 5;
    v16_char bytes,write_mask;
    char output[16]={{0}};
    for(int i=0;i<16;i++){{bytes[i]=(char)(i+1);write_mask[i]=i%2?(char)0x80:0;}}
    __builtin_ia32_maskmovdqu(bytes,write_mask,output);
    __builtin_ia32_sfence();
    for(int i=0;i<16;i++)if(output[i]!=(i%2?i+1:0))return 6;
    int stored=0;
    __builtin_ia32_movnti(&stored,42);
    __builtin_ia32_mfence();
    if(stored!=42)return 7;
    unsigned int control=__builtin_ia32_stmxcsr();
    __builtin_ia32_ldmxcsr(control);
    return 0;
}}
"#,
        prelude()
    );
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::X86_64AppleDarwin,
        Target::X86_64PcWindowsMsvc,
    ] {
        check(&source, target).unwrap();
    }
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [&gcc, "clang"] {
        for optimization in ["-O0", "-O2"] {
            let temp = tempfile::tempdir().unwrap();
            let binary = temp
                .path()
                .join(if cfg!(windows) { "sse.exe" } else { "sse" });
            let output = compiler_input(
                Command::new(compiler)
                    .args(["-std=gnu11", optimization, "-x", "c", "-", "-o"])
                    .arg(&binary),
                &source,
            );
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                Command::new(binary).status().unwrap().success(),
                "{compiler} {optimization}"
            );
        }
    }
}

#[test]
#[ignore = "requires native x86-64 GCC and Clang; run with --include-ignored"]
fn deferred_immediate_boundaries_match_compiler_lowering() {
    use std::process::Command;
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let architecture = Command::new(&gcc).arg("-dumpmachine").output().unwrap();
    let native_x86 = String::from_utf8_lossy(&architecture.stdout).starts_with("x86_64");
    for (body, gnu_accepts, clang_accepts) in [
        (
            "v2_long_long f(v2_long_long v){return __builtin_ia32_pslldqi128(v,8);}",
            true,
            false,
        ),
        (
            "v2_long_long f(v2_long_long v){return __builtin_ia32_pslldqi128(v,2040);}",
            true,
            false,
        ),
        (
            "v2_long_long f(v2_long_long v){return __builtin_ia32_pslldqi128(v,4294967304LL);}",
            true,
            false,
        ),
        (
            "v2_long_long f(v2_long_long v){return __builtin_ia32_pslldqi128(v,7);}",
            false,
            false,
        ),
        (
            "v2_long_long f(v2_long_long v){return __builtin_ia32_pslldqi128(v,-8);}",
            false,
            false,
        ),
        (
            "v2_long_long f(v2_long_long v){return __builtin_ia32_pslldqi128(v,2048);}",
            false,
            false,
        ),
        (
            "v4_float f(v4_float v){return __builtin_ia32_shufps(v,v,256);}",
            true,
            false,
        ),
        (
            "v2_double f(v2_double v){return __builtin_ia32_shufpd(v,v,4);}",
            true,
            false,
        ),
        (
            "v4_short f(v4_short v){return __builtin_ia32_pshufw(v,256);}",
            true,
            true,
        ),
        (
            "void f(void*p){__builtin_ia32_prefetch(p,0,0,1);}",
            false,
            false,
        ),
        (
            "void f(void*p){__builtin_ia32_prefetch(p,0,2,1);}",
            true,
            false,
        ),
        (
            "void f(void*p,int n){__builtin_ia32_prefetch(p,0,n,0);}",
            false,
            false,
        ),
    ] {
        let source = format!("{}{body}\n", prelude());
        // GNU source acceptance deliberately leaves the lowering obligations pending.
        check(&source, Target::X86_64UnknownLinuxGnu).unwrap();
        if native_x86 {
            let temp = tempfile::tempdir().unwrap();
            let output = compiler_input(
                Command::new(&gcc)
                    .args(["-std=gnu11", "-O2", "-c", "-x", "c", "-", "-o"])
                    .arg(temp.path().join("probe.o")),
                &source,
            );
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(gnu_accepts),
                "GCC {body}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        for target in [
            Target::X86_64UnknownLinuxGnu,
            Target::X86_64AppleDarwin,
            Target::X86_64PcWindowsMsvc,
        ] {
            let output = compiler_input(
                Command::new("clang").args([
                    "-target",
                    target.triple(),
                    "-std=gnu11",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
                ]),
                &source,
            );
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(clang_accepts),
                "{target} {body}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

use toucan_semantic::checked::{
    Builtin, Conversion, ExprKind, ImmediateStage, UseContext, X86Feature, X86Intrinsic,
};
use toucan_semantic::{
    Analysis, AnalysisOptions, IntegerKind, Type, TypeKind, analyze, analyze_with_options,
};
use toucan_target::Target;

const INTRINSICS: &[X86Intrinsic] = &[
    X86Intrinsic::Emms,
    X86Intrinsic::Packssdw,
    X86Intrinsic::Packsswb,
    X86Intrinsic::Packuswb,
    X86Intrinsic::Paddb,
    X86Intrinsic::Paddd,
    X86Intrinsic::Paddq,
    X86Intrinsic::Paddsb,
    X86Intrinsic::Paddsw,
    X86Intrinsic::Paddusb,
    X86Intrinsic::Paddusw,
    X86Intrinsic::Paddw,
    X86Intrinsic::Pand,
    X86Intrinsic::Pandn,
    X86Intrinsic::Pcmpeqb,
    X86Intrinsic::Pcmpeqd,
    X86Intrinsic::Pcmpeqw,
    X86Intrinsic::Pcmpgtb,
    X86Intrinsic::Pcmpgtd,
    X86Intrinsic::Pcmpgtw,
    X86Intrinsic::Pmaddwd,
    X86Intrinsic::Pmulhw,
    X86Intrinsic::Pmullw,
    X86Intrinsic::Por,
    X86Intrinsic::Pslld,
    X86Intrinsic::Pslldi,
    X86Intrinsic::Psllq,
    X86Intrinsic::Psllqi,
    X86Intrinsic::Psllw,
    X86Intrinsic::Psllwi,
    X86Intrinsic::Psrad,
    X86Intrinsic::Psradi,
    X86Intrinsic::Psraw,
    X86Intrinsic::Psrawi,
    X86Intrinsic::Psrld,
    X86Intrinsic::Psrldi,
    X86Intrinsic::Psrlq,
    X86Intrinsic::Psrlqi,
    X86Intrinsic::Psrlw,
    X86Intrinsic::Psrlwi,
    X86Intrinsic::Psubb,
    X86Intrinsic::Psubd,
    X86Intrinsic::Psubq,
    X86Intrinsic::Psubsb,
    X86Intrinsic::Psubsw,
    X86Intrinsic::Psubusb,
    X86Intrinsic::Psubusw,
    X86Intrinsic::Psubw,
    X86Intrinsic::Punpckhbw,
    X86Intrinsic::Punpckhdq,
    X86Intrinsic::Punpckhwd,
    X86Intrinsic::Punpcklbw,
    X86Intrinsic::Punpckldq,
    X86Intrinsic::Punpcklwd,
    X86Intrinsic::Pxor,
    X86Intrinsic::VecExtV2si,
    X86Intrinsic::VecInitV2si,
    X86Intrinsic::VecInitV4hi,
    X86Intrinsic::VecInitV8qi,
];
const TYPES: &str = "typedef char V8C __attribute__((vector_size(8)));
    typedef short V4S __attribute__((vector_size(8)));
    typedef int V2I __attribute__((vector_size(8)));
    typedef long long V1L __attribute__((vector_size(8)));
";
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
        (Err(a), Err(b)) => {
            assert_eq!(a.message, b.message);
            assert_eq!(a.offset, b.offset);
        }
        _ => panic!("retention changed acceptance: {ordinary:?} {retained:?}"),
    }
    retained
}
fn spelling(ty: &Type) -> &'static str {
    match &ty.kind {
        TypeKind::Void => "void",
        TypeKind::Integer(IntegerKind::Char) => "char",
        TypeKind::Integer(IntegerKind::Short) => "short",
        TypeKind::Integer(IntegerKind::Int) => "int",
        TypeKind::Vector { element, lanes, .. } => match (&element.kind, lanes) {
            (TypeKind::Integer(IntegerKind::Char), 8) => "V8C",
            (TypeKind::Integer(IntegerKind::Short), 4) => "V4S",
            (TypeKind::Integer(IntegerKind::Int), 2) => "V2I",
            (TypeKind::Integer(IntegerKind::LongLong), 1) => "V1L",
            _ => panic!("unexpected vector {ty:?}"),
        },
        _ => panic!("unexpected intrinsic type {ty:?}"),
    }
}
fn signature_source(profile: Target) -> String {
    let mut source = TYPES.to_owned();
    for (index, intrinsic) in INTRINSICS.iter().enumerate() {
        let signature = intrinsic.signature(profile).unwrap();
        let parameters = signature
            .parameters()
            .iter()
            .enumerate()
            .map(|(index, ty)| format!("{} a{index}", spelling(ty)))
            .collect::<Vec<_>>()
            .join(",");
        let arguments = signature
            .parameters()
            .iter()
            .enumerate()
            .map(|(index, _)| {
                if intrinsic
                    .immediate_constraints(profile)
                    .iter()
                    .any(|c| c.argument() == index)
                {
                    "1".to_owned()
                } else {
                    format!("a{index}")
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
        source.push_str(&format!("void f{index}({parameters}){{"));
        if matches!(signature.result().kind, TypeKind::Void) {
            source.push_str(&format!("{call};"));
        } else {
            source.push_str(&format!(
                "_Static_assert(_Generic({call},{}:1,default:0),\"result type\");{call};",
                spelling(signature.result())
            ));
        }
        source.push_str("}\n");
    }
    source
}
#[test]
fn exact_mmx_signatures_and_retained_operands() {
    for target in Target::ALL {
        if X86Intrinsic::Emms.signature(target).is_none() {
            let err = check("void f(void){__builtin_ia32_emms();}", target).unwrap_err();
            assert!(err.message.contains("x86-64 target"));
            continue;
        }
        let analysis = check(&signature_source(target), target).unwrap();
        let code = analysis.checked().unwrap();
        let mut seen = vec![];
        for (_, expression) in code.expressions() {
            let ExprKind::BuiltinCall {
                builtin: Builtin::X86(intrinsic),
                arguments,
                ..
            } = expression.kind()
            else {
                continue;
            };
            seen.push(*intrinsic);
            let signature = intrinsic.signature(target).unwrap();
            assert_eq!(code.ty(expression.ty()).unwrap(), signature.result());
            assert_eq!(arguments.len(), signature.parameters().len());
            for (argument, parameter) in arguments.iter().zip(signature.parameters()) {
                assert_eq!(argument.context(), UseContext::Value);
                assert_eq!(code.ty(argument.effective_type()).unwrap(), parameter);
            }
            assert!(intrinsic.required_features().contains(&X86Feature::Mmx));
        }
        for intrinsic in INTRINSICS {
            assert!(seen.contains(intrinsic), "{intrinsic:?}");
        }
        assert_eq!(
            X86Intrinsic::Paddq.required_features(),
            &[X86Feature::Mmx, X86Feature::Sse2]
        );
    }
}
#[test]
fn immediate_checks_preserve_compiler_stage_and_parameter_conversion() {
    let expressions = [
        ("1", true),
        ("0x100000001LL", true),
        ("__builtin_bswap32(0)", true),
        ("__builtin_ctz(2.0)", true),
        ("__builtin_bswap32(1.5)==0", true),
        ("(int)1.5", true),
        ("n", false),
        ("2", false),
        ("-1", false),
        ("1.0", false),
        ("(n++,1)", false),
        ("(int)(0.5+0.5)", false),
    ];
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::X86_64AppleDarwin,
        Target::X86_64PcWindowsMsvc,
    ] {
        let gnu = matches!(
            target,
            Target::X86_64UnknownLinuxGnu | Target::X86_64UnknownLinuxMusl
        );
        let constraint = X86Intrinsic::VecExtV2si.immediate_constraints(target)[0];
        assert_eq!(
            (
                constraint.argument(),
                constraint.minimum(),
                constraint.maximum()
            ),
            (1, 0, 1)
        );
        assert_eq!(
            constraint.stage(),
            if gnu {
                ImmediateStage::AfterInlining
            } else {
                ImmediateStage::Frontend
            }
        );
        for (expr, valid) in expressions {
            let source = format!(
                "{TYPES}int f(V2I v,int n){{return __builtin_ia32_vec_ext_v2si(v,{expr});}}\n"
            );
            let result = check(&source, target);
            assert_eq!(result.is_ok(), gnu || valid, "{target}: {expr}: {result:?}");
        }
        // Shift-count builtins ending in i still accept runtime counts.
        check(
            &format!("{TYPES}V2I f(V2I v,int n){{return __builtin_ia32_pslldi(v,n);}}"),
            target,
        )
        .unwrap();
    }
}
#[test]
fn intrinsic_conversions_and_invalid_calls_are_explicit() {
    let source = format!(
        "{TYPES}typedef float F __attribute__((vector_size(8)));void f(F v){{__builtin_ia32_paddd(v,v);}}"
    );
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::X86_64AppleDarwin,
        Target::X86_64PcWindowsMsvc,
    ] {
        let result = check(&source, target);
        if matches!(
            target,
            Target::X86_64UnknownLinuxGnu | Target::X86_64UnknownLinuxMusl
        ) {
            assert!(result.is_err());
        } else {
            let analysis = result.unwrap();
            let code = analysis.checked().unwrap();
            let (_, expression) = code
                .expressions()
                .find(|(_, e)| matches!(e.kind(), ExprKind::BuiltinCall { .. }))
                .unwrap();
            let ExprKind::BuiltinCall { arguments, .. } = expression.kind() else {
                unreachable!()
            };
            assert!(arguments.iter().all(|a| {
                a.conversions()
                    .iter()
                    .any(|c| c.kind() == Conversion::IntrinsicArgument)
            }));
        }
        for source in [
            "void f(void){__builtin_ia32_emms(0);}",
            "void f(void){__builtin_ia32_paddd();}",
            "void f(void){__builtin_ia32_paddd(1,2);}",
            "void f(void){__builtin_ia32_not_a_supported_intrinsic();}",
            "void*f(void){return &__builtin_ia32_emms;}",
        ] {
            assert!(check(source, target).is_err(), "{target}: {source}");
        }
        check(
            "int f(int(*__builtin_ia32_emms)(int)){return __builtin_ia32_emms(1);}",
            target,
        )
        .unwrap();
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
#[ignore = "requires native x86-64 GCC and Clang with all five targets; run with --include-ignored"]
fn mmx_signatures_match_compiler_descriptors() {
    use std::process::Command;
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        !String::from_utf8_lossy(&identity.stdout).contains("clang"),
        "set TOUCAN_GCC to genuine GNU GCC"
    );
    let architecture = Command::new(&gcc).arg("-dumpmachine").output().unwrap();
    if String::from_utf8_lossy(&architecture.stdout).starts_with("x86_64") {
        let mut source = signature_source(Target::X86_64UnknownLinuxGnu);
        for intrinsic in INTRINSICS {
            let signature = intrinsic.signature(Target::X86_64UnknownLinuxGnu).unwrap();
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
        let output = compiler_input(
            Command::new(&gcc).args([
                "-std=gnu11",
                "-pedantic-errors",
                "-fsyntax-only",
                "-x",
                "c",
                "-",
            ]),
            &source,
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for target in Target::ALL {
        let supported = X86Intrinsic::Emms.signature(target).is_some();
        let source = if supported {
            signature_source(Target::X86_64AppleDarwin)
        } else {
            "void f(void){__builtin_ia32_emms();}\n".to_owned()
        };
        let output = compiler_input(
            Command::new("clang").args([
                "-target",
                target.triple(),
                "-std=gnu11",
                "-pedantic-errors",
                "-fsyntax-only",
                "-x",
                "c",
                "-",
            ]),
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
#[ignore = "requires native x86-64 GCC and Clang; run with --include-ignored"]
fn immediate_diagnostics_match_frontend_and_lowering_stages() {
    use std::process::Command;
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let architecture = Command::new(&gcc).arg("-dumpmachine").output().unwrap();
    let native_x86 = String::from_utf8_lossy(&architecture.stdout).starts_with("x86_64");
    for (expression, clang_accepts, gcc_lowering_accepts) in [
        ("1", true, true),
        ("0x100000001LL", true, true),
        ("__builtin_bswap32(1.5)==0", true, true),
        ("__builtin_ctz(2.0)", true, true),
        ("n", false, false),
        ("2", false, false),
        ("-1", false, false),
        ("1.0", false, true),
        ("(n++,1)", false, true),
        ("(int)(0.5+0.5)", false, true),
    ] {
        let source = format!(
            "{TYPES}int f(V2I v,int n){{return __builtin_ia32_vec_ext_v2si(v,{expression});}}\n"
        );
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
                "{target} {expression}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        if native_x86 {
            let output = compiler_input(
                Command::new(&gcc).args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"]),
                &source,
            );
            assert!(
                output.status.success(),
                "GCC syntax: {expression}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let temp = tempfile::tempdir().unwrap();
            for optimization in ["-O0", "-O2"] {
                let output = compiler_input(
                    Command::new(&gcc)
                        .args(["-std=gnu11", optimization, "-c", "-x", "c", "-", "-o"])
                        .arg(temp.path().join("probe.o")),
                    &source,
                );
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&output),
                    Ok(gcc_lowering_accepts),
                    "GCC {optimization} {expression}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

#[test]
#[ignore = "executes compiler reference code on native x86-64; run with --include-ignored"]
fn native_mmx_reference_operations() {
    use std::process::Command;
    if std::env::consts::ARCH != "x86_64" {
        return;
    }
    let source = format!(
        r#"{TYPES}
int run(int count) {{
    V8C bytes=__builtin_ia32_packsswb((V4S){{128,-129,127,-128}},(V4S){{0,256,-256,1}});
    unsigned char expected[8]={{127,128,127,128,0,127,128,1}};
    for(int i=0;i<8;i++) if((unsigned char)bytes[i]!=expected[i]) return 1;
    V2I products=__builtin_ia32_pmaddwd((V4S){{2,3,-2,-3}},(V4S){{5,7,5,7}});
    if(products[0]!=31 || products[1]!=-31) return 2;
    V2I shifted=__builtin_ia32_pslldi((V2I){{1,-1}},count);
    if(shifted[0]!=2 || shifted[1]!=-2) return 3;
    V2I mask=__builtin_ia32_pcmpeqd((V2I){{1,2}},(V2I){{1,3}});
    if(mask[0]!=-1 || mask[1]!=0) return 4;
    V2I packed=__builtin_ia32_vec_init_v2si(12,34);
    if(__builtin_ia32_vec_ext_v2si(packed,1)!=34) return 5;
    __builtin_ia32_emms();
    return 0;
}}
int main(void){{return run(1);}}
"#
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
                .join(if cfg!(windows) { "probe.exe" } else { "probe" });
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
fn query_effects_and_argument_scopes_survive_retention() {
    use toucan_semantic::checked::{QueryEvaluation, QuerySideEffects, QuerySuppression};
    let source = format!(
        r#"{TYPES}
void f(V2I value,volatile V2I *pointer){{
    __builtin_constant_p(__builtin_ia32_vec_ext_v2si(value,1));
    __builtin_constant_p(__builtin_ia32_vec_ext_v2si(*pointer,1));
    __builtin_constant_p((__builtin_ia32_emms(),0));
}}
int g(void){{return __builtin_ia32_vec_ext_v2si(({{struct S{{int x;}};V2I value={{1,2}};value;}}),1);}}
"#
    );
    for target in [Target::X86_64AppleDarwin, Target::X86_64PcWindowsMsvc] {
        let analysis = check(&source, target).unwrap();
        let code = analysis.checked().unwrap();
        let queries = code
            .expressions()
            .filter_map(|(_, expr)| {
                if let ExprKind::BuiltinCall {
                    builtin: Builtin::ConstantQuery,
                    query_evaluation,
                    ..
                } = expr.kind()
                {
                    *query_evaluation
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            queries,
            [
                QueryEvaluation::ClangFallback {
                    side_effects: QuerySideEffects::Absent
                },
                QueryEvaluation::Unevaluated(QuerySuppression::OrdinarySideEffects),
                QueryEvaluation::Unevaluated(QuerySuppression::OrdinarySideEffects),
            ]
        );
    }
    check(&source, Target::X86_64UnknownLinuxGnu).unwrap();
}

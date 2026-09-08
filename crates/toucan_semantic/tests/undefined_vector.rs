use toucan_semantic::{
    AnalysisOptions, FloatKind, TypeKind, analyze_with_profile,
    checked::{Builtin, ExprKind, QueryEvaluation, QuerySideEffects, X86Intrinsic},
};
use toucan_target::{Compiler, CompilerProfile, Target};

fn supported(profile: CompilerProfile) -> bool {
    profile.compiler() == Compiler::Clang
        && matches!(
            profile.target(),
            Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::X86_64AppleDarwin
                | Target::X86_64PcWindowsMsvc
        )
}

const VALID: &[&str] = &[
    "typedef double V __attribute__((vector_size(16))); _Static_assert(_Generic(__builtin_ia32_undef128(),V:1,default:0),\"type\"); _Static_assert(sizeof(__builtin_ia32_undef128())==16,\"size\");",
    "typedef double V __attribute__((vector_size(16))); V f(void){return __builtin_ia32_undef128();}",
    "__attribute__((target(\"no-mmx\"))) void f(void){(void)__builtin_ia32_undef128();}",
    "_Static_assert(!__builtin_constant_p(__builtin_ia32_undef128()),\"not a C constant\");",
    "typedef double V __attribute__((vector_size(16))); V f(V a){return __builtin_shufflevector(a,__builtin_ia32_undef128(),0,1);}",
    "int f(void){return _Generic(__builtin_ia32_undef128(),default:1);}",
];
const INVALID: &[&str] = &[
    "void f(void){(void)__builtin_ia32_undef128(0);}",
    "void f(void){(void)&__builtin_ia32_undef128;}",
    "typedef double V __attribute__((vector_size(16))); V x=__builtin_ia32_undef128();",
    "_Static_assert(__builtin_ia32_undef128()[0]==0,\"not an ICE\");",
];

#[test]
fn profiles_and_constant_contexts_keep_ordinary_retained_parity() {
    for profile in CompilerProfile::ALL {
        for (source, valid) in VALID
            .iter()
            .map(|s| (*s, supported(profile)))
            .chain(INVALID.iter().map(|s| (*s, false)))
        {
            let plain = analyze_with_profile(source, profile, &Default::default());
            let kept = analyze_with_profile(
                source,
                profile,
                &AnalysisOptions {
                    retain_code: true,
                    ..Default::default()
                },
            );
            assert_eq!(plain.is_ok(), valid, "{profile:?} {source}: {plain:?}");
            match (plain, kept) {
                (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
                (Err(a), Err(b)) => assert_eq!((a.offset, a.message), (b.offset, b.message)),
                (a, b) => panic!("retention changed result: {a:?} {b:?}"),
            }
        }
    }
}

#[test]
fn retained_identity_preserves_an_unspecified_value_without_an_isa_requirement() {
    let intrinsic = X86Intrinsic::from_name("__builtin_ia32_undef128").unwrap();
    assert_eq!(intrinsic, X86Intrinsic::Undef128);
    assert!(intrinsic.has_unspecified_result());
    assert!(!X86Intrinsic::Emms.has_unspecified_result());
    assert!(!intrinsic.has_side_effects());
    assert!(intrinsic.required_features().is_empty());
    for profile in CompilerProfile::ALL {
        let signature = intrinsic.signature_with_profile(profile);
        assert_eq!(signature.is_some(), supported(profile));
        let Some(signature) = signature else { continue };
        assert!(signature.parameters().is_empty());
        assert!(
            matches!(&signature.result().kind, TypeKind::Vector { element, lanes: 2, .. } if matches!(element.kind, TypeKind::Float(FloatKind::Double)))
        );
        let analysis = analyze_with_profile(
            VALID[1],
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let code = analysis.checked().unwrap();
        let calls = code
            .expressions()
            .filter(|(_, e)| {
                matches!(e.kind(), ExprKind::BuiltinCall { builtin: Builtin::X86(X86Intrinsic::Undef128), arguments, .. } if arguments.is_empty())
            })
            .count();
        assert_eq!(calls, 1);
    }
}

#[test]
fn query_side_effect_gate_does_not_mistake_unspecified_for_effectful() {
    let source = "int bump(void); unsigned long f(void){return __builtin_object_size((int(*)[bump()])(unsigned long long)__builtin_ia32_undef128()[0],0);}";
    for profile in CompilerProfile::ALL.into_iter().filter(|p| supported(*p)) {
        let analysis = analyze_with_profile(
            source,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let query = analysis
            .checked()
            .unwrap()
            .expressions()
            .find_map(|(_, e)| match e.kind() {
                ExprKind::BuiltinCall {
                    builtin: Builtin::ObjectSize,
                    query_evaluation,
                    ..
                } => *query_evaluation,
                _ => None,
            })
            .unwrap();
        assert_eq!(
            query,
            QueryEvaluation::ClangFallback {
                side_effects: QuerySideEffects::Absent
            }
        );
    }
}

#[test]
#[ignore = "requires native GCC and Clang; cross-target checks use Clang"]
fn compiler_constraints_and_defined_lane_execution() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("undefined.c");
    let output = directory.path().join("undefined.s");
    for profile in CompilerProfile::ALL {
        let host_gnu = cfg!(target_os = "linux")
            && ((cfg!(target_arch = "x86_64")
                && matches!(
                    profile.target(),
                    Target::X86_64UnknownLinuxGnu | Target::X86_64UnknownLinuxMusl
                ))
                || (cfg!(target_arch = "aarch64")
                    && matches!(
                        profile.target(),
                        Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
                    )));
        let mut compiler = if profile.compiler() == Compiler::Gnu {
            if !host_gnu {
                continue;
            }
            std::process::Command::new(std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()))
        } else {
            let mut command = std::process::Command::new("clang");
            command.args(["-target", profile.target().triple()]);
            command
        };
        compiler
            .args(["-std=gnu11", "-Werror=implicit-function-declaration", "-S"])
            .arg(&input)
            .arg("-o")
            .arg(&output);
        for (source, valid) in VALID
            .iter()
            .map(|s| (*s, supported(profile)))
            .chain(INVALID.iter().map(|s| (*s, false)))
        {
            std::fs::write(&input, source).unwrap();
            let result = compiler.output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&result).unwrap(),
                valid,
                "{profile:?} {source}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
    if cfg!(target_arch = "x86_64") {
        // Examine only selected, defined lanes. No particular unspecified bits are required.
        std::fs::write(&input, "typedef double V __attribute__((vector_size(16))); int main(void){V a={1.25,8.5}; V b=__builtin_shufflevector(a,__builtin_ia32_undef128(),1,0); return b[0]!=8.5 || b[1]!=1.25;}").unwrap();
        for optimization in ["-O0", "-O2"] {
            let executable = directory.path().join("undefined-vector.exe");
            let result = std::process::Command::new("clang")
                .args(["-std=gnu11", optimization])
                .arg(&input)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(
                std::process::Command::new(&executable)
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
}

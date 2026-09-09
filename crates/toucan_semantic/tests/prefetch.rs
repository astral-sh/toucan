use toucan_semantic::{
    Analysis, AnalysisOptions, BuiltinFunction, IntegerKind, PrefetchHint, TypeKind,
    analyze_with_profile,
    checked::{Builtin, Conversion, ExprKind, ImmediateStage, UseContext},
};
use toucan_target::{Compiler, CompilerProfile, LanguageMode, Target};

fn profiles() -> impl Iterator<Item = CompilerProfile> {
    CompilerProfile::ALL
        .into_iter()
        .flat_map(|profile| LanguageMode::ALL.map(|mode| profile.with_language_mode(mode)))
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

#[test]
fn prefetch_keeps_promoted_types_and_address_effects() {
    for profile in profiles() {
        let analysis = check("void f(int*p,int*volatile*q){__builtin_prefetch(p++);__builtin_prefetch(*q,1);__builtin_prefetch(p,1LL,3LL);}", profile).unwrap();
        assert_eq!(analysis.unit().declarations.len(), 1);
        let code = analysis.checked().unwrap();
        let calls = code
            .expressions()
            .filter_map(|(_, e)| {
                if let ExprKind::BuiltinCall {
                    builtin: Builtin::Prefetch,
                    arguments,
                    ..
                } = e.kind()
                {
                    Some(arguments)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 3);
        for (index, arguments) in calls.iter().enumerate() {
            assert_eq!(arguments.len(), index + 1);
            assert!(arguments.iter().all(|a| a.context() == UseContext::Value));
            let TypeKind::Pointer(pointee) = &code.ty(arguments[0].effective_type()).unwrap().kind
            else {
                panic!()
            };
            assert!(pointee.qualifiers.is_const);
            assert!(matches!(pointee.kind, TypeKind::Void));
        }
        for argument in &calls[2][1..] {
            assert_eq!(
                code.ty(argument.effective_type()).unwrap().kind,
                TypeKind::Integer(IntegerKind::LongLong)
            );
        }
        assert!(code.expressions().any(|(_, e)| matches!(
            e.kind(),
            ExprKind::Unary {
                operator: toucan_semantic::checked::Unary::PostIncrement,
                ..
            }
        )));
    }
}

#[test]
fn optional_hints_keep_compiler_checking_stage() {
    for profile in profiles() {
        for source in [
            "void f(void*p){__builtin_prefetch(p,(int)1.0,3);}",
            "void f(void*p){__builtin_prefetch(p,((unsigned __int128)1<<64)|1);}",
            "void f(_Atomic(int)*p){__builtin_prefetch(p,0,0);}",
            "void f(void(*p)(void)){__builtin_prefetch(p);}",
            "void f(int n,int(*p)[n]){__builtin_prefetch(p++);}",
        ] {
            if (profile.target() == Target::I686UnknownLinuxGnu || profile.target().is_armv7())
                && source.contains("__int128")
            {
                let error = check(source, profile).unwrap_err();
                assert!(
                    error.message.contains("__int128 is unavailable"),
                    "{profile:?}: {error}"
                );
                continue;
            }
            check(source, profile).unwrap();
        }
        for source in [
            "void f(void){__builtin_prefetch();}",
            "void f(void){__builtin_prefetch(1.0);}",
            "void f(void*p){__builtin_prefetch(p,(void)0);}",
        ] {
            assert!(check(source, profile).is_err(), "{profile:?}: {source}");
        }
        for source in [
            "void f(void*p,int n){__builtin_prefetch(p,n);}",
            "void f(void*p,int n){__builtin_prefetch(p,(n++,1));}",
            "void f(void*p,int n){if(0)__builtin_prefetch(p,n);}",
            "void f(void*p){__builtin_prefetch(p,2,4);}",
            "void f(void*p){__builtin_prefetch(p,1.0,2.0);}",
            "void f(void*p){__builtin_prefetch(p,1.0<2.0);}",
            "void f(void*p){__builtin_prefetch(p,0x100000001ULL);}",
            "void f(void*p){__builtin_prefetch(p,(void*)0);}",
            "void f(void*p){__builtin_prefetch(p,0,3,7);}",
        ] {
            assert_eq!(
                check(source, profile).is_ok(),
                profile.compiler() == Compiler::Gnu,
                "{profile:?}: {source}"
            );
        }
        for hint in PrefetchHint::ALL {
            assert_eq!(
                hint.stage(profile.compiler()),
                if profile.compiler() == Compiler::Gnu {
                    ImmediateStage::AfterInlining
                } else {
                    ImmediateStage::Frontend
                }
            );
            assert_eq!(
                hint.out_of_range_value(profile.compiler()),
                (profile.compiler() == Compiler::Gnu).then_some(0)
            );
        }
    }
    assert_eq!(PrefetchHint::ReadWrite.default_value(), 0);
    assert_eq!(PrefetchHint::Locality.default_value(), 3);
    assert_eq!(
        PrefetchHint::ReadWrite.normalized_value((1u128 << 64) | 1, Compiler::Clang),
        Some(1)
    );
    assert_eq!(
        PrefetchHint::ReadWrite.normalized_value(1u128 << 32, Compiler::Clang),
        None
    );
}

#[test]
fn gnu_extra_arguments_remain_evaluated_and_promoted() {
    for profile in profiles().filter(|p| p.compiler() == Compiler::Gnu) {
        let analysis=check("extern int side(void);void f(void*p,volatile short*n,float x){__builtin_prefetch(p,0,3,(*n)++,side(),x);}",profile).unwrap();
        let code = analysis.checked().unwrap();
        let arguments = code
            .expressions()
            .find_map(|(_, e)| {
                if let ExprKind::BuiltinCall {
                    builtin: Builtin::Prefetch,
                    arguments,
                    ..
                } = e.kind()
                {
                    Some(arguments)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(arguments.len(), 6);
        assert!(arguments.iter().all(|a| a.context() == UseContext::Value));
        assert_eq!(
            code.ty(arguments[3].effective_type()).unwrap().kind,
            TypeKind::Integer(IntegerKind::Int)
        );
        assert_eq!(
            code.ty(arguments[5].effective_type()).unwrap().kind,
            TypeKind::Float(toucan_semantic::FloatKind::Double)
        );
        assert!(
            arguments[5]
                .conversions()
                .iter()
                .any(|c| c.kind() == Conversion::DefaultArgument)
        );
    }
}

#[test]
fn builtin_declarations_shadowing_symbols_and_addresses_are_distinct() {
    for profile in profiles() {
        for source in [
            "void __builtin_prefetch(const void*,...);void f(void*p){__builtin_prefetch(p);}",
            "int f(int(*__builtin_prefetch)(int)){return __builtin_prefetch(3);}",
            "static int __builtin_prefetch(int x){return x;}int f(void){return __builtin_prefetch(2);}",
            "void __builtin_prefetch(const void*,...)__asm__(\"custom\");void f(void*p){__builtin_prefetch(p);}",
        ] {
            check(source, profile).unwrap();
        }
        for source in [
            "int __builtin_prefetch;",
            "void __builtin_prefetch();",
            "int __builtin_prefetch(int);int f(void){return __builtin_prefetch(1);}",
            "void __builtin_prefetch(const void*p,...){(void)p;}",
            "void f(void*p){__builtin_prefetch(p);}void __builtin_prefetch(const void*,...)__asm__(\"late\");",
        ] {
            assert_eq!(
                check(source, profile).is_ok(),
                profile.compiler() == Compiler::Gnu,
                "{profile:?}: {source}"
            );
        }
        let address = "void(*p)(const void*,...)=__builtin_prefetch;";
        let result = check(address, profile);
        if profile.compiler() == Compiler::Gnu {
            let analysis = result.unwrap();
            assert!(
                analysis
                    .checked()
                    .unwrap()
                    .expressions()
                    .any(|(_, e)| matches!(
                        e.kind(),
                        ExprKind::BuiltinFunction(BuiltinFunction::Prefetch)
                    ))
            );
        } else {
            assert!(result.is_err());
        }
    }
}

#[test]
fn explicit_address_and_cast_call_forms_match_gnu_frontend_restrictions() {
    for profile in profiles() {
        for callee in [
            "(&__builtin_prefetch)",
            "(*&__builtin_prefetch)",
            "(&*__builtin_prefetch)",
            "((void(*)(const void*,...))__builtin_prefetch)",
        ] {
            assert!(check(&format!("void f(void*p){{{callee}(p,1,3);}}"), profile).is_err());
        }
    }
}

#[test]
fn queries_keep_type_constraints_and_discard_runtime_evaluation() {
    for profile in profiles() {
        for source in [
            "_Static_assert(!__builtin_constant_p((__builtin_prefetch(0),1)),\"q\");",
            "_Static_assert(__builtin_constant_p(0&&(__builtin_prefetch(0),1)),\"q\");",
            "int f(int*p,int n){return __builtin_constant_p((__builtin_prefetch(p+n++),1));}",
            "int f(int n,int(*p)[n]){return __builtin_constant_p((__builtin_prefetch(p++),1));}",
        ] {
            check(source, profile).unwrap();
        }
        assert_eq!(
            check(
                "int x=__builtin_constant_p(__builtin_prefetch(0));",
                profile
            )
            .is_ok(),
            profile.compiler() == Compiler::Clang
        );
        assert_eq!(
            check(
                "int f(void*p,int n){return __builtin_constant_p((__builtin_prefetch(p,n),0));}",
                profile
            )
            .is_ok(),
            profile.compiler() == Compiler::Gnu
        );
    }
}

#[test]
fn exact_indirection_designators_remain_discoverable() {
    let source = "void f(void*p,int n){(*__builtin_prefetch)(p,n);(**__builtin_prefetch)(p,n);(0,__builtin_prefetch)(p,n);({__builtin_prefetch;})(p,n);_Generic(0,int:__builtin_prefetch)(p,n);__builtin_choose_expr(1,__builtin_prefetch,__builtin_prefetch)(p,n);void(*q)(const void*,...)=__builtin_prefetch;q(p,n);}";
    for profile in profiles().filter(|p| p.compiler() == Compiler::Gnu) {
        let analysis = check(source, profile).unwrap();
        let code = analysis.checked().unwrap();
        let calls = code
            .expressions()
            .filter(|(_, e)| matches!(e.kind(), ExprKind::Call { .. }))
            .map(|(id, _)| code.prefetch_arguments(id).is_some())
            .collect::<Vec<_>>();
        assert_eq!(calls, vec![true, true, true, true, true, true, false]);
    }
}

#[test]
#[ignore = "requires native GCC on Linux and Clang cross-target syntax support"]
fn prefetch_source_constraints_match_compilers() {
    use std::process::Command;
    use toucan_target::Target;
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("prefetch.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let native = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Some(Target::X86_64UnknownLinuxGnu),
        ("aarch64", "linux") => Some(Target::Aarch64UnknownLinuxGnu),
        _ => None,
    };
    let mut oracles = Target::ALL
        .into_iter()
        .map(|t| {
            (
                "clang",
                CompilerProfile::new(t, Compiler::Clang).unwrap(),
                true,
            )
        })
        .collect::<Vec<_>>();
    if let Some(target) = native {
        let version = Command::new(&gcc).arg("--version").output().unwrap();
        assert!(version.status.success());
        assert!(
            !String::from_utf8_lossy(&version.stdout)
                .to_ascii_lowercase()
                .contains("clang"),
            "TOUCAN_GCC must select GNU GCC"
        );
        oracles.push((
            &gcc,
            CompilerProfile::new(target, Compiler::Gnu).unwrap(),
            false,
        ));
    }
    let oracles = oracles
        .into_iter()
        .flat_map(|(cc, profile, cross)| {
            LanguageMode::ALL.map(|mode| (cc, profile.with_language_mode(mode), cross))
        })
        .collect::<Vec<_>>();
    for (source, gnu, clang) in [
        ("void f(void*p){__builtin_prefetch(p,1LL,3LL);}", true, true),
        (
            "void f(void*p,int n){__builtin_prefetch(p,n);}",
            true,
            false,
        ),
        (
            "void f(void*p,int n){__builtin_prefetch(p,(n++,1),3,n++);}",
            true,
            false,
        ),
        ("void f(void*p){__builtin_prefetch(p,2,4);}", true, false),
        (
            "void f(void*p){__builtin_prefetch(p,0x100000001ULL);}",
            true,
            false,
        ),
        (
            "void f(void*p){__builtin_prefetch(p,(int)1.0);}",
            true,
            true,
        ),
        (
            "void f(void*p){__builtin_prefetch(p,1.0<2.0);}",
            true,
            false,
        ),
        ("void f(void){__builtin_prefetch();}", false, false),
        ("void f(void){__builtin_prefetch(1.0);}", false, false),
        (
            "void f(void*p){__builtin_prefetch(p,(void)0);}",
            false,
            false,
        ),
        ("void(*p)(const void*,...)=__builtin_prefetch;", true, false),
        (
            "int f(int(*__builtin_prefetch)(int)){return __builtin_prefetch(3);}",
            true,
            true,
        ),
    ] {
        std::fs::write(&file, source).unwrap();
        for (cc, profile, cross) in &oracles {
            let expected = if profile.compiler() == Compiler::Gnu {
                gnu
            } else {
                clang
            };
            let mut cmd = Command::new(cc);
            if *cross {
                cmd.arg(format!("--target={}", profile.target()));
            }
            let output = cmd
                .arg(format!("-std={}", profile.language_mode()))
                .arg("-fsyntax-only")
                .arg(&file)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output).unwrap(),
                expected,
                "{cmd:?}\n{source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                check(source, *profile).is_ok(),
                expected,
                "{profile:?}: {source}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
#[ignore = "requires native C compilers and executable output"]
fn native_prefetch_preserves_argument_effects_without_object_access() {
    use std::process::Command;
    use toucan_target::Target;
    let native = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        ("x86_64", "windows") => Target::X86_64PcWindowsMsvc,
        _ => return,
    };
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let mut compilers = vec![("clang", Compiler::Clang)];
    if std::env::consts::OS == "linux" {
        compilers.push((&gcc, Compiler::Gnu));
    }
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("prefetch.c");
    let exe = directory.path().join("prefetch.exe");
    for (cc, compiler, mode) in compilers
        .into_iter()
        .flat_map(|(cc, compiler)| LanguageMode::ALL.map(|mode| (cc, compiler, mode)))
    {
        let extra = if compiler == Compiler::Gnu {
            "__builtin_prefetch(address(),(read_hints++,1),(locality_hints++,2),extra());if(addresses!=3||read_hints!=1||locality_hints!=1||extras!=1)return 4;"
        } else {
            ""
        };
        let source = format!(
            "static int value=17,addresses,read_hints,locality_hints,extras;static void*address(void){{addresses++;return &value;}}static int extra(void){{extras++;return 0;}}int main(void){{__builtin_prefetch(address());__builtin_prefetch(address(),1,2);__builtin_prefetch((void*)1,0,0);if(value!=17||addresses!=2)return 1;if(__builtin_constant_p((__builtin_prefetch(address()),1)))return 2;if(addresses!=2)return 3;{extra}return 0;}}"
        );
        check(
            &source,
            CompilerProfile::new(native, compiler)
                .unwrap()
                .with_language_mode(mode),
        )
        .unwrap();
        std::fs::write(&file, &source).unwrap();
        for optimization in ["-O0", "-O2"] {
            let mut cmd = Command::new(cc);
            cmd.arg(format!("-std={mode}"))
                .arg(optimization)
                .arg(&file)
                .arg("-o")
                .arg(&exe);
            let output = cmd.output().unwrap();
            assert!(
                output.status.success(),
                "{cmd:?}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let status = Command::new(&exe).status().unwrap();
            assert!(status.success(), "{cc} {optimization}: {status}");
        }
    }
}

#[test]
fn direct_designator_restrictions_stop_at_pointer_value_escapes() {
    for profile in profiles() {
        for source in [
            "void f(void){if(__builtin_prefetch)return;}",
            "void f(void){_Bool x=__builtin_prefetch;}",
            "void f(void){if(_Generic(0,int:__builtin_prefetch))return;}",
            "void f(void){if(__builtin_choose_expr(1,__builtin_prefetch,__builtin_prefetch))return;}",
            "void f(void){void(*p)(const void*,...)=1?__builtin_prefetch:__builtin_prefetch;}",
        ] {
            assert!(check(source, profile).is_err(), "{profile:?}: {source}");
        }
        for source in [
            "void f(void){if((0,__builtin_prefetch))return;}",
            "void f(void){if(({__builtin_prefetch;}))return;}",
            "void f(void){void(*p)(const void*,...)=&*({__builtin_prefetch;});}",
            "void f(void){_Bool b=({struct S{int x;};void(*p)(const void*,...)=__builtin_prefetch;p;});}",
        ] {
            assert_eq!(
                check(source, profile).is_ok(),
                profile.compiler() == Compiler::Gnu,
                "{profile:?}: {source}"
            );
        }
    }
}

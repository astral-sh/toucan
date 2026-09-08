use toucan_semantic::{
    AllocationOperation, Analysis, AnalysisOptions, TypeKind, analyze_with_profile,
    checked::{Builtin, Conversion, ExprKind},
};
use toucan_target::{Compiler, CompilerProfile, LanguageMode};

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
fn target_signatures_and_argument_evaluation_are_retained() {
    let source = "void*f(void*p,volatile short*n){p=__builtin_malloc((*n)++);p=__builtin_calloc(2.5,(*n)++);p=__builtin_realloc(p,(*n)++);__builtin_free(p);return p;}";
    for profile in CompilerProfile::ALL
        .into_iter()
        .flat_map(|profile| LanguageMode::ALL.map(|mode| profile.with_language_mode(mode)))
    {
        let analysis = check(source, profile).unwrap();
        assert_eq!(
            analysis.unit().declarations.len(),
            1,
            "no invented exported declarations"
        );
        let code = analysis.checked().unwrap();
        let calls = code
            .expressions()
            .filter_map(|(_, e)| match e.kind() {
                ExprKind::BuiltinCall {
                    builtin: Builtin::Allocation(operation),
                    arguments,
                    declaration,
                    link_name,
                    ..
                } => {
                    assert!(declaration.is_none());
                    assert!(link_name.is_none());
                    Some((*operation, arguments))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 4);
        for (operation, arguments) in calls {
            assert_eq!(
                arguments.len(),
                if matches!(
                    operation,
                    AllocationOperation::Malloc | AllocationOperation::Free
                ) {
                    1
                } else {
                    2
                }
            );
            for (index, argument) in arguments.iter().enumerate() {
                let ty = code.ty(argument.effective_type()).unwrap();
                let pointer = operation == AllocationOperation::Free
                    || operation == AllocationOperation::Realloc && index == 0;
                if pointer {
                    assert!(matches!(ty.kind, TypeKind::Pointer(_)));
                } else {
                    assert!(
                        argument
                            .conversions()
                            .iter()
                            .any(|step| step.kind() == Conversion::Assignment)
                    );
                    assert!(matches!(
                        ty.kind,
                        TypeKind::Integer(
                            toucan_semantic::IntegerKind::UnsignedLong
                                | toucan_semantic::IntegerKind::UnsignedLongLong
                        )
                    ));
                }
            }
        }
    }
}

#[test]
fn source_constraints_and_shadowing_match_profiles() {
    for profile in CompilerProfile::ALL {
        for source in [
            "void*f(void){return __builtin_malloc();}",
            "void*f(void){return __builtin_malloc(1,2);}",
            "void*f(void*p){return __builtin_realloc(p);}",
            "void f(void*p){__builtin_free(p,p);}",
            "void*f(void*p){return __builtin_malloc(p);}",
            "void f(const void*p){__builtin_free(p);}",
            "void f(void){__builtin_free(1);}",
            "void*__builtin_malloc();void*f(void){return __builtin_malloc(1,2);}",
        ] {
            assert!(check(source, profile).is_err(), "{profile:?}: {source}");
        }
        for source in [
            "void*f(void){return (__builtin_malloc)(2.5);}",
            "void f(void){__builtin_free(0);}",
            "void*f(void){return __builtin_malloc(-1);}",
            "int f(int(*__builtin_malloc)(int)){return __builtin_malloc(3);}",
            "int f(void){int __builtin_malloc=3;return __builtin_malloc;}",
            "static int __builtin_malloc(int n){return n;}int f(void){return __builtin_malloc(3);}",
        ] {
            check(source, profile).unwrap_or_else(|e| panic!("{profile:?}: {source}: {e}"));
        }
        for source in [
            "int __builtin_malloc(int);int f(void){return __builtin_malloc(1);}",
            "int __builtin_malloc;",
            "void*__builtin_malloc(unsigned long n){return 0;}",
        ] {
            assert_eq!(
                check(source, profile).is_ok(),
                profile.compiler() == Compiler::Gnu,
                "{profile:?}: {source}"
            );
        }
    }
}

#[test]
fn gnu_builtin_addresses_retain_library_identity() {
    let source = "void*(*allocate)(unsigned long)=__builtin_malloc;void(*release)(void*)=&__builtin_free;void*f(void){return (*__builtin_malloc)(4);}";
    for profile in CompilerProfile::ALL {
        let result = check(source, profile);
        if profile.compiler() == Compiler::Clang {
            assert!(result.is_err());
            continue;
        }
        let analysis = result.unwrap();
        let references = analysis
            .checked()
            .unwrap()
            .expressions()
            .filter_map(|(_, expression)| match expression.kind() {
                ExprKind::BuiltinFunction(operation) => Some(operation.symbol()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(references, ["malloc", "free", "malloc"]);
    }
}

#[test]
fn explicit_declarations_keep_prototypes_symbols_and_call_edges() {
    for profile in CompilerProfile::ALL {
        let size = if profile.target().long_width() == profile.target().pointer_width() {
            "unsigned long"
        } else {
            "unsigned long long"
        };
        let source = format!(
            "void*__builtin_malloc();void*__builtin_realloc(void*,{size});void*f(void*p){{p=__builtin_malloc(1);return __builtin_realloc(p,2);}}"
        );
        let analysis = check(&source, profile).unwrap();
        for declaration in analysis.unit().declarations.iter().take(2) {
            assert!(
                declaration
                    .link_name
                    .as_deref()
                    .is_some_and(|s| s == "malloc" || s == "realloc")
            );
            let TypeKind::Function(function) = &declaration.ty.kind else {
                panic!()
            };
            assert!(function.prototype);
        }
        let code = analysis.checked().unwrap();
        for (_, expression) in code.expressions() {
            if let ExprKind::BuiltinCall {
                builtin: Builtin::Allocation(_),
                declaration,
                ..
            } = expression.kind()
            {
                assert!(declaration.is_some());
            }
        }
    }
}

#[test]
fn constant_queries_typecheck_but_do_not_allocate() {
    let source = "_Static_assert(!__builtin_constant_p(__builtin_malloc(1)),\"malloc\");_Static_assert(!__builtin_constant_p(__builtin_calloc(1,1)),\"calloc\");_Static_assert(!__builtin_constant_p(__builtin_realloc(0,1)),\"realloc\");_Static_assert(!__builtin_constant_p((__builtin_free(0),1)),\"free\");_Static_assert(__builtin_constant_p(0?__builtin_malloc(1):(void*)0),\"dead\");";
    for profile in CompilerProfile::ALL {
        check(source, profile).unwrap();
        assert_eq!(
            check(
                "_Static_assert(!__builtin_constant_p(__builtin_free(0)),\"void query\");",
                profile
            )
            .is_ok(),
            profile.compiler() == Compiler::Clang
        );
    }
}

#[test]
fn clang_symbol_aliases_are_lexical_and_respect_first_use() {
    for profile in CompilerProfile::ALL {
        let size = if profile.target().long_width() == profile.target().pointer_width() {
            "unsigned long"
        } else {
            "unsigned long long"
        };
        let source = format!(
            "void*f(void){{extern void*__builtin_malloc({size})__asm__(\"custom\");return __builtin_malloc(1);}}void*g(void){{return __builtin_malloc(1);}}"
        );
        let analysis = check(&source, profile).unwrap();
        let links = analysis
            .checked()
            .unwrap()
            .expressions()
            .filter_map(|(_, e)| match e.kind() {
                ExprKind::BuiltinCall {
                    builtin: Builtin::Allocation(_),
                    link_name,
                    ..
                } => Some(link_name.as_deref()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            links,
            if profile.compiler() == Compiler::Clang {
                vec![Some("custom"), None]
            } else {
                vec![None, None]
            }
        );
        for prefix in [
            "typedef __typeof__(__builtin_malloc(1)) P;",
            "int n=sizeof(__builtin_malloc(1));",
            "int n=__alignof__(__builtin_malloc(1));",
            "int f(void){return _Generic(__builtin_malloc(1),void*:1);}",
        ] {
            check(
                &format!("{prefix}void*__builtin_malloc({size})__asm__(\"custom\");"),
                profile,
            )
            .unwrap();
        }
        for prefix in [
            "void*f(void){return __builtin_malloc(1);}",
            "enum{n=__builtin_constant_p(__builtin_malloc(1))};",
            "void*f(void){return __builtin_choose_expr(1,(void*)0,__builtin_malloc(1));}",
            "void*f(void){return _Generic(0,int:(void*)0,default:__builtin_malloc(1));}",
        ] {
            let result = check(
                &format!("{prefix}void*__builtin_malloc({size})__asm__(\"custom\");"),
                profile,
            );
            assert_eq!(
                result.is_ok(),
                profile.compiler() == Compiler::Gnu,
                "{profile:?}: {prefix}: {result:?}"
            );
        }
        for middle in ["", "void*__builtin_malloc(SIZE);"] {
            let source = format!("void*__builtin_malloc({size})__asm__(\"custom\");{middle}void*f(void){{return __builtin_malloc(1);}}").replace("SIZE",size);
            let analysis = check(&source, profile).unwrap();
            assert_eq!(
                analysis.unit().declarations[0].link_name.as_deref(),
                Some(if profile.compiler() == Compiler::Clang {
                    "custom"
                } else {
                    "malloc"
                })
            );
        }
    }
}

#[test]
fn nonreturn_promises_follow_builtin_attribute_rules() {
    for profile in CompilerProfile::ALL {
        let size = if profile.target().long_width() == profile.target().pointer_width() {
            "unsigned long"
        } else {
            "unsigned long long"
        };
        for (spelling, expected) in [
            (
                "__attribute__((noreturn))",
                profile.compiler() == Compiler::Clang,
            ),
            ("_Noreturn", true),
        ] {
            let source = format!(
                "{spelling} void*__builtin_malloc({size});void f(void){{__builtin_malloc(1);}}"
            );
            let analysis = check(&source, profile).unwrap();
            assert_eq!(analysis.unit().declarations[0].noreturn, expected);
            let call = analysis
                .checked()
                .unwrap()
                .expressions()
                .find_map(|(_, e)| match e.kind() {
                    ExprKind::BuiltinCall {
                        builtin: Builtin::Allocation(_),
                        noreturn,
                        ..
                    } => Some(*noreturn),
                    _ => None,
                })
                .unwrap();
            assert_eq!(call, expected);
        }
    }
}

#[test]
#[ignore = "requires native GCC on Linux and Clang cross-target syntax support"]
fn allocation_constraints_match_native_compilers() {
    use std::process::Command;
    use toucan_target::Target;
    let directory = tempfile::tempdir().unwrap();
    let source_file = directory.path().join("allocation.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let native = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Some(Target::X86_64UnknownLinuxGnu),
        ("aarch64", "linux") => Some(Target::Aarch64UnknownLinuxGnu),
        _ => None,
    };
    let mut oracles = Target::ALL
        .into_iter()
        .map(|target| {
            (
                "clang",
                CompilerProfile::new(target, Compiler::Clang).unwrap(),
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
            "set TOUCAN_GCC to genuine GCC"
        );
        oracles.push((
            gcc.as_str(),
            CompilerProfile::new(target, Compiler::Gnu).unwrap(),
            false,
        ));
    }
    for (compiler, profile, cross) in oracles.into_iter().flat_map(|(compiler, profile, cross)| {
        LanguageMode::ALL.map(|mode| (compiler, profile.with_language_mode(mode), cross))
    }) {
        for (source, accepted) in [
            ("void*f(void){return __builtin_malloc(2.5);}", true),
            ("void*f(void){return __builtin_calloc(2,4);}", true),
            ("void*f(void*p){return __builtin_realloc(p,4);}", true),
            ("void f(void){__builtin_free(0);}", true),
            ("void*f(void){return (__builtin_malloc)(4);}", true),
            (
                "int f(int(*__builtin_malloc)(int)){return __builtin_malloc(3);}",
                true,
            ),
            ("static int __builtin_malloc(int n){return n;}", true),
            (
                "void*__builtin_malloc();void*f(void){return __builtin_malloc(1);}",
                true,
            ),
            ("void*f(void){return __builtin_malloc();}", false),
            ("void*f(void){return __builtin_malloc(1,2);}", false),
            ("void*f(void*p){return __builtin_calloc(p,1);}", false),
            ("void*f(void*p){return __builtin_realloc(p);}", false),
            ("void f(const void*p){__builtin_free(p);}", false),
            ("void f(void){__builtin_free(1);}", false),
            (
                "void __builtin_free();void f(void){__builtin_free(0,1);}",
                false,
            ),
        ] {
            std::fs::write(&source_file, format!("{source}\n")).unwrap();
            let mut command = Command::new(compiler);
            command.arg(format!("-std={}", profile.language_mode()));
            command.args([
                "-fsyntax-only",
                "-Werror=int-conversion",
                "-Werror=incompatible-pointer-types",
            ]);
            // These tests promote C argument-qualification constraints without
            // treating Clang's valid-C non-prototype deprecation as an error.
            command.arg(if profile.compiler() == Compiler::Clang {
                "-Werror=incompatible-pointer-types-discards-qualifiers"
            } else {
                "-Werror=discarded-qualifiers"
            });
            if cross {
                command.arg(format!("--target={}", profile.target()));
            }
            let result = command.arg(&source_file).output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&result),
                Ok(accepted),
                "{profile:?}: {source}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                check(source, profile).is_ok(),
                accepted,
                "{profile:?}: {source}"
            );
        }
    }
}

#[test]
fn array_bound_uses_survive_unevaluated_outer_contexts() {
    for profile in CompilerProfile::ALL {
        for prefix in [
            "void f(int n){sizeof(int(*)[(__builtin_free(0),n)]);}",
            "void f(int n){__alignof__(int[(__builtin_free(0),n)]);}",
            "void f(int n){_Generic((int(*)[(__builtin_free(0),n)])0,default:0);}",
            "void f(int n,int a[(__builtin_free(0),n)]);",
            "void f(void){sizeof(int[1+__builtin_constant_p((__builtin_free(0),0))]);}",
        ] {
            let result = check(
                &format!("{prefix}void __builtin_free(void*)__asm__(\"custom\");"),
                profile,
            );
            assert_eq!(
                result.is_ok(),
                profile.compiler() == Compiler::Gnu,
                "{profile:?}: {prefix}: {result:?}"
            );
        }
        check("void f(void){sizeof(int[sizeof(__builtin_malloc(1))]);}void*__builtin_malloc()__asm__(\"custom\");",profile).unwrap();
    }
}

#[test]
fn nested_type_contexts_keep_independent_first_uses() {
    for profile in CompilerProfile::ALL {
        let size = if profile.target().long_width() == profile.target().pointer_width() {
            "unsigned long"
        } else {
            "unsigned long long"
        };
        for prefix in [
            "void f(int n){sizeof((__builtin_malloc(1),(int(*)[(__builtin_malloc(1),n)])0));}",
            "void f(int n){__typeof__((__builtin_malloc(1),(int(*)[n])0)) p;}",
        ] {
            let source = format!("{prefix}void*__builtin_malloc({size})__asm__(\"custom\");");
            assert_eq!(
                check(&source, profile).is_ok(),
                profile.compiler() == Compiler::Gnu,
                "{profile:?}: {source}"
            );
        }
        let source = "__attribute__((aligned(16+__builtin_constant_p((__builtin_free(0),0))))) int x;void __builtin_free(void*)__asm__(\"custom\");";
        assert_eq!(
            check(source, profile).is_ok(),
            profile.compiler() == Compiler::Gnu
        );
        let source = format!(
            "void f(void){{sizeof(({{extern void*__builtin_malloc({size})__asm__(\"custom\");0;}}));}}"
        );
        let result = check(&source, profile);
        if profile.compiler() == Compiler::Clang {
            assert!(
                result
                    .unwrap_err()
                    .message
                    .contains("inside unevaluated operands are unsupported")
            );
        } else {
            result.unwrap();
        }
    }
}

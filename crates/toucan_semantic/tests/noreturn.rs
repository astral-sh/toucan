use toucan_semantic::{Analysis, AnalysisOptions, analyze_with_profile, checked::ExprKind};
use toucan_target::{Compiler, CompilerProfile};

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

fn calls(analysis: &Analysis) -> Vec<bool> {
    let code = analysis.checked().unwrap();
    let mut calls = code
        .expressions()
        .filter_map(|(_, expression)| match expression.kind() {
            ExprKind::Call { noreturn, .. } => Some((
                code.occurrence(expression.occurrence())
                    .unwrap()
                    .source()
                    .range()
                    .start,
                *noreturn,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    calls.sort_by_key(|(start, _)| *start);
    calls.into_iter().map(|(_, value)| value).collect()
}

#[test]
fn declarations_calls_and_source_spans_keep_the_written_promise() {
    let source =
        "void f(void);void before(void){f();}_Noreturn void f(void);void after(void){f();}";
    for profile in CompilerProfile::ALL {
        let analysis = check(source, profile).unwrap();
        let declaration = analysis
            .unit()
            .declarations
            .iter()
            .find(|d| d.name == "f")
            .unwrap();
        assert!(declaration.noreturn);
        let toucan_semantic::TypeKind::Function(function) = &declaration.ty.kind else {
            panic!()
        };
        assert!(
            !function.noreturn,
            "C11 spelling is not a function type contract"
        );
        assert_eq!(calls(&analysis), [false, true]);
        let code = analysis.checked().unwrap();
        let (entity, function) = code
            .entities()
            .find(|(_, e)| e.name() == Some("f"))
            .unwrap();
        assert!(function.noreturn());
        let sites = code
            .declarations()
            .filter(|(_, site)| site.entity() == entity)
            .map(|(_, site)| site)
            .collect::<Vec<_>>();
        assert_eq!(
            sites.iter().map(|s| s.noreturn()).collect::<Vec<_>>(),
            [false, true]
        );
        assert!(sites[0].noreturn_source().is_none());
        let range = sites[1].noreturn_source().unwrap().range();
        assert_eq!(&source[range.clone()], "_Noreturn");
    }
}

#[test]
fn linked_block_promises_follow_compiler_visibility() {
    for profile in CompilerProfile::ALL {
        let gnu = profile.compiler() == Compiler::Gnu;
        for (source, expected) in [
            (
                "void f(void);void one(void){_Noreturn void f(void);f();}void after(void){f();}",
                vec![true, gnu],
            ),
            (
                "void one(void){_Noreturn void f(void);f();}void f(void);void after(void){f();}",
                vec![true, true],
            ),
            (
                "void f(void);void one(void){{_Noreturn void f(void);f();}{void f(void);f();}}",
                vec![true, gnu],
            ),
            (
                "void f(void);void one(void){_Noreturn void f(void);{void f(void);f();}}",
                vec![true],
            ),
        ] {
            let analysis = check(source, profile).unwrap();
            assert_eq!(calls(&analysis), expected, "{profile:?}: {source}");
        }
    }
}

#[test]
fn later_argument_declarations_do_not_rewrite_the_callee_snapshot() {
    let source = "void f(int);void g(void){f(({_Noreturn void f(int);0;}));f(1);}";
    for profile in CompilerProfile::ALL {
        let analysis = check(source, profile).unwrap();
        assert_eq!(
            calls(&analysis),
            [false, profile.compiler() == Compiler::Gnu]
        );
    }
}

#[test]
fn function_types_and_erased_pointer_values_remain_distinct() {
    let source = "_Noreturn void f(void);void g(void){(&f)();void(*p)(void)=f;p();}";
    for profile in CompilerProfile::ALL {
        assert_eq!(calls(&check(source, profile).unwrap()), [true, false]);
        check(
            "typedef void F(void);_Noreturn F f;void(*p)(void)=f;",
            profile,
        )
        .unwrap();
        let source = "typedef void F(void)__attribute__((noreturn));void g(F*p){p();}";
        let a = check(source, profile).unwrap();
        assert_eq!(calls(&a), [profile.compiler() == Compiler::Clang]);
    }
}

#[test]
fn c11_specifier_requires_a_function_declaration() {
    for profile in CompilerProfile::ALL {
        for source in [
            "_Noreturn typedef void F(void);",
            "_Noreturn void(*p)(void);",
            "_Noreturn struct S;",
            "_Noreturn int;",
            "_Noreturn void f(void),(*p)(void);",
            "void f(_Noreturn int x);",
            "void f(_Noreturn void callback(void));",
            "void f(void){_Noreturn typedef void F(void);}",
        ] {
            let error = check(source, profile).unwrap_err();
            assert!(
                error
                    .message
                    .contains("_Noreturn requires a function declaration"),
                "{profile:?}: {source}: {error}"
            );
        }
        for source in [
            "_Noreturn _Noreturn void f(void);",
            "_Noreturn int*f(void);",
            "_Noreturn void f(void){__builtin_trap();}",
        ] {
            check(source, profile).unwrap();
        }
    }
}

#[test]
fn malformed_public_metadata_is_rejected() {
    let analysis = check("int object;", CompilerProfile::ALL[0]).unwrap();
    let mut unit = analysis.unit().clone();
    unit.declarations[0].noreturn = true;
    let error = toucan_semantic::evaluate_integer(&unit, "1").unwrap_err();
    assert!(
        error
            .message
            .contains("noreturn metadata requires a function declaration")
    );
    unit.declarations[0].kind = toucan_semantic::DeclarationKind::Function;
    assert!(toucan_semantic::evaluate_integer(&unit, "1").is_err());
}

#[test]
#[ignore = "requires native GCC on Linux and Clang cross-target syntax support"]
fn c11_subject_constraints_match_native_compilers() {
    use std::process::Command;
    use toucan_target::Target;
    let directory = tempfile::tempdir().unwrap();
    let source_file = directory.path().join("noreturn.c");
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
    for (compiler, profile, cross) in oracles {
        for (source, accepted) in [
            ("_Noreturn void f(void);void f(void);", true),
            ("void f(void);_Noreturn void f(void);", true),
            ("void f(void){}_Noreturn void f(void);", true),
            ("typedef void F(void);_Noreturn F f;void(*p)(void)=f;", true),
            ("_Noreturn _Noreturn int*f(void);", true),
            (
                "void f(void);void outer(void){_Noreturn void f(void);f();}",
                true,
            ),
            ("_Noreturn void f(void){__builtin_trap();}", true),
            ("_Noreturn typedef void F(void);", false),
            ("_Noreturn void(*p)(void);", false),
            ("_Noreturn struct S;", false),
            ("_Noreturn int;", false),
            ("_Noreturn void f(void),(*p)(void);", false),
            ("void f(_Noreturn int x);", false),
            ("void f(_Noreturn void callback(void));", false),
            ("void f(void){_Noreturn typedef void F(void);}", false),
        ] {
            std::fs::write(&source_file, format!("{source}\n")).unwrap();
            let mut command = Command::new(compiler);
            command.args(["-std=c11", "-pedantic-errors", "-fsyntax-only"]);
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

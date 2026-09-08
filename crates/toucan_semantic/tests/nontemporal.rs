use toucan_semantic::{
    Analysis, AnalysisOptions, TypeKind, analyze_with_profile,
    checked::{
        Builtin, Conversion, ExprKind, NontemporalOperation as Op, QueryEvaluation, TypeStep,
        UseContext,
    },
};
use toucan_target::{Compiler, CompilerProfile, Target};

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
        (Err(a), Err(b)) => assert_eq!((a.offset, &a.message), (b.offset, &b.message)),
        _ => panic!("{source}: {plain:?} {kept:?}"),
    }
    kept
}
fn cases() -> Vec<(String, bool)> {
    let mut cases = Vec::new();
    for (ty, value, valid) in [
        ("int", "1", true),
        ("_Bool", "3", true),
        ("enum E", "1", true),
        ("float", "2.5", true),
        ("double", "2.5f", true),
        ("long double", "2.5", true),
        ("_Float16", "2.5", true),
        ("__bf16", "2.5", true),
        ("int*", "(int*)0", true),
        ("const int*", "(int*)0", true),
        ("void*", "(int*)0", true),
        ("V", "(V){1,2,3,4}", true),
        ("const int", "1", true),
        ("volatile int", "1", true),
        ("const volatile int", "1", true),
        ("_Atomic(int)", "1", false),
        ("struct S", "(struct S){1}", false),
        ("_Complex double", "2.0", true),
        ("int[4]", "1", false),
        ("int(void)", "1", false),
    ] {
        let prefix =
            "typedef int V __attribute__((vector_size(16))); enum E{E0}; struct S{int x;};";
        cases.push((
            format!("{prefix} void f(__typeof__({ty})*p){{(void)__builtin_nontemporal_load(p);}}"),
            valid,
        ));
        cases.push((
            format!(
                "{prefix} void f(__typeof__({ty})*p){{__builtin_nontemporal_store({value},p);}}"
            ),
            valid,
        ));
    }
    for (source, valid) in [
        ("void f(int*p){__builtin_nontemporal_store(p);}", false),
        (
            "void f(int*p){(void)__builtin_nontemporal_load(p,0);}",
            false,
        ),
        ("void f(void){(void)__builtin_nontemporal_load(0);}", false),
        ("void f(int*p){__builtin_nontemporal_load(p)=1;}", false),
        (
            "void f(void){int a[4];__builtin_nontemporal_store(1,a);(void)__builtin_nontemporal_load(a);}",
            true,
        ),
        (
            "typedef int V __attribute__((vector_size(16))); typedef float F __attribute__((vector_size(16))); void f(V*p,F x){__builtin_nontemporal_store(x,p);}",
            true,
        ),
        (
            "typedef int V __attribute__((vector_size(16))); void f(V*p){__builtin_nontemporal_store(1,p);}",
            false,
        ),
        (
            "typedef int V __attribute__((vector_size(16))); typedef short W __attribute__((vector_size(8))); void f(V*p,W x){__builtin_nontemporal_store(x,p);}",
            false,
        ),
        (
            "typedef int(*P)(void);void f(P*p,P v){__builtin_nontemporal_store(v,p);P q=__builtin_nontemporal_load(p);}",
            true,
        ),
        (
            "void f(_Atomic(int)**p,_Atomic(int)*v){__builtin_nontemporal_store(v,p);(void)__builtin_nontemporal_load(p);}",
            true,
        ),
        (
            "void f(int*p,_Atomic(int) value){__builtin_nontemporal_store(value,p);}",
            true,
        ),
        (
            "_Static_assert(!__builtin_constant_p(__builtin_nontemporal_load((int*)0)),\"not constant\");",
            true,
        ),
        ("int x=__builtin_nontemporal_load((int*)0);", false),
        (
            "typedef int A __attribute__((aligned(1))); _Static_assert(__alignof__(__typeof__(__builtin_nontemporal_load((A*)0)))==1,\"alignment\");",
            true,
        ),
    ] {
        cases.push((source.into(), valid));
    }
    cases
}
#[test]
fn memory_types_and_constraints_match_profile_rules() {
    for profile in CompilerProfile::ALL {
        for (source, valid) in cases() {
            let result = check(&source, profile);
            assert_eq!(
                result.is_ok(),
                valid && profile.compiler() == Compiler::Clang,
                "{profile:?} {source}: {result:?}"
            );
        }
    }
}
#[test]
fn retained_access_keeps_qualifiers_and_distinguishes_vector_bits_from_scalar_conversion() {
    let source = "typedef int V __attribute__((vector_size(16)));typedef float F __attribute__((vector_size(16))); void f(volatile int*volatile p,volatile double v,V*bits,F lanes,_Atomic(int) atom){__builtin_nontemporal_store(v,p);__builtin_nontemporal_store(lanes,bits);__builtin_nontemporal_store(atom,p);(void)__builtin_nontemporal_load(p);}";
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let a = check(source, profile).unwrap();
        let code = a.checked().unwrap();
        let calls = code
            .expressions()
            .filter_map(|(_, e)| match e.kind() {
                ExprKind::BuiltinCall {
                    builtin: Builtin::Nontemporal(op),
                    arguments,
                    ..
                } => Some((*op, arguments)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 4);
        for (op, args) in &calls {
            assert_eq!(args.len(), op.address_argument() + 1);
            assert!(args.iter().all(|a| a.context() == UseContext::Value));
        }
        let scalar = &calls[0].1;
        assert!(
            scalar[0]
                .conversions()
                .iter()
                .any(|c| c.kind() == Conversion::Assignment)
        );
        assert!(
            code.ty(code.expression(scalar[0].expression()).unwrap().ty())
                .unwrap()
                .qualifiers
                .is_volatile
        );
        let address = code.ty(scalar[1].effective_type()).unwrap();
        let TypeKind::Pointer(pointee) = &address.kind else {
            panic!()
        };
        assert!(a.unit().qualifiers(pointee).unwrap().is_volatile);
        assert!(
            code.ty(code.expression(scalar[1].expression()).unwrap().ty())
                .unwrap()
                .qualifiers
                .is_volatile
        );
        assert!(
            calls[1].1[0]
                .conversions()
                .iter()
                .any(|c| c.kind() == Conversion::VectorReinterpret)
        );
        assert!(
            calls[2].1[0]
                .conversions()
                .iter()
                .any(|c| c.kind() == Conversion::AtomicLoad)
        );
        let load = code
            .expressions()
            .find(|(_, e)| {
                matches!(
                    e.kind(),
                    ExprKind::BuiltinCall {
                        builtin: Builtin::Nontemporal(Op::Load),
                        ..
                    }
                )
            })
            .unwrap()
            .1;
        assert!(!code.ty(load.ty()).unwrap().qualifiers.is_volatile);
    }
}
#[test]
fn queries_suppress_accesses_and_preserve_pointer_to_vla_origins() {
    let source = "int bump(void); void f(int n,int m,int(*p)[n],int(*q)[m]){ __builtin_nontemporal_store(q,&p); int(*r)[n]=__builtin_nontemporal_load(&p); (void)__builtin_object_size((int(*)[bump()])__builtin_nontemporal_load(&p),0); _Static_assert(!__builtin_constant_p(__builtin_nontemporal_load(&p)),\"query\"); }";
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let a = check(source, profile).unwrap();
        let code = a.checked().unwrap();
        for (_, e) in code.expressions() {
            if let ExprKind::BuiltinCall {
                builtin: Builtin::Nontemporal(Op::Load),
                arguments,
                ..
            } = e.kind()
            {
                let input = code.type_use(arguments[0].type_use()).unwrap().extents();
                let result = code.type_use(e.type_use()).unwrap().extents();
                assert_eq!(input.len(), 1);
                assert_eq!(result.len(), 1);
                assert_eq!(input[0].bound(), result[0].bound());
                assert_eq!(input[0].path(), [TypeStep::Pointer, TypeStep::Pointer]);
                assert_eq!(result[0].path(), [TypeStep::Pointer]);
            }
            if let ExprKind::BuiltinCall {
                builtin: Builtin::ObjectSize,
                query_evaluation,
                ..
            } = e.kind()
            {
                assert!(matches!(
                    query_evaluation,
                    Some(QueryEvaluation::Unevaluated(_))
                ));
            }
        }
    }
}
#[test]
fn repeated_queries_do_not_redeclare_operand_scopes() {
    let source = "int f(void){return __builtin_constant_p(__builtin_nontemporal_load(({struct Once{int x;};int value=1;&value;})));}";
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let a = check(source, profile).unwrap();
        assert_eq!(
            a.unit()
                .records
                .iter()
                .filter(|r| r.name.as_deref() == Some("Once"))
                .count(),
            1
        );
    }
}
#[test]
#[ignore = "requires native GCC and Clang; Clang also checks all five targets"]
fn native_constraint_matrix() {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("hint.c");
    let output = d.path().join("hint.s");
    for p in CompilerProfile::ALL {
        let host = cfg!(target_os = "linux")
            && ((cfg!(target_arch = "x86_64") && p.target() == Target::X86_64UnknownLinuxGnu)
                || (cfg!(target_arch = "aarch64") && p.target() == Target::Aarch64UnknownLinuxGnu));
        let mut cc = if p.compiler() == Compiler::Gnu {
            if !host {
                continue;
            }
            std::process::Command::new(std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()))
        } else {
            let mut c = std::process::Command::new("clang");
            c.args(["-target", p.target().triple()]);
            c
        };
        cc.args(["-std=gnu11", "-Werror=implicit-function-declaration", "-S"])
            .arg(&input)
            .arg("-o")
            .arg(&output);
        for (source, valid) in cases() {
            if source.contains("_Complex") {
                continue;
            } // Classified separately: valid typing, incomplete compiler lowering.
            std::fs::write(&input, &source).unwrap();
            let result = cc.output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&result).unwrap(),
                valid && p.compiler() == Compiler::Clang,
                "{p:?} {source}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

#[test]
#[ignore = "requires native Clang and native GCC on Linux"]
fn memory_accesses_work_through_independent_c_callers() {
    let source = include_str!("fixtures/nontemporal/access.c");
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let source = source.replace("__ATOMIC_SEQ_CST", "5");
        check(&source, profile).unwrap();
    }
    let d = tempfile::tempdir().unwrap();
    let source_path = d.path().join("access.c");
    let caller = d.path().join("caller.c");
    std::fs::write(&source_path, source).unwrap();
    std::fs::write(&caller, include_str!("fixtures/nontemporal/caller.c")).unwrap();
    for optimization in ["-O0", "-O2"] {
        let object = d.path().join("access.o");
        let output = std::process::Command::new("clang")
            .args(["-std=gnu11", optimization, "-c"])
            .arg(&source_path)
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for cc in ["clang", "gcc"] {
            if cc == "gcc" && !cfg!(target_os = "linux") {
                continue;
            }
            let cc = if cc == "gcc" {
                std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into())
            } else {
                cc.into()
            };
            let executable = d.path().join("caller.exe");
            let result = std::process::Command::new(cc)
                .args(["-std=gnu11", optimization])
                .arg(&caller)
                .arg(&object)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let result = std::process::Command::new(&executable).output().unwrap();
            assert!(
                result.status.success(),
                "exit={:?} {}",
                result.status.code(),
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

#[test]
fn microsoft_forward_enum_layout_has_an_explicit_boundary() {
    let profile = CompilerProfile::default_for(Target::X86_64PcWindowsMsvc);
    for expression in [
        "(void)__builtin_nontemporal_load(p)",
        "__builtin_nontemporal_store(1,p)",
    ] {
        let source = format!("enum E; void f(enum E*p){{{expression};}}");
        let error = check(&source, profile).unwrap_err();
        assert!(
            error
                .message
                .contains("unsupported incomplete-enum layouts"),
            "{error}"
        );
    }
}

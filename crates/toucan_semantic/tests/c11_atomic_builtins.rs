use toucan_semantic::checked::{
    Builtin, C11AtomicOperation as A, Conversion, ExprKind, TypeStep, UseContext,
};
use toucan_semantic::{
    Analysis, AnalysisOptions, IntegerKind, TypeKind, analyze, analyze_with_options,
};
use toucan_target::Target;

const CLANG: [Target; 3] = [
    Target::X86_64AppleDarwin,
    Target::Aarch64AppleDarwin,
    Target::X86_64PcWindowsMsvc,
];
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
const CALLS: &[(&str, A)] = &[
    ("__c11_atomic_init(p,value)", A::Init),
    ("__c11_atomic_load(p,order)", A::Load),
    ("__c11_atomic_store(p,value,order)", A::Store),
    ("__c11_atomic_exchange(p,value,order)", A::Exchange),
    (
        "__c11_atomic_compare_exchange_strong(p,q,value,order,order)",
        A::CompareExchangeStrong,
    ),
    (
        "__c11_atomic_compare_exchange_weak(p,q,value,order,order)",
        A::CompareExchangeWeak,
    ),
    ("__c11_atomic_fetch_add(p,value,order)", A::FetchAdd),
    ("__c11_atomic_fetch_sub(p,value,order)", A::FetchSub),
    ("__c11_atomic_fetch_and(p,value,order)", A::FetchAnd),
    ("__c11_atomic_fetch_or(p,value,order)", A::FetchOr),
    ("__c11_atomic_fetch_xor(p,value,order)", A::FetchXor),
    ("__c11_atomic_fetch_nand(p,value,order)", A::FetchNand),
    ("__c11_atomic_fetch_min(p,value,order)", A::FetchMin),
    ("__c11_atomic_fetch_max(p,value,order)", A::FetchMax),
    ("__c11_atomic_thread_fence(order)", A::ThreadFence),
    ("__c11_atomic_signal_fence(order)", A::SignalFence),
    ("__c11_atomic_is_lock_free(value++)", A::IsLockFree),
];
fn source() -> String {
    format!(
        "void f(volatile _Atomic(short)*p,short*q,char value,unsigned char order){{{};}}",
        CALLS.iter().map(|(s, _)| *s).collect::<Vec<_>>().join(";")
    )
}
#[test]
fn signatures_retain_initialization_orders_and_evaluated_arguments() {
    for target in CLANG {
        let analysis = check(&source(), target).unwrap();
        let code = analysis.checked().unwrap();
        let calls = code
            .expressions()
            .filter_map(|(_, e)| match e.kind() {
                ExprKind::BuiltinCall {
                    builtin: Builtin::C11Atomic(op),
                    arguments,
                    ..
                } => Some((*op, arguments, e)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), CALLS.len());
        let queries = format!(
            "void f(volatile _Atomic(short)*p,short*q,char value,unsigned char order){{{};}}",
            CALLS
                .iter()
                .map(|(s, _)| format!("__builtin_constant_p(({s},1))"))
                .collect::<Vec<_>>()
                .join(";")
        );
        let q = check(&queries, target).unwrap();
        let q = q.checked().unwrap();
        let policies = q
            .expressions()
            .filter_map(|(_, e)| match e.kind() {
                ExprKind::BuiltinCall {
                    builtin: Builtin::ConstantQuery,
                    query_evaluation,
                    ..
                } => *query_evaluation,
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            policies,
            vec![
                toucan_semantic::checked::QueryEvaluation::Unevaluated(
                    toucan_semantic::checked::QuerySuppression::OrdinarySideEffects
                );
                CALLS.len()
            ]
        );

        for ((_, expected), (op, args, e)) in CALLS.iter().zip(calls) {
            assert_eq!(*expected, op);
            assert!(e.atomic_access().is_none());
            for arg in args {
                assert_eq!(arg.context(), UseContext::Value);
                assert!(
                    !arg.conversions()
                        .iter()
                        .any(|c| c.kind() == Conversion::DefaultArgument)
                );
            }
            if let Some((order, failure)) = op.memory_order_arguments() {
                for index in [Some(order), failure].into_iter().flatten() {
                    assert_eq!(
                        code.ty(args[index].effective_type()).unwrap().kind,
                        TypeKind::Integer(IntegerKind::Int)
                    );
                }
            }
            assert_eq!(
                matches!(code.ty(e.ty()).unwrap().kind, TypeKind::Bool),
                matches!(
                    op,
                    A::CompareExchangeStrong | A::CompareExchangeWeak | A::IsLockFree
                )
            );
        }
    }
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        assert!(
            check(&source(), target)
                .unwrap_err()
                .message
                .contains("Clang compiler profile")
        );
    }
}

const CASES: &[(&str, bool)] = &[
    ("void f(int*p){__c11_atomic_load(p,5);}", false),
    (
        "void f(const _Atomic(int)*p){__c11_atomic_load(p,5);}",
        true,
    ),
    (
        "void f(const _Atomic(int)*p){__c11_atomic_store(p,1,5);}",
        false,
    ),
    (
        "void f(const _Atomic(int)*p){__c11_atomic_init(p,1);}",
        false,
    ),
    (
        "void f(volatile _Atomic(int)*p){__c11_atomic_store(p,1,5);}",
        true,
    ),
    (
        "void f(_Atomic(int)*p,const int*q){__c11_atomic_compare_exchange_strong(p,q,1,5,2);}",
        false,
    ),
    (
        "void f(_Atomic(int)*p,volatile int*q){__c11_atomic_compare_exchange_strong(p,q,1,5,2);}",
        false,
    ),
    (
        "void f(_Atomic(int)*p,void*q){__c11_atomic_compare_exchange_strong(p,q,1,5,2);}",
        true,
    ),
    (
        "void f(_Atomic(int)*p,unsigned*q){__c11_atomic_compare_exchange_strong(p,q,1,5,2);}",
        false,
    ),
    (
        "void f(_Atomic(int)*p,_Atomic(int)*q){__c11_atomic_compare_exchange_strong(p,q,1,5,2);}",
        false,
    ),
    ("void f(_Atomic(int*)*p){__c11_atomic_store(p,7,5);}", false),
    (
        "void f(_Atomic(int*)*p,const int*q){__c11_atomic_store(p,q,5);}",
        false,
    ),
    (
        "void f(_Atomic(int*)*p,float q){__c11_atomic_fetch_add(p,q,5);}",
        true,
    ),
    (
        "void f(_Atomic(int*)*p,int*q){__c11_atomic_fetch_add(p,q,5);}",
        false,
    ),
    (
        "struct S;void f(_Atomic(struct S*)*p){__c11_atomic_fetch_add(p,1,5);}",
        false,
    ),
    (
        "void f(_Atomic(void*)*p){__c11_atomic_fetch_add(p,1,5);}",
        false,
    ),
    (
        "void f(int n,_Atomic(int(*)[n])*p){__c11_atomic_fetch_add(p,1,5);}",
        true,
    ),
    (
        "void f(_Atomic(float)*p){__c11_atomic_fetch_add(p,1,5);__c11_atomic_fetch_max(p,3,5);}",
        true,
    ),
    (
        "void f(_Atomic(float)*p){__c11_atomic_fetch_and(p,1,5);}",
        false,
    ),
    (
        "void f(_Atomic(_Bool)*p){__c11_atomic_fetch_xor(p,1,5);}",
        true,
    ),
    (
        "enum E{A};enum E f(_Atomic(enum E)*p){return __c11_atomic_fetch_add(p,1,5);}",
        true,
    ),
    (
        "struct S{char x[3];};void f(_Atomic(struct S)*p,struct S s,struct S*q){__c11_atomic_init(p,s);s=__c11_atomic_load(p,0);__c11_atomic_store(p,s,0);s=__c11_atomic_exchange(p,s,5);__c11_atomic_compare_exchange_weak(p,q,s,5,2);}",
        true,
    ),
    (
        "struct S{const int x;};void f(_Atomic(struct S)*p,struct S s){__c11_atomic_store(p,s,0);}",
        true,
    ),
    ("void f(_Atomic(int)*p){__c11_atomic_store(p,1);}", false),
    ("void f(_Atomic(int)*p){__c11_atomic_load(p,1,2);}", false),
];
#[test]
fn constraints_and_profile_specific_types() {
    for target in CLANG {
        for (source, accepted) in CASES {
            assert_eq!(
                check(source, target).is_ok(),
                *accepted,
                "{target}: {source}"
            );
        }
        check("typedef int I __attribute__((aligned(16)));_Atomic(I) a;typedef __typeof__(__c11_atomic_load(&a,0)) R;_Static_assert(__alignof__(R)==16,\"alias\");",target).unwrap();
        let source = "long double f(_Atomic(long double)*p){return __c11_atomic_fetch_add(p,1,5);}";
        assert_eq!(
            check(source, target).is_ok(),
            target != Target::X86_64AppleDarwin
        );
        assert!(
            check(
                "typedef void F(void);void f(_Atomic(F*)*p){__c11_atomic_fetch_add(p,1,5);}",
                target
            )
            .unwrap_err()
            .message
            .contains("unsupported")
        );
        for source in [
            "void f(_Atomic(int)*p){__c11_atomic_load(p,3);}",
            "void f(_Atomic(int)*p){__c11_atomic_store(p,1,2);}",
            "void f(_Atomic(int)*p,int*q){__c11_atomic_compare_exchange_strong(p,q,1,0,5);}",
        ] {
            assert!(check(source, target).unwrap_err().message.contains("order"));
        }
    }
}
#[test]
fn queries_preserve_bool_type_effects_and_unknown_runtime_cases() {
    for target in CLANG {
        let source = "enum{z=__c11_atomic_is_lock_free(0),s=__c11_atomic_is_lock_free(8)};int f(unsigned n,_Atomic(int)*p){__c11_atomic_is_lock_free(n++);return __builtin_constant_p(__c11_atomic_load(p,0));}";
        let a = check(source, target).unwrap();
        assert_eq!(a.unit().constants["z"].as_u64().unwrap(), 1);
        assert_eq!(a.unit().constants["s"].as_u64().unwrap(), 1);
        assert!(check("enum{x=__c11_atomic_is_lock_free(16)};", target).is_err());
        assert!(check("enum{x=__c11_atomic_is_lock_free(3)};", target).is_err());
        check(
            "_Bool f(int n){return __c11_atomic_is_lock_free(n);}",
            target,
        )
        .unwrap();
        let mut nested = "8".to_owned();
        for _ in 0..24 {
            nested = format!("__c11_atomic_is_lock_free({nested})");
        }
        check(&format!("enum{{x={nested}}};"), target).unwrap();
    }
}
#[test]
fn pointer_operations_keep_the_single_vla_bound_and_result_identity() {
    let source = "void f(int n){typedef int A[n++];_Atomic(A*) p;A *q=__c11_atomic_load(&p,0);q=__c11_atomic_fetch_add(&p,1,5);q=__c11_atomic_exchange(&p,q,5);}";
    for target in CLANG {
        let a = check(source, target).unwrap();
        let code = a.checked().unwrap();
        assert_eq!(code.bounds().count(), 1);
        for (_, e) in code.expressions() {
            if let ExprKind::BuiltinCall {
                builtin: Builtin::C11Atomic(_),
                arguments,
                ..
            } = e.kind()
            {
                let returned = code.type_use(e.type_use()).unwrap();
                assert_eq!(returned.extents().len(), 1);
                assert_eq!(returned.extents()[0].path(), [TypeStep::Pointer]);
                let address = code.type_use(arguments[0].type_use()).unwrap();
                assert_eq!(
                    address.extents()[0].path(),
                    [TypeStep::Pointer, TypeStep::AtomicValue, TypeStep::Pointer]
                );
                assert_eq!(returned.extents()[0].bound(), address.extents()[0].bound());
            }
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
#[ignore = "requires Clang target backends"]
fn signatures_and_constraints_match_clang() {
    for target in CLANG {
        for (source, accepted) in std::iter::once((source(), true))
            .chain(CASES.iter().map(|(s, a)| ((*s).to_owned(), *a)))
        {
            let out = compiler_input(
                std::process::Command::new("clang").args([
                    "-target",
                    target.triple(),
                    "-std=gnu11",
                    "-Werror=int-conversion",
                    "-Werror=incompatible-pointer-types",
                    "-Werror=pointer-sign",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
                ]),
                &source,
            );
            assert_eq!(
                toucan_test_support::compiler_acceptance(&out),
                Ok(accepted),
                "{target}: {source}\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[test]
#[ignore = "requires native Clang and the platform atomic runtime"]
fn native_initialization_records_flags_pointer_scaling_and_vla_bounds() {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return;
    }
    let source = r#"
int main(void) {
    _Atomic(int) x; __c11_atomic_init(&x,10);
    if(__c11_atomic_load(&x,2)!=10)return 1;
    __c11_atomic_store(&x,11,3);if(__c11_atomic_exchange(&x,12,5)!=11)return 2;
    int expected=11;
    if(__c11_atomic_compare_exchange_strong(&x,&expected,13,5,2)||expected!=12||x!=12)return 3;
    if(!__c11_atomic_compare_exchange_strong(&x,&expected,13,5,2)||x!=13)return 4;
    if(__c11_atomic_fetch_add(&x,3,5)!=13||x!=16)return 5;
    if(__c11_atomic_fetch_sub(&x,3,5)!=16||x!=13)return 6;
    if(__c11_atomic_fetch_and(&x,7,5)!=13||x!=5)return 7;
    if(__c11_atomic_fetch_or(&x,8,5)!=5||x!=13)return 8;
    if(__c11_atomic_fetch_xor(&x,3,5)!=13||x!=14)return 9;
    if(__c11_atomic_fetch_nand(&x,3,5)!=14||x!=~2)return 10;
    if(__c11_atomic_fetch_max(&x,20,5)!=~2||x!=20)return 11;
    if(__c11_atomic_fetch_min(&x,10,5)!=20||x!=10)return 12;
    _Atomic(_Bool) flag;__c11_atomic_init(&flag,0);
    if(__c11_atomic_exchange(&flag,1,5)||!flag)return 13;
    __c11_atomic_store(&flag,0,3);if(flag)return 14;
    _Atomic(float) f;__c11_atomic_init(&f,2.0f);
    if(__c11_atomic_fetch_add(&f,1.5f,5)!=2.0f||f!=3.5f)return 15;
    struct S{char x[3];};struct S a={{1,2,3}},b={{4,5,6}},out;
    _Atomic(struct S) record;__c11_atomic_init(&record,a);
    out=__c11_atomic_exchange(&record,b,5);if(out.x[2]!=3)return 16;
    out=__c11_atomic_load(&record,2);if(out.x[0]!=4)return 17;
    /* Clang 18 drops expected-buffer copyback for an expanded 3-byte record.
       Keep that compiler limitation in the evidence; prove CAS with equal-size
       value/storage while retaining the 3-byte load/exchange checks above. */
    struct R{int x;};struct R before={1},after={2};_Atomic(struct R) cas;
    __c11_atomic_init(&cas,after);
    if(__c11_atomic_compare_exchange_strong(&cas,&before,after,5,2)||before.x!=2)return 18;
    int values[4];_Atomic(int*) p;__c11_atomic_init(&p,values);
    if(__c11_atomic_fetch_add(&p,1,5)!=values||p!=values+1)return 19;
    if(__c11_atomic_fetch_sub(&p,1,5)!=values+1||p!=values)return 20;
    int n=3;typedef int Row[n++];Row rows[2];_Atomic(Row*) vla;
    __c11_atomic_init(&vla,rows);if(n!=4)return 21;n=12;
    /* Clang 18 incorrectly lowers a VLA fetch-add to zero stride; that failing
       probe is persisted separately. Here verify initialization and loading. */
    if(__c11_atomic_load(&vla,0)!=rows)return 21;
    __c11_atomic_thread_fence(5);__c11_atomic_signal_fence(2);
    int size=4; if(!__c11_atomic_is_lock_free(size++)||size!=5)return 22;
    if(!__c11_atomic_is_lock_free(0))return 23;
    return 0;
}
"#;
    for target in CLANG {
        check(source, target).unwrap();
    }
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("probe.c"), source).unwrap();
    for opt in ["-O0", "-O2"] {
        let mut command = std::process::Command::new("clang");
        command
            .current_dir(directory.path())
            .args(["-std=gnu11", opt, "probe.c", "-o", "probe"]);
        if cfg!(target_os = "linux") {
            command.arg("-latomic");
        }
        let out = command.output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            std::process::Command::new(directory.path().join("probe"))
                .status()
                .unwrap()
                .success(),
            "{opt}"
        );
    }
}

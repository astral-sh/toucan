use toucan_semantic::checked::{AtomicOperation as A, Builtin, Conversion, ExprKind, UseContext};
use toucan_semantic::{
    Analysis, AnalysisOptions, IntegerKind, TypeKind, analyze, analyze_with_options,
};
use toucan_target::Target;

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
    ("__atomic_load_n(p,order)", A::Load),
    ("__atomic_load(p,q,order)", A::LoadGeneric),
    ("__atomic_store_n(p,value,order)", A::Store),
    ("__atomic_store(p,q,order)", A::StoreGeneric),
    ("__atomic_exchange_n(p,value,order)", A::Exchange),
    ("__atomic_exchange(p,q,q,order)", A::ExchangeGeneric),
    (
        "__atomic_compare_exchange_n(p,q,value,weak,order,order)",
        A::CompareExchange,
    ),
    (
        "__atomic_compare_exchange(p,q,q,weak,order,order)",
        A::CompareExchangeGeneric,
    ),
    ("__atomic_fetch_add(p,value,order)", A::FetchAdd),
    ("__atomic_fetch_sub(p,value,order)", A::FetchSub),
    ("__atomic_fetch_and(p,value,order)", A::FetchAnd),
    ("__atomic_fetch_or(p,value,order)", A::FetchOr),
    ("__atomic_fetch_xor(p,value,order)", A::FetchXor),
    ("__atomic_fetch_nand(p,value,order)", A::FetchNand),
    ("__atomic_add_fetch(p,value,order)", A::AddFetch),
    ("__atomic_sub_fetch(p,value,order)", A::SubFetch),
    ("__atomic_and_fetch(p,value,order)", A::AndFetch),
    ("__atomic_or_fetch(p,value,order)", A::OrFetch),
    ("__atomic_xor_fetch(p,value,order)", A::XorFetch),
    ("__atomic_nand_fetch(p,value,order)", A::NandFetch),
    ("__atomic_test_and_set(p,order)", A::TestAndSet),
    ("__atomic_clear(p,order)", A::Clear),
    ("__atomic_thread_fence(order)", A::ThreadFence),
    ("__atomic_signal_fence(order)", A::SignalFence),
    ("__atomic_always_lock_free(4,q++)", A::AlwaysLockFree),
    ("__atomic_is_lock_free(value,q++)", A::IsLockFree),
];
fn gnu(target: Target) -> bool {
    matches!(
        target,
        Target::X86_64UnknownLinuxGnu
            | Target::X86_64UnknownLinuxMusl
            | Target::Aarch64UnknownLinuxGnu
            | Target::Aarch64UnknownLinuxMusl
    )
}
fn source() -> String {
    let mut source =
        "void f(volatile int*p,int*q,short value,float weak,unsigned char order){".to_owned();
    for (call, _) in CALLS {
        source.push_str(call);
        source.push(';');
    }
    source.push('}');
    source
}

#[test]
fn operations_preserve_orders_conversions_and_query_evaluation() {
    for target in Target::ALL {
        let analysis = check(&source(), target).unwrap();
        let code = analysis.checked().unwrap();
        let calls: Vec<_> = code
            .expressions()
            .filter_map(|(_, expression)| {
                if let ExprKind::BuiltinCall {
                    builtin: Builtin::Atomic(operation),
                    arguments,
                    ..
                } = expression.kind()
                {
                    Some((*operation, arguments, code.ty(expression.ty()).unwrap()))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(calls.len(), CALLS.len());
        for ((_, expected), (operation, arguments, result)) in CALLS.iter().zip(calls) {
            assert_eq!(*expected, operation);
            for argument in arguments {
                assert_eq!(
                    argument.context(),
                    if operation == A::AlwaysLockFree {
                        UseContext::UnevaluatedValue
                    } else {
                        UseContext::Value
                    }
                );
                assert!(
                    !argument
                        .conversions()
                        .iter()
                        .any(|c| c.kind() == Conversion::DefaultArgument)
                );
            }
            if let Some((order, failure)) = operation.memory_order_arguments() {
                for index in [Some(order), failure].into_iter().flatten() {
                    assert_eq!(
                        code.ty(arguments[index].effective_type()).unwrap().kind,
                        TypeKind::Integer(IntegerKind::Int)
                    );
                }
            }
            if let Some(index) = operation.weak_argument() {
                assert!(matches!(
                    code.ty(arguments[index].effective_type()).unwrap().kind,
                    TypeKind::Bool
                ));
                assert!(matches!(result.kind, TypeKind::Bool));
            }
            if operation.is_lock_free_query() {
                assert!(matches!(result.kind, TypeKind::Bool));
            }
        }
    }
}

// GCC language acceptance and Clang pointer constraints. GCC qualifier-discard
// extensions remain accepted, with their conversions retained explicitly.
const CASES: &[(&str, bool, bool)] = &[
    (
        "int f(const int*p){return __atomic_load_n(p,0);}",
        true,
        true,
    ),
    ("void f(const int*p){__atomic_store_n(p,1,0);}", true, false),
    ("void f(int**p){__atomic_store_n(p,7,0);}", true, false),
    (
        "void f(int**p,const int*q){__atomic_store_n(p,q,0);}",
        true,
        false,
    ),
    ("void f(float*p){__atomic_load_n(p,0);}", false, false),
    ("void f(_Bool*p){__atomic_fetch_add(p,1,0);}", false, true),
    ("void f(float*p){__atomic_fetch_add(p,1,0);}", false, true),
    ("void f(double*p){__atomic_sub_fetch(p,1,0);}", false, true),
    ("void f(float*p){__atomic_fetch_or(p,1,0);}", false, false),
    ("void f(int**p){__atomic_fetch_add(p,1,0);}", true, true),
    ("void f(int**p){__atomic_fetch_or(p,1,0);}", true, false),
    ("void f(float*p,int*q){__atomic_load(p,q,0);}", true, false),
    (
        "void f(double*p,int*q){__atomic_load(p,q,0);}",
        false,
        false,
    ),
    ("void f(int*p,void*q){__atomic_load(p,q,0);}", false, true),
    (
        "void f(int*p,const int*q){__atomic_store(p,q,0);}",
        true,
        false,
    ),
    (
        "void f(int*p,volatile int*q){__atomic_load(p,q,0);}",
        true,
        false,
    ),
    (
        "void f(int*p,const int*q){__atomic_load(p,q,0);}",
        true,
        false,
    ),
    (
        "void f(int*p,unsigned*q){__atomic_compare_exchange_n(p,q,1,0,5,2);}",
        true,
        false,
    ),
    (
        "void f(int*p,const int*q){__atomic_compare_exchange_n(p,q,1,0,5,2);}",
        true,
        false,
    ),
    (
        "void f(void*p){__atomic_test_and_set(p,5);__atomic_clear(p,3);}",
        true,
        true,
    ),
    ("void f(const char*p){__atomic_clear(p,3);}", true, false),
    (
        "void f(float order){__atomic_thread_fence(order);}",
        true,
        true,
    ),
    (
        "_Bool f(int n,void*p){return __atomic_always_lock_free(n,p);}",
        false,
        false,
    ),
    (
        "void f(int n,void*p){__atomic_is_lock_free(n,p);}",
        true,
        true,
    ),
    (
        "struct S{char v[3];};void f(struct S*p,struct S*q){__atomic_load(p,q,0);__atomic_store(p,q,0);__atomic_exchange(p,q,q,0);__atomic_compare_exchange(p,q,q,0,5,2);}",
        true,
        true,
    ),
    (
        "void f(int(*p)[3],int(*q)[3]){__atomic_load(p,q,0);}",
        true,
        true,
    ),
    ("void f(int*p){__atomic_load_n(p,0,1);}", false, false),
    ("void f(int*p){__atomic_store_n(p);}", false, false),
    (
        "int f(int(*__atomic_load_n)(int,int)){return __atomic_load_n(1,2);}",
        true,
        true,
    ),
];
#[test]
fn overloads_follow_the_selected_compiler_profile() {
    for &(source, gcc, clang) in CASES {
        for target in Target::ALL {
            let result = check(source, target);
            assert_eq!(
                result.is_ok(),
                if gnu(target) { gcc } else { clang },
                "{target} {source}: {result:?}"
            );
        }
    }
}
#[test]
fn orders_are_checked_without_inventing_dynamic_values() {
    for call in [
        "__atomic_load_n(p,3)",
        "__atomic_store_n(p,1,2)",
        "__atomic_clear(p,4)",
        "__atomic_load_n(p,-1)",
        "__atomic_load_n(p,9)",
        "__atomic_load_n(p,2|65536)",
        "__atomic_compare_exchange_n(p,p,1,0,0,5)",
        "__atomic_compare_exchange_n(p,p,1,0,5,3)",
    ] {
        let source = format!("void f(int*p){{{call};}}");
        for target in Target::ALL {
            assert!(check(&source, target).is_err(), "{target} {call}");
        }
    }
    for target in Target::ALL {
        check(
            "void f(int*p,int m){__atomic_store_n(p,1,m);__atomic_load_n(p,4294967296ULL);}",
            target,
        )
        .unwrap();
    }
}
#[test]
fn lock_free_constants_require_a_proof_and_mutations_never_fold() {
    let source = "_Static_assert(__atomic_always_lock_free(4,0)==1,\"\");_Static_assert(__atomic_is_lock_free(8,0)==1,\"\");_Static_assert(__atomic_always_lock_free(3,0)==0,\"\");";
    for target in Target::ALL {
        check(source, target).unwrap();
        let unit = analyze("", target).unwrap();
        for (query, expected) in [
            ("__atomic_always_lock_free(4,0)", 1),
            ("__atomic_is_lock_free(8,0)", 1),
            ("__atomic_always_lock_free(3,0)", 0),
        ] {
            let toucan_semantic::ArithmeticConstant::Integer(value) =
                toucan_semantic::evaluate_arithmetic(&unit, query).unwrap()
            else {
                panic!()
            };
            assert_eq!(
                (value.value, value.bits, value.signed, value.rank),
                (expected, 8, false, 0)
            );
        }
        check("_Static_assert(__atomic_always_lock_free(-1,0)==0,\"\");_Static_assert(__atomic_always_lock_free(4.0,0)==1,\"\");",target).unwrap();
        if target != Target::X86_64PcWindowsMsvc {
            assert!(
                check(
                    "struct S{};void f(struct S*p){__atomic_load(p,p,0);}",
                    target
                )
                .is_err()
            );
        }
        for source in [
            "enum{N=__atomic_always_lock_free(16,0)};",
            "int x;enum{N=__atomic_load_n(&x,0)};",
            "int*p;enum{N=__atomic_is_lock_free(0,p++)};",
        ] {
            assert!(check(source, target).is_err(), "{source}");
        }
        check(
            "void f(int*p){__atomic_always_lock_free(16,p);__atomic_always_lock_free(4,p);}",
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
#[ignore = "requires GNU GCC and Clang target backends"]
fn signatures_and_rejections_match_native_gcc_and_cross_target_clang() {
    use std::process::Command;
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        identity.status.success() && !String::from_utf8_lossy(&identity.stdout).contains("clang")
    );
    for (source, accepted_gcc, accepted_clang) in std::iter::once((source(), true, true))
        .chain(CASES.iter().map(|(s, g, c)| ((*s).to_owned(), *g, *c)))
    {
        for target in Target::ALL {
            let output = compiler_input(
                Command::new("clang").args([
                    "-target",
                    target.triple(),
                    "-std=gnu11",
                    "-Werror=int-conversion",
                    "-Werror=incompatible-pointer-types",
                    "-Werror=pointer-sign",
                    "-S",
                    "-emit-llvm",
                    "-o",
                    "/dev/null",
                    "-x",
                    "c",
                    "-",
                ]),
                &source,
            );
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(accepted_clang),
                "Clang {target} {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let output = compiler_input(
            Command::new(&gcc).args(["-std=gnu11", "-S", "-o", "/dev/null", "-x", "c", "-"]),
            &source,
        );
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(accepted_gcc),
            "GCC {source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
#[test]
#[ignore = "requires native GNU GCC and Clang and the platform atomic runtime"]
fn native_operations_and_queries_have_the_retained_semantics() {
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let mut source = "int main(void){unsigned x=10,r;\n".to_owned();
    for (operation, result) in [
        ("add", "10u+3u"),
        ("sub", "10u-3u"),
        ("and", "10u&3u"),
        ("or", "10u|3u"),
        ("xor", "10u^3u"),
        ("nand", "~(10u&3u)"),
    ] {
        source.push_str(&format!("x=10;r=__atomic_fetch_{operation}(&x,3u,5);if(r!=10||x!=({result}))return 1;\nx=10;r=__atomic_{operation}_fetch(&x,3u,5);if(r!=({result})||x!=r)return 2;\n"));
    }
    source.push_str("__atomic_store_n(&x,10,3);if(__atomic_load_n(&x,2)!=10)return 3;if(__atomic_exchange_n(&x,3,5)!=10||x!=3)return 4;unsigned e=2;if(__atomic_compare_exchange_n(&x,&e,7,0,5,2)||e!=3||x!=3)return 5;if(!__atomic_compare_exchange_n(&x,&e,7,0,5,2)||x!=7)return 6;\nstruct S{char v[3];};struct S a={{1,2,3}},b={{4,5,6}},c={{0,0,0}};__atomic_load(&a,&c,2);if(c.v[2]!=3)return 7;__atomic_store(&a,&b,3);if(a.v[0]!=4)return 8;__atomic_exchange(&a,&c,&b,5);if(a.v[0]!=1||b.v[0]!=4)return 9;if(__atomic_compare_exchange(&a,&b,&c,0,5,2)||b.v[0]!=1)return 10;if(!__atomic_compare_exchange(&a,&b,&c,0,5,2))return 11;\nunsigned char flag=0;if(__atomic_test_and_set(&flag,5)||flag!=1||!__atomic_test_and_set(&flag,5))return 12;__atomic_clear(&flag,3);if(flag)return 13;__atomic_thread_fence(5);__atomic_signal_fence(5);\nint data[4];int*p=data;int*old=__atomic_fetch_add(&p,1,5);if(old!=data||(char*)p!=(char*)data+1)return 14;char*q=(char*)data;__atomic_always_lock_free(4,q++);if(q!=(char*)data)return 15;__atomic_is_lock_free(4,q++);if(q!=(char*)data+1)return 16;return 0;}\n");
    for target in Target::ALL {
        check(&source, target).unwrap();
    }
    source=source.replace("int main(void)", "int qualifier_probe(void);int main(void)").replace("return 0;}", "\n#if defined(__GNUC__) && !defined(__clang__)\nreturn qualifier_probe();\n#else\nreturn 0;\n#endif\n}");
    source.push_str("\n#if defined(__GNUC__) && !defined(__clang__)\nint qualifier_probe(void){int x=1,e=0;const volatile int*p=&x,*q=&e;__atomic_store_n(p,2,0);if(__atomic_fetch_add(p,1,5)!=2)return 17;e=3;if(!__atomic_compare_exchange_n(p,q,4,0,5,2))return 18;__atomic_load(p,q,0);return e!=4;}\n#endif\n");
    std::fs::write(directory.path().join("probe.c"), source).unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc, "clang".into()] {
        for optimization in ["-O0", "-O2"] {
            let mut command = Command::new(&compiler);
            command.current_dir(directory.path()).args([
                "-std=gnu11",
                optimization,
                "probe.c",
                "-o",
                "probe",
            ]);
            if cfg!(target_os = "linux") {
                command.arg("-latomic");
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                Command::new(directory.path().join("probe"))
                    .status()
                    .unwrap()
                    .success(),
                "{compiler} {optimization}"
            );
        }
    }
}

#[test]
fn nested_atomic_operands_are_checked_without_exponential_replay() {
    let mut address = "p".to_owned();
    for _ in 0..24 {
        address = format!("__atomic_load_n((int**){address},0)");
    }
    let mut query = "4".to_owned();
    for _ in 0..24 {
        query = format!("__atomic_always_lock_free({query},0)");
    }
    for target in Target::ALL {
        check(&format!("void f(int*p){{{address};{query};}}"), target).unwrap();
    }
}

#[test]
fn gcc_atomic_qualifier_erasure_is_visible_in_retained_uses() {
    let source = "void f(const volatile int*p,const volatile int*q){__atomic_store_n(p,3,0);__atomic_compare_exchange_n(p,q,4,0,5,2);__atomic_load(p,q,0);}";
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        let analysis = check(source, target).unwrap();
        let code = analysis.checked().unwrap();
        for (_, expression) in code.expressions() {
            if let ExprKind::BuiltinCall {
                builtin: Builtin::Atomic(operation),
                arguments,
                ..
            } = expression.kind()
            {
                let modified = if *operation == A::LoadGeneric { 1 } else { 0 };
                let argument = &arguments[modified];
                assert!(
                    argument
                        .conversions()
                        .iter()
                        .any(|c| c.kind() == Conversion::IntrinsicArgument)
                );
                let TypeKind::Pointer(pointee) = &code.ty(argument.effective_type()).unwrap().kind
                else {
                    panic!()
                };
                let qualifiers = analysis.unit().qualifiers(pointee).unwrap();
                assert!(!qualifiers.is_const);
                assert_eq!(qualifiers.is_volatile, modified == 0);
            }
        }
    }
}

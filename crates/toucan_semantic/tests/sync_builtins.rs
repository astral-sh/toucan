use toucan_semantic::checked::{Builtin, Conversion, ExprKind, SyncOperation, UseContext};
use toucan_semantic::{Analysis, AnalysisOptions, TypeKind, analyze, analyze_with_options};
use toucan_target::Target;

const OPS: &[(&str, SyncOperation, usize)] = &[
    ("fetch_and_add", SyncOperation::FetchAdd, 2),
    ("fetch_and_sub", SyncOperation::FetchSub, 2),
    ("fetch_and_or", SyncOperation::FetchOr, 2),
    ("fetch_and_and", SyncOperation::FetchAnd, 2),
    ("fetch_and_xor", SyncOperation::FetchXor, 2),
    ("fetch_and_nand", SyncOperation::FetchNand, 2),
    ("add_and_fetch", SyncOperation::AddFetch, 2),
    ("sub_and_fetch", SyncOperation::SubFetch, 2),
    ("or_and_fetch", SyncOperation::OrFetch, 2),
    ("and_and_fetch", SyncOperation::AndFetch, 2),
    ("xor_and_fetch", SyncOperation::XorFetch, 2),
    ("nand_and_fetch", SyncOperation::NandFetch, 2),
    (
        "bool_compare_and_swap",
        SyncOperation::BoolCompareAndSwap,
        3,
    ),
    (
        "val_compare_and_swap",
        SyncOperation::ValueCompareAndSwap,
        3,
    ),
    ("lock_test_and_set", SyncOperation::LockTestAndSet, 2),
    ("lock_release", SyncOperation::LockRelease, 1),
    ("synchronize", SyncOperation::Synchronize, 0),
];
fn gnu(target: Target) -> bool {
    matches!(
        target,
        Target::I686UnknownLinuxGnu
            | Target::X86_64UnknownLinuxGnu
            | Target::X86_64UnknownLinuxMusl
            | Target::Aarch64UnknownLinuxGnu
            | Target::Aarch64UnknownLinuxMusl
    )
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
        (Err(a), Err(b)) => {
            assert_eq!(a.message, b.message);
            assert_eq!(a.offset, b.offset);
        }
        _ => panic!("retention changed acceptance: {ordinary:?} {retained:?}"),
    }
    retained
}
fn call(name: &str, count: usize, extra: bool) -> String {
    let mut args = match count {
        0 => vec![],
        1 => vec!["p"],
        2 => vec!["p", "1"],
        _ => vec!["p", "0", "1"],
    };
    if extra && count > 0 {
        args.extend(["n++", "(void)0", "sizeof(int[++n])"]);
    }
    format!("__sync_{name}({})", args.join(","))
}
fn source() -> String {
    let mut source = "void f(volatile int*p,int n){\n".to_owned();
    for (name, _, count) in OPS {
        source.push_str(&format!("{};\n", call(name, *count, true)));
    }
    source.push_str("}\n");
    source
}
#[test]
fn operations_keep_result_types_and_unevaluated_extra_operands() {
    let source = source();
    for target in Target::ALL {
        let analysis = check(&source, target).unwrap();
        let code = analysis.checked().unwrap();
        let calls: Vec<_> = code
            .expressions()
            .filter_map(|(_, expr)| {
                if let ExprKind::BuiltinCall {
                    builtin: Builtin::Sync(op),
                    arguments,
                    ..
                } = expr.kind()
                {
                    Some((*op, arguments, expr))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(calls.len(), OPS.len());
        for ((_, expected, count), (op, args, expr)) in OPS.iter().zip(calls) {
            assert_eq!(*expected, op);
            match op {
                SyncOperation::BoolCompareAndSwap => {
                    assert!(matches!(code.ty(expr.ty()).unwrap().kind, TypeKind::Bool))
                }
                SyncOperation::LockRelease | SyncOperation::Synchronize => {
                    assert!(matches!(code.ty(expr.ty()).unwrap().kind, TypeKind::Void))
                }
                _ => assert!(matches!(
                    code.ty(expr.ty()).unwrap().kind,
                    TypeKind::Integer(_)
                )),
            }
            for arg in &args[*count..] {
                assert_eq!(
                    arg.context(),
                    if gnu(target) {
                        UseContext::UnevaluatedValue
                    } else {
                        UseContext::Unevaluated
                    }
                );
                assert!(
                    !arg.conversions()
                        .iter()
                        .any(|step| step.kind() == Conversion::DefaultArgument)
                );
            }
        }
    }
}
#[test]
fn gcc_and_clang_overload_rules_are_explicit() {
    for target in Target::ALL {
        for source in [
            "int f(const int*p){return __sync_fetch_and_add(p,1);}",
            "int*f(int**p){return __sync_fetch_and_add(p,1);}",
            "int f(int*p,void*v){return __sync_fetch_and_add(p,v);}",
            "int*f(int**p,const int*v){return __sync_lock_test_and_set(p,v);}",
            "int*f(int**p,char*v){return __sync_lock_test_and_set(p,v);}",
        ] {
            assert_eq!(
                check(source, target).is_ok(),
                gnu(target),
                "{target}: {source}"
            );
        }
        for source in [
            "__int128 f(__int128*p){return __sync_fetch_and_add(p,1);}",
            "enum E{A,B}; enum E f(enum E*p){return __sync_fetch_and_add(p,1);}",
            "typedef void(*F)(void);F f(F*p,F v){return __sync_lock_test_and_set(p,v);}",
            "_Bool f(_Bool*p){return __sync_bool_compare_and_swap(p,0,1);}",
            "void f(_Bool*p){__sync_lock_release(p);}",
            "int f(int (*__sync_fetch_and_add)(int,int)){return __sync_fetch_and_add(1,2);}",
        ] {
            if target == Target::I686UnknownLinuxGnu && source.contains("__int128") {
                assert!(check(source, target).is_err(), "{target}: {source}");
                continue;
            }
            check(source, target).unwrap_or_else(|error| panic!("{target}: {source}: {error}"));
        }
        for (name, _, count) in OPS.iter().filter(|(_, _, n)| *n == 2) {
            let source = format!("void f(_Bool*p){{{};}}", call(name, *count, false));
            let valid = !gnu(target) || *name == "lock_test_and_set";
            assert_eq!(check(&source, target).is_ok(), valid, "{target}: {name}");
        }
        let source = "void f(int*p){register int a[1];__sync_fetch_and_add(p,1,a);}";
        assert_eq!(check(source, target).is_ok(), !gnu(target));
        let alignment = if gnu(target) { 4 } else { 16 };
        check(&format!("typedef int I __attribute__((aligned(16))); int f(I*p){{_Static_assert(_Alignof(__typeof__(__sync_fetch_and_add(p,1)))=={alignment},\"alignment\");return 0;}}"),target).unwrap();
    }
}
#[test]
fn intrinsic_conversions_preserve_gcc_pointer_bridges() {
    let source = "int f(const int*p,void*v){return __sync_fetch_and_add(p,v);}";
    let analysis = check(source, Target::X86_64UnknownLinuxGnu).unwrap();
    let code = analysis.checked().unwrap();
    let (_, expr) = code
        .expressions()
        .find(|(_, e)| {
            matches!(
                e.kind(),
                ExprKind::BuiltinCall {
                    builtin: Builtin::Sync(_),
                    ..
                }
            )
        })
        .unwrap();
    let ExprKind::BuiltinCall { arguments, .. } = expr.kind() else {
        unreachable!()
    };
    for argument in arguments {
        assert_eq!(
            argument.conversions().last().unwrap().kind(),
            Conversion::IntrinsicArgument
        );
    }
    let TypeKind::Pointer(pointee) = &code.ty(arguments[0].effective_type()).unwrap().kind else {
        panic!("address")
    };
    assert!(!pointee.qualifiers.is_const);
    assert!(matches!(
        code.ty(arguments[1].effective_type()).unwrap().kind,
        TypeKind::Integer(_)
    ));
}
#[test]
fn invalid_atomic_uses_and_constant_contexts_are_rejected() {
    for target in Target::ALL {
        for source in [
            "void f(void){__sync_synchronize(1);}",
            "void f(int*p){__sync_fetch_and_add(p);}",
            "void f(void){__sync_lock_release();}",
            "void f(float*p){__sync_lock_test_and_set(p,1);}",
            "struct S{int x;};void f(struct S*p){__sync_lock_release(p);}",
            "void f(void*p){__sync_lock_release(p);}",
            "void f(int*p){__sync_fetch_and_add(p,1,missing);}",
            "void f(int**p){__sync_fetch_and_add(p,1.0);}",
            "int x;enum{A=__sync_fetch_and_add(&x,1)};",
            "int x;static int y=__sync_fetch_and_add(&x,1);",
            "int f(int*p){return __sync_fetch_and_add_4(p,1);}",
        ] {
            assert!(check(source, target).is_err(), "{target}: {source}");
        }
        check("int x;_Static_assert(__builtin_constant_p(__sync_fetch_and_add(&x,1))==0,\"effectful\");",target).unwrap();
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
        .expect("test compiler must be installed");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn signature_source(ty: &str, name: &str, count: usize) -> String {
    let call = call(name, count, false)
        .replace(",1", ",value")
        .replace(",0", ",value");
    let expected = if name == "bool_compare_and_swap" {
        "_Bool"
    } else {
        ty
    };
    let body = if matches!(name, "lock_release" | "synchronize") {
        format!("{call};")
    } else {
        format!("_Static_assert(_Generic({call},{expected}:1,default:0),\"result type\");")
    };
    format!("void test({ty} *p,{ty} value){{{body}}}\n")
}
#[test]
#[ignore = "requires GCC and Clang with all five target backends; run with --include-ignored"]
fn sync_signatures_match_native_gcc_and_cross_target_clang() {
    use std::process::Command;
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        identity.status.success() && !String::from_utf8_lossy(&identity.stdout).contains("clang"),
        "set TOUCAN_GCC to genuine GNU GCC"
    );
    let common_types = [
        "signed char",
        "unsigned short",
        "int",
        "unsigned long long",
        "__int128",
        "void *",
    ];
    for ty in common_types {
        let mut source = String::new();
        for (index, (name, _, count)) in OPS.iter().enumerate() {
            source.push_str(
                &signature_source(ty, name, *count)
                    .replace("void test(", &format!("void test_{index}(")),
            );
        }
        for target in Target::ALL {
            check(&source, target).unwrap();
            let output = compiler_input(
                Command::new("clang").args([
                    "-target",
                    target.triple(),
                    "-std=gnu11",
                    "-Werror=int-conversion",
                    "-Werror=incompatible-pointer-types",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
                ]),
                &source,
            );
            assert!(
                output.status.success(),
                "{target} {ty}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let output = compiler_input(
            Command::new(&gcc).args([
                "-std=gnu11",
                "-Werror=int-conversion",
                "-Werror=incompatible-pointer-types",
                "-fsyntax-only",
                "-x",
                "c",
                "-",
            ]),
            &source,
        );
        assert!(
            output.status.success(),
            "GCC {ty}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for source in [
        "int f(const int*p){return __sync_fetch_and_add(p,1);}",
        "int*f(int**p){return __sync_fetch_and_add(p,1);}",
        "int f(int*p,void*v){return __sync_fetch_and_add(p,v);}",
        "int*f(int**p,const int*v){return __sync_lock_test_and_set(p,v);}",
        "int*f(int**p,char*v){return __sync_lock_test_and_set(p,v);}",
    ] {
        let source = format!("{source}\n");
        let output = compiler_input(
            Command::new(&gcc).args([
                "-std=gnu11",
                "-Werror=int-conversion",
                "-Werror=incompatible-pointer-types",
                "-fsyntax-only",
                "-x",
                "c",
                "-",
            ]),
            &source,
        );
        assert!(
            output.status.success(),
            "GCC: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        for target in Target::ALL {
            let output = compiler_input(
                Command::new("clang").args([
                    "-target",
                    target.triple(),
                    "-std=gnu11",
                    "-Werror=int-conversion",
                    "-Werror=incompatible-pointer-types",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
                ]),
                &source,
            );
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(false),
                "Clang accepted {target}: {source}"
            );
        }
    }
    for (name, _, count) in OPS {
        let source = signature_source("_Bool", name, *count);
        for target in Target::ALL {
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
            assert!(
                output.status.success(),
                "{target} {name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let output = compiler_input(
            Command::new(&gcc).args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"]),
            &source,
        );
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(matches!(
                *name,
                "bool_compare_and_swap"
                    | "val_compare_and_swap"
                    | "lock_test_and_set"
                    | "lock_release"
                    | "synchronize"
            )),
            "GCC {name}"
        );
    }
}

#[test]
#[ignore = "requires native GCC and Clang; verifies the runtime behavior described by the retained operation kinds"]
fn compiler_runtime_probes_confirm_operation_and_extra_argument_semantics() {
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let mut source = "int main(void){unsigned x=10,old=0;int ignored=0;\n".to_owned();
    let binary = [
        ("add", "10u+3u"),
        ("sub", "10u-3u"),
        ("or", "10u|3u"),
        ("and", "10u&3u"),
        ("xor", "10u^3u"),
        ("nand", "~(10u&3u)"),
    ];
    for (operation, result) in binary {
        source.push_str(&format!("x=10;old=__sync_fetch_and_{operation}(&x,3u,ignored++,sizeof(int[++ignored]));if(old!=10u||x!=({result})||ignored)return 1;\n"));
        source.push_str(&format!("x=10;old=__sync_{operation}_and_fetch(&x,3u,ignored++,sizeof(int[++ignored]));if(old!=({result})||x!=({result})||ignored)return 2;\n"));
    }
    source.push_str("x=10;if(!__sync_bool_compare_and_swap(&x,10,3,ignored++)||x!=3||ignored)return 3;\nx=10;if(__sync_bool_compare_and_swap(&x,9,3,ignored++)||x!=10||ignored)return 4;\nx=10;if(__sync_val_compare_and_swap(&x,10,3,ignored++)!=10||x!=3||ignored)return 5;\nx=10;if(__sync_lock_test_and_set(&x,3,ignored++)!=10||x!=3||ignored)return 6;\n__sync_lock_release(&x,ignored++);if(x||ignored)return 7;\n__sync_synchronize();\nint storage[4];int *p=storage;int *before=__sync_fetch_and_add(&p,(int*)1);if(before!=storage||(char*)p!=(char*)storage+1)return 8;\nreturn 0;}\n");
    for target in Target::ALL {
        check(&source, target).unwrap();
    }
    std::fs::write(directory.path().join("probe.c"), source).unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc, "clang".into()] {
        for optimization in ["-O0", "-O2"] {
            let output = Command::new(&compiler)
                .current_dir(directory.path())
                .args(["-std=gnu11", optimization, "probe.c", "-o", "probe"])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler} {optimization}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                Command::new(directory.path().join("probe"))
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
}

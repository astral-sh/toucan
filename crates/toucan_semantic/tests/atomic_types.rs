use toucan_semantic::{Analysis, AnalysisOptions, TypeKind, analyze, analyze_with_options};
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
        _ => panic!(
            "retention changed acceptance: {:?} {:?}",
            ordinary.as_ref().err(),
            retained.as_ref().err()
        ),
    }
    retained
}
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

#[test]
fn atomic_values_layout_and_operator_types() {
    let source = r#"
    typedef _Atomic(int) A;
    struct Three { char bytes[3]; };
    typedef _Atomic(struct Three) AtomicThree;
    _Atomic(int) scalar = 3;
    const _Atomic(int) constant = 4;
    _Atomic(int*) pointer;
    int f(A *p, _Atomic(float)*q) {
        int a = *p; *p = a; *p += 2; (*p)++; ++(*p); *q += 1.0f;
        _Static_assert(_Generic(*p, int:1, A:0), "value conversion");
        _Static_assert(_Generic((*p=1), int:1, A:0), "assignment type");
        _Static_assert(_Generic((*p)++, int:1, A:0), "increment type");
        return a;
    }
    "#;
    for target in Target::ALL {
        let analysis = check(source, target).unwrap_or_else(|e| panic!("{target:?}: {e:?}"));
        let unit = analysis.unit();
        let layout = unit.layout(&unit.typedefs["AtomicThree"]).unwrap();
        assert_eq!(layout.size_bytes(), if gnu(target) { 3 } else { 4 });
        assert_eq!(layout.alignment_bytes(), if gnu(target) { 1 } else { 4 });
        assert!(matches!(unit.typedefs["A"].kind, TypeKind::Atomic(_)));
    }
}
#[test]
fn atomic_type_constraints_follow_the_compiler_profile() {
    let shared_good = [
        "const _Atomic(int) a;",
        "_Atomic _Atomic int a;",
        "typedef _Atomic(int) A; _Atomic A a;",
        "_Atomic(int) a[3];",
        "void f(int n){_Atomic(int(*)[n]) a;}",
        "struct S{int a;}; void f(_Atomic(struct S)*p,struct S s){*p=s;s=*p;}",
        "void f(_Atomic(int*)*p){(*p)++;}",
        "struct S{int a;}; _Atomic(struct S) a=(struct S){1};",
        "typedef const int I; _Atomic I a;",
        "_Atomic(int*) a;",
        "int * _Atomic a;",
    ];
    let shared_bad = [
        "_Atomic(const int) a;",
        "_Atomic(volatile int) a;",
        "_Atomic(_Atomic(int)) a;",
        "_Atomic(int[3]) a;",
        "typedef int A[3]; _Atomic A a;",
        "_Atomic(int(void)) a;",
        "struct S{_Atomic(int) a:3;};",
        "extern _Atomic(int) a; extern int a;",
        "void f(_Atomic(int)); void f(int);",
        "void f(_Atomic(int)*a,int*b){b=a;}",
        "struct S{int a;};int f(_Atomic(struct S)*p){return p->a;}",
    ];
    let gnu_good = [
        "struct S;_Atomic(struct S)*p;",
        "_Atomic(void)*p;",
        "struct S{int a;};_Atomic(struct S) a={1};",
        "void f(_Atomic(int*)*p){*p+=1;}",
        "int f(int x){return (_Atomic(int))x;}",
        "_Atomic(int) f(void);int g(void){return f();}",
        "int f(_Atomic(int)*p){return __atomic_load_n(p,5);}",
    ];
    for target in Target::ALL {
        for source in shared_good {
            check(source, target).unwrap_or_else(|e| panic!("{target:?}: {source}: {e:?}"));
        }
        for source in shared_bad {
            assert!(check(source, target).is_err(), "{target:?}: {source}");
        }
        for source in gnu_good {
            assert_eq!(
                check(source, target).is_ok(),
                gnu(target),
                "{target:?}: {source}"
            );
        }
    }
}

#[test]
fn atomic_accesses_keep_single_updates_and_query_suppression() {
    use toucan_semantic::checked::{
        AtomicAccess, Binary, Builtin, Conversion, ExprKind, QueryEvaluation, QuerySuppression,
        UseContext,
    };
    let source = "int f(_Atomic(short)*p) { _Atomic(int) initialized=1; int a=*p; *p=3; *p+=2; (*p)++; ++(*p); *p; sizeof(*p); __builtin_constant_p(*p); return a; }";
    for target in Target::ALL {
        let analysis = check(source, target).unwrap();
        let code = analysis.checked().unwrap();
        let accesses = code
            .expressions()
            .filter_map(|(_, e)| e.atomic_access())
            .collect::<Vec<_>>();
        assert_eq!(
            accesses,
            [
                AtomicAccess::Store,
                AtomicAccess::ReadModifyWrite,
                AtomicAccess::ReadModifyWrite,
                AtomicAccess::ReadModifyWrite
            ]
        );
        let mut updates = 0;
        let mut queries = 0;
        for (_, expression) in code.expressions() {
            if let ExprKind::Binary {
                operator: Binary::AssignPlus,
                left,
                write_back: Some(back),
                ..
            }
            | ExprKind::Unary {
                operand: left,
                write_back: Some(back),
                ..
            } = expression.kind()
            {
                assert_eq!(left.context(), UseContext::ReadModifyWrite);
                assert_eq!(left.conversions()[0].kind(), Conversion::AtomicLoad);
                assert!(matches!(code.ty(*back).unwrap().kind, TypeKind::Atomic(_)));
                updates += 1;
            }
            if let ExprKind::BuiltinCall {
                builtin: Builtin::ConstantQuery,
                query_evaluation: Some(policy),
                arguments,
                ..
            } = expression.kind()
            {
                assert_eq!(
                    *policy,
                    QueryEvaluation::Unevaluated(if gnu(target) {
                        QuerySuppression::GnuProfile
                    } else {
                        QuerySuppression::OrdinarySideEffects
                    })
                );
                assert_eq!(arguments[0].conversions()[0].kind(), Conversion::AtomicLoad);
                queries += 1;
            }
        }
        assert_eq!(updates, 3);
        assert_eq!(queries, 1);
    }
}

#[test]
fn atomic_vla_type_uses_preserve_bound_identity_and_load_projection() {
    use toucan_semantic::checked::{Conversion, ExprKind, TypeStep};
    let source = "void f(int n) { typedef int (*P)[n++]; _Atomic(P) a; P plain=a; int (* _Atomic b)[n++]; int a2[2][n]; _Atomic(int (*)[n]) c=a2; }";
    for target in Target::ALL {
        let analysis = check(source, target).unwrap();
        let code = analysis.checked().unwrap();
        let mut projected = false;
        for (_, use_) in code.type_uses() {
            for extent in use_.extents() {
                let mut ty = code.ty(use_.shape()).unwrap();
                for step in extent.path() {
                    ty = match (step, &analysis.unit().resolve(ty).unwrap().kind) {
                        (TypeStep::AtomicValue, TypeKind::Atomic(value))
                        | (TypeStep::Pointer, TypeKind::Pointer(value))
                        | (TypeStep::Element, TypeKind::Array { element: value, .. })
                        | (TypeStep::Element, TypeKind::VariableArray { element: value, .. }) => {
                            value
                        }
                        _ => panic!("invalid atomic bound path: {step:?} {ty:?}"),
                    };
                }
                assert!(matches!(
                    ty.kind,
                    TypeKind::Array { .. } | TypeKind::VariableArray { .. }
                ));
            }
        }
        for (_, e) in code.expressions() {
            if let ExprKind::Name(_) = e.kind() {
                projected |= analysis
                    .unit()
                    .atomic_value(code.ty(e.ty()).unwrap())
                    .unwrap()
                    .is_some();
            }
        }
        assert!(projected);
        assert!(code.assignment_conversions().any(|(_, a)| {
            a.conversions()
                .iter()
                .any(|c| c.kind() == Conversion::AtomicLoad)
        }));
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
#[ignore = "requires native GNU GCC and Clang cross-target LLVM support"]
fn atomic_layouts_match_the_target_compiler_profiles() {
    use std::process::Command;
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        identity.status.success() && !String::from_utf8_lossy(&identity.stdout).contains("clang")
    );
    let mut types = vec![
        "_Bool".to_owned(),
        "char".into(),
        "short".into(),
        "int".into(),
        "long".into(),
        "long long".into(),
        "float".into(),
        "double".into(),
        "long double".into(),
        "void*".into(),
        "__int128".into(),
    ];
    for n in [0, 1, 2, 3, 4, 5, 7, 8, 9, 12, 15, 16, 17, 24, 32, 33] {
        types.push(format!("struct {{ char value[{n}]; }}"));
    }
    for target in Target::ALL {
        // The GNU Linux profile is checked by the native GCC Linux driver.
        // Clang triples exercise the Darwin and Microsoft compiler profiles.
        if gnu(target) && !cfg!(target_os = "linux") {
            continue;
        }
        let mut source = String::new();
        for (index, ty) in types.iter().enumerate() {
            let declaration = format!(
                "typedef _Atomic({ty}) A{index}; struct H{index}{{char prefix;A{index} value;}};"
            );
            let unit = analyze(&declaration, target).unwrap();
            let atomic = unit.layout(&unit.typedefs[&format!("A{index}")]).unwrap();
            let host = unit
                .records
                .iter()
                .position(|r| r.name.as_deref() == Some(&format!("H{index}")))
                .unwrap();
            let host = unit
                .layout(&toucan_semantic::Type::new(TypeKind::Record(host)))
                .unwrap();
            source.push_str(&declaration);
            source.push_str(&format!("_Static_assert(sizeof(A{index})=={},\"size\");_Static_assert(_Alignof(A{index})=={},\"alignment\");_Static_assert(__builtin_offsetof(struct H{index},value)=={},\"field offset\");",atomic.size_bytes(),atomic.alignment_bytes(),host.fields[1].as_ref().unwrap().offset_bits/8));
        }
        check(&source, target).unwrap();
        let output = if gnu(target) {
            compiler_input(
                Command::new(&gcc).args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"]),
                &source,
            )
        } else {
            compiler_input(
                Command::new("clang").args([
                    "-target",
                    target.triple(),
                    "-std=gnu11",
                    "-S",
                    "-emit-llvm",
                    "-o",
                    "/dev/null",
                    "-x",
                    "c",
                    "-",
                ]),
                &source,
            )
        };
        assert!(
            output.status.success(),
            "{target:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

const NATIVE_OPERATIONS: &str = r#"
struct Three { unsigned char values[3]; };
int load(_Atomic(int)*p){return *p;}
int store(_Atomic(int)*p,int x){return *p=x;}
int update(_Atomic(int)*p,int x){return *p+=x;}
int increment(_Atomic(int)*p){return (*p)++;}
void discard(_Atomic(int)*p){*p;}
int main(void) {
    _Atomic(int) a=1;
    if(load(&a)!=1 || store(&a,5)!=5 || update(&a,2)!=7 || increment(&a)!=7 || a!=8)return 1;
    a=2147483647; a++; if(a!=(-2147483647-1))return 2;
    _Atomic(float) f=1.5f; f+=2.0f; if(f!=3.5f)return 3;
    _Atomic(_Bool) b=1; b++; if(!b)return 4;
    int array[4]; _Atomic(int*) p=array; p++; if(p!=array+1)return 5;
    _Atomic(struct Three) record=(struct Three){{1,2,3}};
    struct Three value=record; if(value.values[2]!=3)return 6;
    record=(struct Three){{4,5,6}};value=record;if(value.values[0]!=4)return 7;
    volatile _Atomic(int) v=1;v+=2;if(v!=3)return 8;
    int n=2; _Atomic(int (*)[n++]) bound; (void)&bound; if(n!=3)return 9;
    return 0;
}
"#;

#[test]
#[ignore = "requires native GNU GCC, Clang, and libatomic"]
fn native_atomic_reads_stores_updates_and_bounds() {
    use std::process::Command;
    for target in Target::ALL {
        check(NATIVE_OPERATIONS, target).unwrap();
    }
    let folder = std::env::temp_dir().join(format!("toucan-atomic-types-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("probe.c"), NATIVE_OPERATIONS).unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for (compiler, gnu) in [(gcc, true), ("clang".into(), false)] {
        for optimization in ["-O0", "-O2"] {
            let mut command = Command::new(&compiler);
            command.current_dir(&folder).args([
                "-std=gnu11",
                optimization,
                "probe.c",
                "-o",
                "probe",
            ]);
            if gnu || cfg!(target_os = "linux") {
                // GCC's atomic float updates use __atomic_feraiseexcept on
                // AArch64 Darwin too; its implementation lives in libatomic.
                command.arg("-latomic");
            }
            if cfg!(target_os = "linux") {
                command.arg("-lm");
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                Command::new(folder.join("probe"))
                    .status()
                    .unwrap()
                    .success(),
                "{compiler} {optimization}"
            );
        }
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn atomic_casts_and_parameter_bounds_keep_written_and_effective_types() {
    use toucan_semantic::checked::{ExprKind, TypeStep};
    for target in Target::ALL {
        let analysis=check("void f(int n,int a[_Atomic n]) { _Atomic(int(*)[n++]) pointer; (void)(_Atomic(int(*)[n++]))0; }",target).unwrap();
        let code = analysis.checked().unwrap();
        let parameter = code
            .declarations()
            .find(|(_, d)| code.entity(d.entity()).unwrap().name() == Some("a"))
            .unwrap()
            .1;
        assert_eq!(
            analysis
                .unit()
                .atomic_value(code.ty(parameter.ty()).unwrap())
                .unwrap()
                .is_some(),
            gnu(target)
        );
        let pointer = code
            .declarations()
            .find(|(_, d)| code.entity(d.entity()).unwrap().name() == Some("pointer"))
            .unwrap()
            .1;
        let bound = code.type_use(pointer.type_use()).unwrap().extents();
        assert_eq!(bound.len(), 1);
        assert_eq!(bound[0].path(), [TypeStep::AtomicValue, TypeStep::Pointer]);
        let atomic_cast = code
            .expressions()
            .find(|(_, e)| {
                if let ExprKind::Cast { .. } = e.kind() {
                    e.type_name_use().is_some_and(|id| {
                        matches!(
                            code.ty(code.type_use(id).unwrap().shape()).unwrap().kind,
                            TypeKind::Atomic(_)
                        )
                    })
                } else {
                    false
                }
            })
            .unwrap()
            .1;
        let written = code.type_use(atomic_cast.type_name_use().unwrap()).unwrap();
        let effective = code.type_use(atomic_cast.type_use()).unwrap();
        assert_eq!(written.extents().len(), 1);
        assert_eq!(effective.extents().len(), 1);
        assert_eq!(written.extents()[0].bound(), effective.extents()[0].bound());
        assert_eq!(
            effective.extents()[0].path(),
            if gnu(target) {
                vec![TypeStep::Pointer]
            } else {
                vec![TypeStep::AtomicValue, TypeStep::Pointer]
            }
        );
    }
}

#[test]
fn atomic_constructor_and_public_layout_fail_with_bounded_diagnostics() {
    let mut source = "void f(void){typedef int T0;".to_owned();
    for n in 1..=2000 {
        source.push_str(&format!("typedef _Atomic(T{}*) T{n};", n - 1));
    }
    source.push('}');
    for target in Target::ALL {
        let error = check(&source, target).unwrap_err();
        assert!(
            error.message.contains("128-level") || error.message.contains("nesting limit"),
            "{error:?}"
        );
        let unit = analyze("", target).unwrap();
        let mut qualified =
            toucan_semantic::Type::new(TypeKind::Integer(toucan_semantic::IntegerKind::Int));
        qualified.qualifiers.is_const = true;
        for value in [
            qualified,
            toucan_semantic::Type::new(TypeKind::Void),
            toucan_semantic::Type::new(TypeKind::Array {
                element: Box::new(toucan_semantic::Type::new(TypeKind::Integer(
                    toucan_semantic::IntegerKind::Int,
                ))),
                length: Some(2),
            }),
        ] {
            assert!(
                unit.layout(&toucan_semantic::Type::new(TypeKind::Atomic(Box::new(
                    value
                ))))
                .is_err()
            );
        }
    }
}

#[test]
fn atomic_loads_do_not_hide_aligned_typedef_arithmetic_limits() {
    let prefix = "typedef int I __attribute__((aligned(16))); _Atomic(I) value;";
    for target in Target::ALL {
        for expression in ["value+0", "0+value", "+value"] {
            let result = check(
                &format!("{prefix}int f(void){{return {expression};}}"),
                target,
            );
            if toucan_target::CompilerProfile::default_for(target).compiler()
                == toucan_target::Compiler::Clang
            {
                result.unwrap();
            } else {
                let error = result.unwrap_err();
                assert!(
                    error.message.contains("arithmetic result alignment"),
                    "{error:?}"
                );
            }
        }
        check(&format!("{prefix}_Static_assert(_Alignof(__typeof__((0,value)))==16,\"atomic value alignment\");"),target).unwrap();
    }
}

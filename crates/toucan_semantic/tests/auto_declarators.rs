use toucan_semantic::checked::{BoundEvaluation, ExprKind, TypeStep};
use toucan_semantic::{Analysis, AnalysisOptions, Error, TypeKind, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile};

const VALID: &[&str] = &[
    "enum __attribute__((packed)) E{A=0,B=255};enum E e;__auto_type *p=&e,*q=&e;_Static_assert(__builtin_types_compatible_p(typeof(*p),enum E),\"enum\");",
    "__bf16 a;__auto_type *p=&a;_Static_assert(__builtin_types_compatible_p(typeof(*p),__bf16),\"bfloat\");",
    "__attribute__((nodebug)) __auto_type a=1,*p=&a;",
    "int n;__auto_type *p=&n;",
    "typedef int A[3];const A a={0};const int b[3]={0};__auto_type *p=&a,*q=&b;",
    "typedef int A[3];int f(const A*);int g(const int(*)[3]);void h(void){__auto_type p=f,q=g;}",
    "typedef int A[2][3];const A a={0};const int b[2][3]={0};__auto_type *p=&a,*q=&b;",
    "const int n=1;__auto_type *p=&n;",
    "int n;const __auto_type *p=&n;",
    "int n;int *a=&n;__auto_type **p=&a;",
    "int n;int *const a=&n;__auto_type *const *p=&a;",
    "void *a;__auto_type **p=&a;",
    "int a[3];__auto_type (*p)[3]=&a;",
    "int a[3];const __auto_type (*p)[3]=&a;",
    "int a[2][3];__auto_type (*p)[2][3]=&a;",
    "extern int a[];__auto_type (*p)[]=&a;",
    "int a[3];__auto_type (*p)[1+2]=&a;",
    "typedef int A[3];const A a={0};__auto_type (*p)[3]=&a;",
    "int f(int);__auto_type (*p)(int)=f;",
    "int f(int);__auto_type (*p)(const int)=f;",
    "int f(int,...);__auto_type (*p)(int,...)=f;",
    "int *f(void);__auto_type *(*p)(void)=f;",
    "void f(void);__auto_type (*p)(void)=f;",
    "const int f(void);const __auto_type (*p)(void)=f;",
    "int f();__auto_type *p=f;",
    "__auto_type x=1,y=2;",
    "int n;__auto_type x=1,*p=&n;",
    "const int n=1;const __auto_type x=1,*p=&n;",
    "int f(void){__auto_type x=1,y=x+1;return y;}",
    "void f(int n){int a[n];__auto_type *p=&a,*q=&a;}",
    "void f(int n){typedef int A[n];A a,b;__auto_type *p=&a,*q=&b;}",
    "void f(int n,int c){int a[n],b[n];__auto_type p=c?&a:&b,q=&a;}",
    "typedef void F(int a[][*]);F f,g;void h(void){__auto_type p=f,q=g;}",
    "extern int x;__auto_type x=1.5,y=2.5;",
    "extern int x;__auto_type x=1.5,y=2;",
    "__attribute__((aligned(sizeof(struct S{int member;})))) __auto_type x=1,y=2;",
];
const INVALID: &[&str] = &[
    "_Float16 h;__bf16 b;void f(void){__auto_type *p=&h,*q=&b;}",
    "__attribute__((nodebug(1))) __auto_type a=1,*p=&a;",
    "__auto_type *p=0;",
    "__auto_type *p=1.0;",
    "int a[3];__auto_type (*p)[4]=&a;",
    "int a[3];__auto_type (*p)[]=&a;",
    "void f(int n){int a[n];__auto_type (*p)[n]=&a;}",
    "int a[3];__auto_type b[3]=a;",
    "int f(int);__auto_type (*p)(double)=f;",
    "int f(int,...);__auto_type (*p)(int)=f;",
    "int f();__auto_type (*p)()=f;",
    "enum E{e};int f(enum E);__auto_type (*p)(int)=f;",
    "int f(int n,int a[][n]);__auto_type (*p)(int n,int a[][n])=f;",
    "int f(void);const __auto_type *p=f;",
    "int f(void);const __auto_type (*p)(void)=f;",
    "int n;int *a=&n;const __auto_type **p=&a;",
    "int n;int *const a=&n;__auto_type **p=&a;",
    "__auto_type x=1,y=2.0;",
    "const int n=1;__auto_type x=1,*p=&n;",
    "enum E{e};enum E n;void f(void){__auto_type x=n,y=1;}",
    "void f(int n){int a[n],b[n];__auto_type *p=&a,*q=&b;}",
    "void f(int c,int n){int a[n],b[n];__auto_type p=c?&a:&b,q=&b;}",
    "void f(int a[][*]);void g(int a[][*]);void h(void){__auto_type p=f,q=g;}",
    "int f(void){__auto_type x=1,y=y;return x;}",
];

fn parity(source: &str, profile: CompilerProfile) -> Result<Analysis, Error> {
    let plain = analyze_with_profile(source, profile, &AnalysisOptions::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&plain, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => assert_eq!((a.offset, &a.message), (b.offset, &b.message)),
        _ => panic!("{profile:?}: {source}: {plain:?}; {retained:?}"),
    }
    retained
}

#[test]
fn clang_patterns_and_groups_preserve_gnu_constraints() {
    for profile in CompilerProfile::ALL {
        for source in VALID {
            let result = parity(source, profile);
            assert_eq!(
                result.is_ok(),
                profile.compiler() == Compiler::Clang,
                "{profile:?}: {source}: {result:?}"
            );
        }
        for source in INVALID {
            assert!(parity(source, profile).is_err(), "{profile:?}: {source}");
        }
        if profile.compiler() == Compiler::Clang {
            let error = parity("int n;__auto_type *_Atomic p=&n;", profile).unwrap_err();
            assert!(error.message.contains("does not produce a concrete type"));
        }
    }
}

#[test]
fn inferred_bases_keep_bound_identity_and_written_callback_parameters() {
    let source = "int next(int x){return x+1;}int f(int n){typedef int A[n];A a;__auto_type *p=&a,*q=&a;__auto_type (*callback)(int argument)=next,scalar=1;return callback(scalar)+sizeof *p+sizeof *q;}";
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let analysis = parity(source, profile).unwrap();
        let code = analysis.checked().unwrap();
        let site = |name: &str| {
            code.declarations()
                .find(|(_, s)| code.entity(s.entity()).unwrap().name() == Some(name))
                .unwrap()
                .1
        };
        let a = code.type_use(site("a").type_use()).unwrap();
        let bound = a.extents()[0].bound();
        assert_eq!(
            code.bound(bound).unwrap().evaluation(),
            BoundEvaluation::Required
        );
        for name in ["p", "q"] {
            let declaration = site(name);
            let inference = declaration.type_inference().unwrap();
            let inferred = code.type_use(inference.type_use()).unwrap();
            assert!(matches!(
                code.ty(inferred.shape()).unwrap().kind,
                TypeKind::VariableArray { .. }
            ));
            assert_eq!(inferred.extents()[0].bound(), bound);
            assert!(inferred.extents()[0].path().is_empty());
            let final_type = code.type_use(declaration.type_use()).unwrap();
            assert_eq!(final_type.extents()[0].bound(), bound);
            assert_eq!(final_type.extents()[0].path(), [TypeStep::Pointer]);
            assert_eq!(&source[inference.keyword().range()], "__auto_type");
        }
        let callback = site("callback");
        let callback_type = code.type_use(callback.type_use()).unwrap();
        assert_eq!(callback_type.functions()[0].path(), [TypeStep::Pointer]);
        let parameter = code
            .declaration(callback_type.functions()[0].parameters()[0])
            .unwrap();
        assert_eq!(
            code.entity(parameter.entity()).unwrap().name(),
            Some("argument")
        );
        let base = code
            .type_use(callback.type_inference().unwrap().type_use())
            .unwrap();
        assert!(matches!(
            code.ty(base.shape()).unwrap().kind,
            TypeKind::Integer(_)
        ));
        assert!(base.functions().is_empty());
        assert_eq!(
            code.bounds()
                .filter(|(_, b)| b.evaluation() == BoundEvaluation::Required)
                .count(),
            1
        );
    }
}

#[test]
fn each_initializer_is_owned_once_and_common_attributes_do_not_redeclare_tags() {
    let source = "__attribute__((aligned(sizeof(struct S{int member;})))) __auto_type a=1,b=2;int f(int n){__auto_type x=n++,y=n++;return x+y+n;}";
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let analysis = parity(source, profile).unwrap();
        let code = analysis.checked().unwrap();
        assert_eq!(
            analysis
                .unit()
                .records
                .iter()
                .filter(|r| r.name.as_deref() == Some("S"))
                .count(),
            1
        );
        let post_increments = code
            .expressions()
            .filter(|(_, e)| {
                matches!(
                    e.kind(),
                    ExprKind::Unary {
                        operator: toucan_semantic::checked::Unary::PostIncrement,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(post_increments, 2);
        for name in ["x", "y"] {
            let declaration = code
                .declarations()
                .find(|(_, s)| code.entity(s.entity()).unwrap().name() == Some(name))
                .unwrap()
                .1;
            assert!(declaration.initializer().is_some());
            assert!(declaration.type_inference().is_some());
        }
    }
}

#[test]
fn array_qualification_conversion_preserves_element_pointer_constraints() {
    for profile in CompilerProfile::ALL {
        for source in [
            "int a[3];const int (*p)[3]=&a;",
            "int a[3][4];volatile int (*p)[3][4]=&a;",
            "int *a[3];int *const (*p)[3]=&a;",
            "extern int a[];const int (*p)[]=&a;",
            "void f(const int a[][3]);void g(void){int a[2][3];f(a);}",
        ] {
            parity(source, profile).unwrap_or_else(|e| panic!("{profile:?}: {source}: {e}"));
        }
        for source in [
            "const int a[3]={0};int (*p)[3]=&a;",
            "int a[3];const int (*p)[4]=&a;",
            "int *a[3];const int *(*p)[3]=&a;",
            "const int a[3]={0};void *p=&a;",
        ] {
            assert!(parity(source, profile).is_err(), "{profile:?}: {source}");
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn pattern_constraints_and_runtime_calls_match_compilers() {
    use std::process::Command;
    let temporary = tempfile::tempdir().unwrap();
    let compilers = ["gcc", "clang"].map(|compiler| {
        let version = Command::new(compiler).arg("--version").output().unwrap();
        assert!(version.status.success());
        let clang = String::from_utf8_lossy(&version.stdout)
            .to_lowercase()
            .contains("clang");
        (compiler, clang)
    });
    for (index, (source, accepted)) in VALID
        .iter()
        .map(|s| (*s, true))
        .chain(INVALID.iter().map(|s| (*s, false)))
        .enumerate()
    {
        let path = temporary.path().join(format!("case-{index}.c"));
        std::fs::write(&path, source).unwrap();
        for (compiler, clang) in compilers {
            let output = Command::new(compiler)
                .args([
                    "-std=gnu11",
                    "-Werror=incompatible-pointer-types",
                    "-fsyntax-only",
                ])
                .arg(&path)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(accepted && clang),
                "{compiler}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    for (index, (source, accepted)) in [
        ("int a[3];const int (*p)[3]=&a;", true),
        ("int a[3][4];volatile int (*p)[3][4]=&a;", true),
        ("int *a[3];int *const (*p)[3]=&a;", true),
        ("extern int a[];const int (*p)[]=&a;", true),
        ("const int a[3]={0};int (*p)[3]=&a;", false),
        ("int a[3];const int (*p)[4]=&a;", false),
        ("int *a[3];const int *(*p)[3]=&a;", false),
        ("const int a[3]={0};void *p=&a;", false),
    ]
    .into_iter()
    .enumerate()
    {
        let path = temporary.path().join(format!("array-{index}.c"));
        std::fs::write(&path, source).unwrap();
        for (compiler, _) in compilers {
            let output = Command::new(compiler)
                .args(["-std=gnu11", "-Werror", "-fsyntax-only"])
                .arg(&path)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(accepted),
                "{compiler}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    let source = "int next(int n){return n+1;}int f(int n){__auto_type x=n++,y=n++;__auto_type (*callback)(int)=next;return callback(x)+y+n;}int main(void){return f(3)!=13;}";
    let path = temporary.path().join("runtime.c");
    std::fs::write(&path, source).unwrap();
    for optimization in ["-O0", "-O2"] {
        let binary = temporary.path().join(format!(
            "program{optimization}{}",
            std::env::consts::EXE_SUFFIX
        ));
        let output = Command::new("clang")
            .args(["-std=gnu11", optimization])
            .arg(&path)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(Command::new(binary).status().unwrap().success());
    }
}

#[test]
fn derived_declarator_work_is_bounded() {
    let source = format!("__auto_type {}p = 0;", "*".repeat(129));
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        let error = parity(&source, profile).unwrap_err();
        assert!(error.message.contains("128"), "{error}");
    }
}

use toucan_semantic::checked::{BoundEvaluation, ExprKind, TypeStep};
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

const VALID: &[&str] = &[
    "__auto_type x=1;",
    "int f(void){__auto_type x=1;return x;}",
    "__auto_type (x)=1;",
    "const __auto_type x=1;",
    "volatile __auto_type x=1;",
    "int n; restrict __auto_type x=&n;",
    "static __auto_type x=1;",
    "extern __auto_type x=1;",
    "int f(void){auto __auto_type x=1;return x;}",
    "int f(void){register __auto_type x=1;return x;}",
    "_Thread_local __auto_type x=1;",
    "__thread __auto_type x=1;",
    "_Alignas(16) __auto_type x=1;",
    "__auto_type x __attribute__((aligned(16)))=1;",
    "int x;__auto_type x=1;",
    "void f(void){for(__auto_type x=0;x<3;x++){};}",
    "__extension__ __auto_type x=1;",
    "void f(void){__auto_type x=(struct S {int x;}){1};struct S y=x;}",
    "int f(void){__auto_type x=({__auto_type y=1;y;});return x;}",
];
const INVALID: &[&str] = &[
    "__auto_type x;",
    "__auto_type x={1};",
    "__auto_type f(void){return 1;}",
    "void f(__auto_type x);",
    "struct S{__auto_type x;};",
    "int x=(__auto_type)1;",
    "int x=sizeof(__auto_type);",
    "unsigned __auto_type x=1;",
    "__auto_type __auto_type x=1;",
    "_Atomic(__auto_type) x=1;",
    "typedef __auto_type T=1;",
    "int f(void){__auto_type x=x;return x;}",
    "int f(void){__auto_type x=&x;return 0;}",
    "int f(void){int x;__auto_type x=1;return x;}",
    "__auto_type x=(void)0;",
    "struct S; extern struct S s; void f(void){__auto_type x=s;}",
    "struct S{unsigned x:3;};void f(struct S s){__auto_type x=s.x;}",
    "int f(void){__auto_type x=sizeof x;return x;}",
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
fn checked(source: &str, target: Target) -> toucan_semantic::Analysis {
    analyze_with_options(
        source,
        target,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    )
    .unwrap_or_else(|e| panic!("{target}: {source}: {e}"))
}
#[test]
fn plain_inference_preserves_target_types_and_declaration_constraints() {
    for target in Target::ALL {
        for source in VALID {
            analyze(source, target).unwrap_or_else(|e| panic!("{target}: {source}: {e}"));
        }
        for source in INVALID {
            assert!(analyze(source, target).is_err(), "{target}: {source}");
        }
        for source in [
            "int x=3;int f(void){__auto_type x=x;return x;}",
            "typedef int T;int f(void){__auto_type T=(T)1;return T;}",
            "int x;int f(void){__auto_type x=__builtin_choose_expr(1,1,x);return x;}",
            "int x;int f(void){__auto_type x=__builtin_constant_p(x);return x;}",
            "int x;int f(void){__auto_type x=_Generic(x,int:1);return x;}",
            "_Atomic __auto_type x=1;",
        ] {
            assert_eq!(
                analyze(source, target).is_ok(),
                gnu(target),
                "{target}: {source}"
            );
        }
        for source in ["__auto_type x=1,y=2;", "int n;__auto_type *p=&n;"] {
            assert_eq!(
                analyze(source, target).is_ok(),
                !gnu(target),
                "{target}: {source}"
            );
        }
        let source = format!(
            "int f(void){{const int a=1;__auto_type x=a;_Static_assert(__builtin_types_compatible_p(typeof(&x),int*),\"int\");_Atomic int b=1;__auto_type y=b;_Static_assert(__builtin_types_compatible_p(typeof(&y),{}),\"atomic\");return x;}}",
            if gnu(target) { "int*" } else { "_Atomic(int)*" }
        );
        checked(&source, target);
    }
}
#[test]
fn checking_an_initializer_does_not_rebind_its_names() {
    let source = "float x;int f(void){static __auto_type x=sizeof x;_Static_assert(sizeof x==sizeof(unsigned long),\"size_t\");return x;}";
    let a = checked(source, Target::X86_64UnknownLinuxGnu);
    let code = a.checked().unwrap();
    let inference = code
        .declarations()
        .find_map(|(_, d)| d.type_inference())
        .unwrap();
    let expression = code.expression(inference.expression()).unwrap();
    let ExprKind::SizeOfValue { operand, .. } = expression.kind() else {
        panic!()
    };
    let operand = code.expression(operand.expression()).unwrap();
    assert!(matches!(
        code.ty(operand.ty()).unwrap().kind,
        toucan_semantic::TypeKind::Float(_)
    ));
    let source = "typedef int T;int f(void){__auto_type T=sizeof(T);return T;}";
    checked(source, Target::X86_64UnknownLinuxGnu);
    let source = "typedef int T;int f(void){__auto_type T=({typedef double T;T x=1;x;});return T;}";
    for target in Target::ALL {
        checked(source, target);
    }
}
#[test]
fn inferred_vla_types_reuse_the_initializer_bound_identities() {
    for source in [
        "int f(int n){__auto_type p=(int(*)[n++])0;return sizeof *p;}",
        "int f(int n){int a[2][n++];__auto_type p=a;return sizeof *p;}",
        "int f(int n){typedef int A[n++];A a;__auto_type p=&a;return sizeof *p;}",
    ] {
        for target in Target::ALL {
            let a = checked(source, target);
            let code = a.checked().unwrap();
            assert_eq!(
                code.bounds()
                    .filter(|(_, b)| b.evaluation() == BoundEvaluation::Required)
                    .count(),
                1
            );
            let (_, site) = code
                .declarations()
                .find(|(_, d)| d.type_inference().is_some())
                .unwrap();
            let inference = site.type_inference().unwrap();
            let before = code.type_use(inference.type_use()).unwrap();
            let after = code.type_use(site.type_use()).unwrap();
            assert_eq!(before.extents().len(), 1);
            assert_eq!(after.extents().len(), 1);
            assert_eq!(before.extents()[0].bound(), after.extents()[0].bound());
            assert_eq!(after.extents()[0].path(), &[TypeStep::Pointer]);
        }
    }
}
#[test]
fn retention_does_not_change_inferred_declarations_or_errors() {
    for target in Target::ALL {
        for source in VALID
            .iter()
            .chain(INVALID)
            .chain([&"float x;int f(void){static __auto_type x=sizeof x;return x;}"])
        {
            match (
                analyze(source, target),
                analyze_with_options(
                    source,
                    target,
                    &AnalysisOptions {
                        retain_code: true,
                        ..Default::default()
                    },
                ),
            ) {
                (Ok(a), Ok(b)) => assert_eq!(format!("{a:?}"), format!("{:?}", b.unit())),
                (Err(a), Err(b)) => assert_eq!((a.offset, a.message), (b.offset, b.message)),
                (a, b) => panic!("{target}: {source}: {a:?} {b:?}"),
            }
        }
    }
}

#[test]
fn atomic_deduction_preserves_nested_array_bounds() {
    for source in [
        "int f(int n){_Atomic(int(*)[n++]) p=0;__auto_type q=p;return sizeof *q;}",
        "int f(int n){__auto_type q=(_Atomic(int(*)[n++]))0;return sizeof *q;}",
    ] {
        for target in Target::ALL {
            let a = checked(source, target);
            let code = a.checked().unwrap();
            let (_, site) = code
                .declarations()
                .find(|(_, d)| d.type_inference().is_some())
                .unwrap();
            let inferred = code
                .type_use(site.type_inference().unwrap().type_use())
                .unwrap();
            let declared = code.type_use(site.type_use()).unwrap();
            assert_eq!(inferred.extents().len(), 1);
            assert_eq!(declared.extents().len(), 1);
            assert_eq!(inferred.extents()[0].bound(), declared.extents()[0].bound());
            let expected = if gnu(target) {
                vec![TypeStep::Pointer]
            } else {
                vec![TypeStep::AtomicValue, TypeStep::Pointer]
            };
            assert_eq!(declared.extents()[0].path(), expected);
            assert_eq!(
                code.bounds()
                    .filter(|(_, b)| b.evaluation() == BoundEvaluation::Required)
                    .count(),
                1
            );
        }
    }
}

#[test]
fn profile_specific_qualifiers_and_prior_types_remain_explicit() {
    for target in Target::ALL {
        let source = "restrict __auto_type x=1;";
        if gnu(target) {
            assert!(analyze(source, target).is_err());
        } else {
            let a = checked(source, target);
            assert!(a.unit().declarations[0].ty.qualifiers.is_restrict);
        }
        let source = "extern int x;__auto_type x=1.5;";
        if gnu(target) {
            assert!(analyze(source, target).is_err());
        } else {
            let a = checked(source, target);
            let code = a.checked().unwrap();
            let (_, site) = code
                .declarations()
                .find(|(_, d)| d.type_inference().is_some())
                .unwrap();
            assert!(site.type_inference().unwrap().reuses_prior_type());
            assert!(matches!(
                code.ty(site.ty()).unwrap().kind,
                toucan_semantic::TypeKind::Integer(toucan_semantic::IntegerKind::Int)
            ));
        }
    }
    // If the ordinary initializer path rebound x before checking it again,
    // sizeof would change from char to int and this would divide by zero.
    let source = "char x;int f(void){static __auto_type x=1/(sizeof x==1);return x;}";
    analyze(source, Target::X86_64UnknownLinuxGnu).unwrap();
    checked(source, Target::X86_64UnknownLinuxGnu);
}

#[test]
fn inferred_keyword_spans_and_retention_quotas_survive_adapters() {
    let source = "struct S{int x;}; __extension__ __auto_type value=(struct S){};";
    let a = checked(source, Target::X86_64UnknownLinuxGnu);
    let code = a.checked().unwrap();
    let inference = code
        .declarations()
        .find_map(|(_, d)| d.type_inference())
        .unwrap();
    assert_eq!(&source[inference.keyword().range()], "__auto_type");
    for limits in [
        toucan_semantic::checked::Limits {
            nodes: 2,
            ..Default::default()
        },
        toucan_semantic::checked::Limits {
            edges: 2,
            ..Default::default()
        },
        toucan_semantic::checked::Limits {
            payload_bytes: 2,
            ..Default::default()
        },
    ] {
        let error = analyze_with_options(
            source,
            Target::X86_64UnknownLinuxGnu,
            &AnalysisOptions {
                retain_code: true,
                retain_declaration_origins: false,
                limits,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.message.contains("retention"), "{error}");
        assert!(source.is_char_boundary(error.offset));
    }
}

fn type_assertions(is_gnu: bool) -> String {
    format!(
        r#"
int function(int x){{return x;}}
void types(void){{
 const volatile int source=1;__auto_type integer=source;
 _Static_assert(__builtin_types_compatible_p(typeof(&integer),int*),"qualifiers");
 int a[3];__auto_type array=a;
 _Static_assert(__builtin_types_compatible_p(typeof(&array),int**),"array decay");
 __auto_type callback=function;
 _Static_assert(__builtin_types_compatible_p(typeof(&callback),int(**)(int)),"function decay");
 __auto_type string="a";
 _Static_assert(__builtin_types_compatible_p(typeof(&string),char**),"string decay");
 _Atomic int atom=1;__auto_type inferred=atom;
 _Static_assert(__builtin_types_compatible_p(typeof(&inferred),{}),"atomic profile");
}}
"#,
        if is_gnu { "int*" } else { "_Atomic(int)*" }
    )
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn constraints_types_and_initializer_effects_match_native_compilers() {
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    for compiler in ["gcc", "clang"] {
        let version = Command::new(compiler).arg("--version").output().unwrap();
        assert!(version.status.success());
        let is_gnu = !String::from_utf8_lossy(&version.stdout)
            .to_ascii_lowercase()
            .contains("clang");
        for (index, (source, accepted)) in VALID
            .iter()
            .map(|s| (*s, true))
            .chain(INVALID.iter().map(|s| (*s, false)))
            .chain([
                ("int x=3;int f(void){__auto_type x=x+1;return x;}", is_gnu),
                (
                    "typedef int T;int f(void){__auto_type T=(T)1;return T;}",
                    is_gnu,
                ),
                ("extern int x;__auto_type x=1.5;", !is_gnu),
                ("restrict __auto_type x=1;", !is_gnu),
            ])
            .enumerate()
        {
            let path = temp.path().join(format!("constraint-{index}.c"));
            std::fs::write(&path, source).unwrap();
            let output = Command::new(compiler)
                .args(["-std=gnu11", "-fsyntax-only"])
                .arg(path)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(accepted),
                "{compiler}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let path = temp.path().join("runtime.c");
        let runtime = format!(
            r#"{}
int main(void){{
 int n=3;__auto_type pointer=(int(*)[n++])0;
 if(n!=4||sizeof *pointer!=3*sizeof(int))return 1;
 int a[2][n++];__auto_type decay=a;
 if(n!=5||sizeof *decay!=4*sizeof(int))return 2;
 typedef int A[n++];A b;__auto_type address=&b;
 if(n!=6||sizeof *address!=5*sizeof(int))return 3;
 __auto_type value=({{__auto_type nested=n++;nested;}});
 if(n!=7||value!=6)return 4;
 int selected=0,left=3,right=4;
 __auto_type choice=selected?sizeof(int[left++]):sizeof(int[right++]);
 if(left!=3||right!=5||choice!=4*sizeof(int))return 5;
 _Atomic(int(*)[n++]) atomic_pointer=0;__auto_type copy=atomic_pointer;
 if(n!=8||sizeof *copy!=7*sizeof(int))return 6;
 __auto_type atomic_cast=(_Atomic(int(*)[n++]))0;
 if(n!=9||sizeof *atomic_cast!=8*sizeof(int))return 7;
 return 0;
}}
"#,
            type_assertions(is_gnu)
        );
        std::fs::write(&path, runtime).unwrap();
        for optimization in ["-O0", "-O2"] {
            let exe = temp.path().join("runtime");
            let output = Command::new(compiler)
                .args(["-std=gnu11", optimization])
                .arg(&path)
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let status = Command::new(exe).status().unwrap();
            assert!(status.success(), "{compiler}: {optimization}: {status}");
        }
    }
}

#[test]
#[ignore = "requires Clang with all five target backends"]
fn target_inferred_types_match_clang() {
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("types.c");
    std::fs::write(&path, type_assertions(false)).unwrap();
    for target in Target::ALL {
        let output = Command::new("clang")
            .args(["-target", target.triple(), "-std=gnu11", "-fsyntax-only"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        checked(&type_assertions(gnu(target)), target);
    }
}

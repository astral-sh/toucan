use toucan_semantic::{
    AnalysisOptions, Type, TypeKind, VariableArrayId, analyze_with_profile, evaluate_integer,
};
use toucan_target::CompilerProfile;

fn identity(ty: &Type) -> VariableArrayId {
    match &ty.kind {
        TypeKind::VariableArray { identity, .. } => *identity,
        TypeKind::Pointer(element)
        | TypeKind::Atomic(element)
        | TypeKind::Array { element, .. } => identity(element),
        _ => panic!("not a variably modified type: {ty:?}"),
    }
}
#[test]
fn independent_occurrences_differ_and_reused_types_share_identity() {
    let source = "struct Marker{int member;};struct Marker marker=(struct Marker){};void f(int n,int c){int a[n],b[n];typedef int A[n];A x,y;typeof(a) reused;__auto_type pointer=&a;__auto_type composite=c?&a:&b;_Static_assert(__builtin_types_compatible_p(typeof(a),typeof(b)),\"compatible\");}";
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile(
            source,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let code = analysis.checked().unwrap();
        let get = |name: &str| {
            let (_, site) = code
                .declarations()
                .find(|(_, s)| code.entity(s.entity()).unwrap().name() == Some(name))
                .unwrap();
            assert_eq!(&source[site.name_source().unwrap().range()], name);
            identity(code.ty(site.ty()).unwrap())
        };
        assert_ne!(get("a"), get("b"));
        assert_ne!(get("a"), get("x"));
        assert_eq!(get("x"), get("y"));
        assert_eq!(get("a"), get("reused"));
        assert_eq!(get("a"), get("pointer"));
        assert_eq!(get("a"), get("composite"));
    }
}
#[test]
fn prototype_and_query_replay_identity_is_retention_independent() {
    let source = "void f(int n,int a[][n]){int local[n];(void)sizeof(typeof(local));(void)sizeof(typeof(local));}void g(int n,int a[][n]);void s(int a[][*]);void t(int a[][*]);typedef void F(int a[][*]);F x,y;";
    for profile in CompilerProfile::ALL {
        let plain = analyze_with_profile(source, profile, &AnalysisOptions::default())
            .unwrap()
            .into_unit();
        let retained = analyze_with_profile(
            source,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(format!("{plain:?}"), format!("{:?}", retained.unit()));
        let param = |name: &str, index: usize| {
            let d = plain.declarations.iter().find(|d| d.name == name).unwrap();
            let TypeKind::Function(f) = &plain.resolve(&d.ty).unwrap().kind else {
                panic!()
            };
            identity(&f.parameters[index].ty)
        };
        assert_ne!(param("f", 1), param("g", 1));
        assert_ne!(param("s", 0), param("t", 0));
        assert_eq!(param("x", 0), param("y", 0));
        let before = format!("{plain:?}");
        for _ in 0..2 {
            assert_eq!(
                evaluate_integer(&plain, "__builtin_types_compatible_p(typeof(s),typeof(t))")
                    .unwrap()
                    .value,
                1
            );
        }
        assert_eq!(before, format!("{plain:?}"));
    }
}

#[test]
#[ignore = "requires native Clang"]
fn exact_deduction_identity_observations_match_clang() {
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    for (index, (source, accepted)) in [
        ("void f(int n){int a[n],b[n];__auto_type p=&a,q=&b;}", false),
        ("void f(int n){int a[n];__auto_type p=&a,q=&a;}", true),
        (
            "void f(int n){typedef int A[n];A a,b;__auto_type p=&a,q=&b;}",
            true,
        ),
        (
            "void f(int a[][*]);void g(int a[][*]);void h(void){__auto_type p=f,q=g;}",
            false,
        ),
        (
            "typedef void F(int a[][*]);F f,g;void h(void){__auto_type p=f,q=g;}",
            true,
        ),
        (
            "void f(int c,int n){int a[n],b[n];__auto_type p=c?&a:&b,q=&a;}",
            true,
        ),
        (
            "void f(int c,int n){int a[n],b[n];__auto_type p=c?&a:&b,q=&b;}",
            false,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let path = temp.path().join(format!("case-{index}.c"));
        std::fs::write(&path, source).unwrap();
        let out = Command::new("clang")
            .args(["-std=gnu11", "-fsyntax-only"])
            .arg(path)
            .output()
            .unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&out),
            Ok(accepted),
            "{source}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

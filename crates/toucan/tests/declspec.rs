use std::process::Command;
use toucan::{AnalysisOptions, CompilerProfile, Target};

const CASES: &[(&str, bool)] = &[
    ("__declspec() int x;", true),
    ("_declspec(,noinline,,noreturn,) void f(void);", true),
    ("__declspec(align(16)align(32)) int x;", true),
    ("int __declspec(align(16)) x;", true),
    ("__declspec(align(0)) int x;", false),
    ("__declspec(align(3)) int x;", false),
    ("__declspec(align(16384)) int x;", false),
    ("__declspec(align()) int x;", false),
    ("__declspec(align(16,32)) int x;", false),
    ("__declspec(align(unknown)) int x;", false),
    ("__declspec(align(sizeof(int)*4)) int x;", true),
    ("enum {A=16}; __declspec(align(A)) int x;", true),
    ("__declspec(noreturn) void f(void);", true),
    ("__declspec(noreturn) int x;", true),
    ("__declspec(noreturn(1)) int x;", false),
    ("__declspec(noreturn(1)) struct S;", true),
    ("__declspec(noreturn(unknown)) struct S;", false),
    ("struct __declspec(noreturn(1)) S;", false),
    ("__declspec(noinline(1)) int x;", true),
    ("__declspec(noinline(unknown)) int x;", false),
    ("__declspec(noinline(1)) void f(void);", false),
    ("__declspec(noinline) void f(void);", true),
    ("__declspec(deprecated) void f(void);", true),
    ("__declspec(deprecated(\"old\")) void f(void);", true),
    ("__declspec(deprecated(1)) void f(void);", false),
    ("__declspec(deprecated(\"a\",\"b\")) void f(void);", false),
    ("__declspec(\"SAL\") int f(void);", true),
    ("__declspec(\"SAL\"(unknown)) int f(void);", true),
    ("__declspec(align((1,16))) int x;", false),
    ("const int A=16; __declspec(align(A)) int x;", false),
    ("__declspec(align((int)16.0)) int x;", true),
    ("__declspec(align(1?16:(1,2))) int x;", true),
    ("__declspec(deprecated((\"old\"))) void f(void);", true),
    ("void f(void) __declspec(noreturn);", false),
    ("typedef void (__declspec(noreturn) *N)(void);", false),
    ("typedef void (*__declspec(noreturn) N)(void);", false),
    ("__declspec(noreturn) typedef void (*N)(void);", true),
    ("__declspec(align(16)) void f(void);", true),
    ("void f(__declspec(align(16)) int x);", true),
    (
        "int f(void){return sizeof(__declspec(align(16)) int);}",
        false,
    ),
    (
        "int f(void){return sizeof(struct __declspec(align(16)) S{int x;});}",
        true,
    ),
    ("struct S{__declspec(align(16)) int x:3;};", true),
    (
        "__declspec(noreturn) typedef void (*N)(void); typedef void (*P)(void); P f(N p){return p;}",
        true,
    ),
    (
        "__declspec(noreturn) typedef void (*N)(void); typedef void (*P)(void); N f(P p){return p;}",
        false,
    ),
];

const LAYOUTS: &str = r#"
__declspec(align(16)) struct Prefix {int x;} prefix;
struct Postfix {int x;} __declspec(align(16)) postfix;
struct __declspec(align(16)) Interior {int x;};
__declspec(align(32)) struct Forward;
struct Forward {int x;};
struct Existing {int x;};
__declspec(align(16)) struct Existing object;
__declspec(align(16)) typedef struct Incomplete *Pointer;
__declspec(align(16)) typedef int Aligned;
__declspec(align(1)) typedef int Decreased;
struct Fields {char a; Aligned x; char b;};
struct DecreasedFields {char a; Decreased x; char b;};
#pragma pack(push,1)
struct Packed {char a; Aligned x; char b;};
#pragma pack(pop)
__declspec(align(1)) int reduced;
__declspec(align(16) noreturn) struct Return {int x;} *factory(void);
_Static_assert(sizeof(struct Prefix)==16 && __alignof(struct Prefix)==16,"prefix tag");
_Static_assert(sizeof(struct Postfix)==4 && __alignof(struct Postfix)==4,"postfix tag");
_Static_assert(sizeof(postfix)==4 && __alignof(postfix)==16,"postfix object");
_Static_assert(sizeof(struct Interior)==16 && __alignof(struct Interior)==16,"interior tag");
_Static_assert(sizeof(struct Forward)==32 && __alignof(struct Forward)==32,"forward tag");
_Static_assert(sizeof(struct Existing)==4 && __alignof(struct Existing)==4,"existing tag");
_Static_assert(sizeof(object)==4 && __alignof(object)==16,"existing object");
_Static_assert(sizeof(Pointer)==8 && __alignof(Pointer)==16,"pointer typedef");
_Static_assert(sizeof(Aligned)==4 && __alignof(Aligned)==16,"typedef");
_Static_assert(sizeof(struct Fields)==32 && __builtin_offsetof(struct Fields,x)==16 && __builtin_offsetof(struct Fields,b)==20,"fields");
_Static_assert(sizeof(struct DecreasedFields)==12 && __builtin_offsetof(struct DecreasedFields,x)==4,"decreased fields");
_Static_assert(sizeof(struct Packed)==32 && __builtin_offsetof(struct Packed,x)==16 && __builtin_offsetof(struct Packed,b)==20,"packed fields");
_Static_assert(sizeof(reduced)==4 && __alignof(reduced)==1,"decreased object");
_Static_assert(sizeof(struct Return)==16,"return tag");
"#;

fn compare_modes(source: &str, profile: CompilerProfile, accepted: bool) {
    let ordinary = toucan::semantic::analyze_with_profile(source, profile, &Default::default());
    let retained = toucan::semantic::analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    assert_eq!(
        ordinary.is_ok(),
        accepted,
        "{profile:?}: {source}: {ordinary:?}"
    );
    match (ordinary, retained) {
        (Ok(a), Ok(b)) => assert_eq!(
            serde_json::to_value(a.unit()).unwrap(),
            serde_json::to_value(b.unit()).unwrap()
        ),
        (Err(a), Err(b)) => assert_eq!((a.offset, a.message), (b.offset, b.message)),
        result => panic!("retained mode changed outcome: {result:?}"),
    }
}

#[test]
fn declspec_preserves_microsoft_spelling_placement_and_constraints() {
    for profile in CompilerProfile::ALL {
        for &(source, accepted) in CASES {
            compare_modes(
                source,
                profile,
                accepted && profile.target() == Target::X86_64PcWindowsMsvc,
            );
        }
        if profile.target() != Target::X86_64PcWindowsMsvc {
            compare_modes(
                "int __declspec; typedef int _declspec; _declspec f(_declspec x){return x+__declspec;}",
                profile,
                true,
            );
        }
    }
}

#[test]
fn declspec_alignment_keeps_tag_typedef_field_and_object_effects_separate() {
    let profile = CompilerProfile::default_for(Target::X86_64PcWindowsMsvc);
    compare_modes(LAYOUTS, profile, true);
    let analysis =
        toucan::semantic::analyze_with_profile(LAYOUTS, profile, &Default::default()).unwrap();
    let unit = analysis.unit();
    let get = |name: &str| unit.declarations.iter().find(|d| d.name == name).unwrap();
    assert!(get("prefix").alignment.is_empty());
    assert_eq!(get("postfix").alignment.msvc().unwrap().get(), 16);
    assert_eq!(
        serde_json::to_value(get("postfix").alignment).unwrap(),
        serde_json::json!({"msvc":16,"effective":16})
    );
    let toucan::semantic::TypeKind::Function(function) = &get("factory").ty.kind else {
        panic!()
    };
    assert!(function.noreturn);
    assert!(get("factory").alignment.is_empty());
}

#[test]
#[ignore = "requires Clang with Windows target support"]
fn native_clang_declspec_oracle() {
    let profile = CompilerProfile::default_for(Target::X86_64PcWindowsMsvc);
    for (source, accepted) in CASES.iter().copied().chain([(LAYOUTS, true), (API, true)]) {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), source).unwrap();
        let command = Command::new("clang")
            .args([
                "--target=x86_64-pc-windows-msvc",
                "-std=gnu11",
                "-fsyntax-only",
                "-Werror=incompatible-function-pointer-types",
                "-x",
                "c",
            ])
            .arg(file.path())
            .output()
            .unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&command).unwrap(),
            accepted,
            "{source}: {}",
            String::from_utf8_lossy(&command.stderr)
        );
        compare_modes(source, profile, accepted);
    }
}

const API: &str = r#"
struct __declspec(align(16)) Pair { unsigned long long a, b; };
__declspec(noreturn) typedef void (*Stop)(int);
__declspec(noreturn) void (*make_callback(void))(void);
__declspec(noinline) struct Pair exchange(struct Pair value);
__declspec(align(32)) extern int required;
"#;

#[test]
fn declspec_bindings_match_equivalent_checked_gnu_attributes() {
    let canonical = API
        .replace("__declspec(align(16))", "__attribute__((aligned(16)))")
        .replace("__declspec(noreturn)", "__attribute__((noreturn))")
        .replace("__declspec(noinline)", "__attribute__((noinline))")
        .replace("__declspec(align(32))", "__attribute__((aligned(32)))");
    let config = toucan::Config::new(Target::X86_64PcWindowsMsvc);
    let generate = |source: &str| {
        toucan::parse_source(std::path::Path::new("api.h"), source, &config)
            .unwrap()
            .bindings(&toucan::BindingOptions {
                rust_target: toucan::RustTarget::RUST_1_64,
                ..Default::default()
            })
            .unwrap()
            .0
    };
    assert_eq!(generate(API), generate(&canonical));
    let parsed = toucan::parse_source(std::path::Path::new("api.h"), API, &config).unwrap();
    let unit = parsed.unit();
    let factory = unit
        .declarations
        .iter()
        .find(|d| d.name == "make_callback")
        .unwrap();
    let toucan::semantic::TypeKind::Function(outer) = &factory.ty.kind else {
        panic!()
    };
    assert!(outer.noreturn);
    let toucan::semantic::TypeKind::Pointer(pointer) = &outer.return_type.kind else {
        panic!()
    };
    let toucan::semantic::TypeKind::Function(inner) = &pointer.kind else {
        panic!()
    };
    assert!(!inner.noreturn);
}

#[test]
fn declspec_queries_and_unsupported_attributes_keep_their_boundaries() {
    for profile in CompilerProfile::ALL {
        for name in [
            "align",
            "noreturn",
            "noinline",
            "__align__",
            "__noreturn__",
            "__noinline__",
        ] {
            assert_eq!(
                toucan::semantic::has_declspec_attribute(profile, name),
                u64::from(profile.target() == Target::X86_64PcWindowsMsvc)
            );
        }
        for name in [
            "_align_",
            "__align",
            "align__",
            "deprecated",
            "dllimport",
            "thread",
            "unknown",
        ] {
            assert_eq!(toucan::semantic::has_declspec_attribute(profile, name), 0);
        }
    }
    for source in [
        "__declspec(__align__(16)) int x;",
        "enum __declspec(align(16)) E{A};",
        "__declspec(align(16)) typedef void F(void);",
        "__declspec(dllimport) int x;",
        "__declspec(thread) int x;",
        "__declspec(selectany) int x;",
    ] {
        let error = toucan::semantic::analyze(source, Target::X86_64PcWindowsMsvc).unwrap_err();
        assert!(
            error.message.contains("unsupported")
                || error
                    .message
                    .contains("aligned typedefs require an object type"),
            "{source}: {error}"
        );
    }
}

use toucan_bindings::{ExternalTypeKind, Options, RustTarget};
use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{CompilerProfile, Target};

fn generate(
    source: &str,
    blocked: &[&str],
    profile: CompilerProfile,
    old_rust: bool,
) -> Result<toucan_bindings::Bindings, toucan_bindings::Error> {
    let analysis = analyze_with_profile(source, profile, &AnalysisOptions::default()).unwrap();
    toucan_bindings::generate(
        analysis.unit(),
        &Options {
            blocklist_types: blocked.iter().map(|name| (*name).into()).collect(),
            rust_target: if old_rust {
                RustTarget::RUST_1_64
            } else {
                RustTarget::default()
            },
            ..Default::default()
        },
    )
}

#[test]
fn external_aliases_and_incomplete_tags_keep_uses_and_report_the_contract() {
    for profile in CompilerProfile::ALL {
        let output = generate(
            "typedef int T; typedef T U; void f(T, U); struct Opaque; void g(struct Opaque *);",
            &["T", "Opaque"],
            profile,
            false,
        )
        .unwrap();
        assert!(!output.source.contains("pub type T ="));
        assert!(output.source.contains("pub type U = T;"));
        assert!(!output.source.contains("pub struct Opaque"));
        assert!(output.source.contains("*mut Opaque"));
        let integer = output
            .blocked_types
            .iter()
            .find(|ty| ty.c_name == "T")
            .unwrap();
        assert_eq!(integer.rust_name, "T");
        assert_eq!(integer.kind, ExternalTypeKind::Typedef);
        assert!(integer.referenced);
        assert_eq!(
            (integer.size_bytes, integer.alignment_bytes),
            (Some(4), Some(4))
        );
        let incomplete = output
            .blocked_types
            .iter()
            .find(|ty| ty.c_name == "Opaque")
            .unwrap();
        assert!(incomplete.referenced);
        assert_eq!(
            (incomplete.size_bytes, incomplete.alignment_bytes),
            (None, None)
        );
        assert!(!output.source.contains("size_of::<Opaque>"));
    }
}

#[test]
fn incomplete_array_and_void_aliases_report_only_known_layout() {
    for profile in CompilerProfile::ALL {
        let output = generate(
            "typedef int A[]; typedef void V; void f(A *, V *);",
            &["A", "V"],
            profile,
            true,
        )
        .unwrap();
        let array = output
            .blocked_types
            .iter()
            .find(|ty| ty.c_name == "A")
            .unwrap();
        assert_eq!((array.size_bytes, array.alignment_bytes), (None, Some(4)));
        assert!(array.referenced && !array.layout_required);
        let void = output
            .blocked_types
            .iter()
            .find(|ty| ty.c_name == "V")
            .unwrap();
        assert_eq!((void.size_bytes, void.alignment_bytes), (None, None));
        assert!(void.referenced && !void.layout_required);
        assert!(!output.source.contains("size_of::<A>"));
        assert!(!output.source.contains("align_of::<A>"));
    }
}

#[test]
fn unused_blocked_anonymous_storage_does_not_require_a_rust_definition() {
    let output = generate(
        "typedef struct { long double x; } max_align_t;",
        &["max_align_t"],
        CompilerProfile::default_for(Target::X86_64UnknownLinuxGnu),
        true,
    )
    .unwrap();
    assert_eq!(output.blocked_types.len(), 1);
    assert!(!output.blocked_types[0].referenced);
    assert!(!output.source.contains("max_align_t"));
    assert!(!output.source.contains("__toucan_record_"));
}

#[test]
fn enum_ownership_distinguishes_named_tags_from_anonymous_typedefs() {
    for profile in CompilerProfile::ALL {
        for (source, blocked, constants) in [
            ("enum E { A=1,B=2 }; void f(enum E);", "E", false),
            (
                "enum E { A=1,B=2 }; typedef enum E T; void f(T);",
                "T",
                true,
            ),
            (
                "typedef enum { A=1,B=2 } T; typedef T U; void f(U);",
                "T",
                false,
            ),
        ] {
            let output = generate(source, &[blocked], profile, false).unwrap();
            assert_eq!(
                output.source.contains("pub const A:"),
                constants,
                "{}",
                output.source
            );
            assert_eq!(!output.enum_constants.is_empty(), constants);
        }
    }
}

#[test]
fn unrelated_same_spelled_tag_and_typedef_keep_distinct_external_names() {
    for profile in CompilerProfile::ALL {
        let output = generate(
            "typedef long T; struct T {int x;}; void f(T, struct T *);",
            &["T"],
            profile,
            false,
        )
        .unwrap();
        let alias = output
            .blocked_types
            .iter()
            .find(|ty| ty.kind == ExternalTypeKind::Typedef)
            .unwrap();
        let record = output
            .blocked_types
            .iter()
            .find(|ty| ty.kind == ExternalTypeKind::Struct)
            .unwrap();
        assert_eq!(alias.rust_name, "T");
        assert_ne!(alias.rust_name, record.rust_name);
        assert!(
            output
                .source
                .contains(&format!("*mut {}", record.rust_name))
        );
        assert!(alias.referenced && record.referenced);
    }
}

#[test]
fn external_substitution_cannot_hide_unsupported_call_abis() {
    for profile in CompilerProfile::ALL {
        for (source, blocked, expected) in [
            ("typedef unsigned __int128 W; W f(W);", "W", "128-bit C ABI"),
            (
                "enum E { HUGE=(unsigned __int128)1<<100 }; enum E f(enum E);",
                "E",
                "128-bit C ABI",
            ),
            (
                "typedef struct {unsigned __int128 x;} W; W f(W);",
                "W",
                "128-bit C ABI",
            ),
            (
                "typedef void (*Callback)(unsigned __int128); Callback *get(void);",
                "Callback",
                "128-bit C ABI",
            ),
            (
                "typedef void (*Inner)(unsigned __int128); typedef void (*Callback)(Inner); Callback get(void);",
                "Callback",
                "128-bit C ABI",
            ),
            ("typedef long double W; W f(W);", "W", "long double"),
            ("typedef double _Complex W; W f(W);", "W", "call ABI"),
            (
                "typedef struct {double _Complex value;} W; W f(W);",
                "W",
                "call ABI",
            ),
            (
                "typedef double _Complex (*Inner)(int); typedef void (*Callback)(Inner); Callback get(void);",
                "Callback",
                "call ABI",
            ),
            (
                "typedef struct { _Atomic int x; } W; void f(W);",
                "W",
                "atomic storage",
            ),
            (
                "typedef struct { unsigned x:3; } W; void f(W);",
                "W",
                "bitfields",
            ),
            (
                "typedef int (*Callback)(); Callback get(void);",
                "Callback",
                "without a prototype",
            ),
            (
                "typedef struct {int x __attribute__((aligned(16)));} W; void f(W);",
                "W",
                "field-level alignment",
            ),
        ] {
            if source.starts_with("enum E") && profile.compiler() == toucan_target::Compiler::Clang
            {
                assert!(
                    analyze_with_profile(source, profile, &AnalysisOptions::default()).is_err()
                );
                continue;
            }
            let error = generate(source, &[blocked], profile, true).unwrap_err();
            assert!(
                error.to_string().contains(expected),
                "{profile:?} {source}: {error}"
            );
        }
        if profile.compiler() == toucan_target::Compiler::Clang {
            let error = generate(
                "typedef _Atomic(signed char) A; A f(A);",
                &["A"],
                profile,
                false,
            )
            .unwrap_err();
            assert!(error.to_string().contains("narrow atomic scalar calls"));
        }
    }
}

#[test]
fn incomplete_types_are_available_only_behind_pointers() {
    let profile = CompilerProfile::default_for(Target::X86_64UnknownLinuxGnu);
    let error = generate(
        "struct Opaque; void f(struct Opaque);",
        &["Opaque"],
        profile,
        false,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("incomplete record passed by value")
    );
}

#[test]
#[ignore = "requires native C compilers and Rust; run with --include-ignored"]
fn caller_owned_noncopy_types_work_in_arrays_unions_and_bidirectional_calls() {
    use std::process::Command;
    let host = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let header = "struct T { int value; }; typedef struct T T; struct R { T items[2]; }; union U { T t; int scalar; }; struct Opaque {long double wide; unsigned __int128 big; int value;}; struct Opaque *c_handle(void); int c_read(struct Opaque *); T rust_bump(T); int rust_sum(struct R); T c_bump(T); int c_sum(struct R); union U c_union(int); int c_take(union U);";
    let implementation = "T c_bump(T v){return rust_bump(v);} int c_sum(struct R r){return rust_sum(r);} union U c_union(int v){union U u;u.scalar=v;return u;} int c_take(union U u){return u.t.value;} struct Opaque *c_handle(void){static struct Opaque object={1.5L,(unsigned __int128)1<<100,42};return &object;} int c_read(struct Opaque *p){return p->wide==1.5L && p->big==((unsigned __int128)1<<100) ? p->value : -1;}";
    let rust = r#"
#[repr(C)] pub struct T { pub value: ::core::cell::UnsafeCell<i32> }
#[repr(C)] pub struct Opaque { _private: [u8;0] }
fn make(value:i32)->T { T{value: ::core::cell::UnsafeCell::new(value)} }
#[no_mangle] pub unsafe extern "C" fn rust_bump(value:T)->T {make(value.value.into_inner()+7)}
#[no_mangle] pub unsafe extern "C" fn rust_sum(value:R)->i32 {let [a,b]=value.items;a.value.into_inner()+b.value.into_inner()}
fn main(){ for value in -300..300 { unsafe {
 assert_eq!(c_read(c_handle()),42);
 assert_eq!(c_bump(make(value)).value.into_inner(),value+7);
 assert_eq!(c_sum(R{items:[make(value),make(3)]}),value+3);
 let union=c_union(value); assert_eq!(::core::mem::ManuallyDrop::into_inner(union.t).value.into_inner(),value);
 assert_eq!(c_take(U{t: ::core::mem::ManuallyDrop::new(make(value))}),value);
} } }
"#;
    let directory = tempfile::tempdir().unwrap();
    let c = directory.path().join("native.c");
    std::fs::write(&c, format!("{header}\n{implementation}\n")).unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let rustc = std::env::var("TOUCAN_TEST_RUSTC").unwrap_or_else(|_| "rustc".into());
    for (compiler, kind) in [
        (gcc.as_str(), toucan_target::Compiler::Gnu),
        ("clang", toucan_target::Compiler::Clang),
    ] {
        let Ok(profile) = CompilerProfile::new(host, kind) else {
            continue;
        };
        let analysis = analyze_with_profile(header, profile, &AnalysisOptions::default()).unwrap();
        let bindings = toucan_bindings::generate(
            analysis.unit(),
            &Options {
                blocklist_types: vec!["T".into(), "Opaque".into()],
                blocklist_functions: vec!["rust_*".into()],
                rust_target: RustTarget::RUST_1_64,
                ..Default::default()
            },
        )
        .unwrap();
        let input = directory.path().join("consumer.rs");
        std::fs::write(&input, format!("{}\n{rust}", bindings.source)).unwrap();
        for c_opt in ["-O0", "-O2"] {
            let object = directory.path().join("native.o");
            let output = Command::new(compiler)
                .args(["-std=c11", c_opt, "-c"])
                .arg(&c)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(true),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let library = directory.path().join("libnative.a");
            if library.exists() {
                std::fs::remove_file(&library).unwrap();
            }
            assert!(
                Command::new("ar")
                    .arg("crs")
                    .arg(&library)
                    .arg(&object)
                    .status()
                    .unwrap()
                    .success()
            );
            for rust_opt in ["0", "3"] {
                let binary = directory.path().join("consumer");
                let output = Command::new(&rustc)
                    .args(["--edition=2021", "-C"])
                    .arg(format!("opt-level={rust_opt}"))
                    .arg(&input)
                    .arg("-L")
                    .arg(format!("native={}", directory.path().display()))
                    .args(["-l", "static=native", "-o"])
                    .arg(&binary)
                    .output()
                    .unwrap();
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&output),
                    Ok(true),
                    "{compiler} {c_opt} rust{rust_opt}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let output = Command::new(&binary).output().unwrap();
                assert!(
                    output.status.success(),
                    "{compiler} {c_opt} rust{rust_opt}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

#[test]
#[ignore = "requires Rust; run with --include-ignored"]
fn external_layout_mismatches_fail_rust_compilation() {
    use std::process::Command;
    let host = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        ("x86_64", "windows") => Target::X86_64PcWindowsMsvc,
        _ => return,
    };
    let bindings = generate(
        "typedef int T; void f(T);",
        &["T"],
        CompilerProfile::default_for(host),
        true,
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("bad.rs");
    std::fs::write(&input, format!("{}\npub type T = u64;\n", bindings.source)).unwrap();
    let rustc = std::env::var("TOUCAN_TEST_RUSTC").unwrap_or_else(|_| "rustc".into());
    let output = Command::new(rustc)
        .args(["--edition=2021", "--crate-type=lib"])
        .arg(&input)
        .arg("-o")
        .arg(directory.path().join("bad.rlib"))
        .output()
        .unwrap();
    assert_eq!(toucan_test_support::compiler_acceptance(&output), Ok(false));
    assert!(String::from_utf8_lossy(&output.stderr).contains("assertion failed"));
}

#[test]
fn opaque_pointer_uses_skip_layout_but_later_value_uses_upgrade_aliases() {
    for profile in CompilerProfile::ALL {
        for source in [
            "typedef unsigned __int128 Wide; void f(Wide *);",
            "typedef double _Complex Wide; void f(Wide *);",
            "typedef __typeof__(1.0Q) Wide; void f(Wide *);",
            "typedef unsigned __int128 Wide; typedef Wide Alias; void f(Alias *);",
            "typedef struct {unsigned __int128 value;} Wide; struct R {Wide *p;}; void f(struct R);",
        ] {
            let output = generate(source, &["Wide"], profile, true)
                .unwrap_or_else(|error| panic!("{profile:?} {source}: {error}"));
            let wide = output
                .blocked_types
                .iter()
                .find(|ty| ty.c_name == "Wide")
                .unwrap();
            assert!(wide.referenced);
            assert!(!wide.layout_required);
            assert!(!output.source.contains("size_of::<Wide>"));
        }
        for source in [
            "typedef unsigned __int128 Wide; typedef Wide Alias; void pointer(Alias *); void value(Alias);",
            "typedef struct {unsigned __int128 value;} Wide; struct R {Wide value;}; void f(struct R *);",
            "typedef unsigned __int128 Wide; extern Wide value;",
            "struct Inner {unsigned __int128 value;}; typedef struct {struct Inner *p; struct Inner value;} Wide; extern Wide object;",
        ] {
            let error = generate(source, &["Wide"], profile, true).unwrap_err();
            assert!(
                error.to_string().contains("128-bit C ABI"),
                "{source}: {error}"
            );
        }
    }
}

#[test]
fn external_complex_storage_and_binary128_calls_keep_their_proof_boundaries() {
    for profile in CompilerProfile::ALL {
        for source in [
            "typedef double _Complex C; extern C value;",
            "typedef struct {double _Complex value;} C; extern C value;",
        ] {
            let error = generate(source, &["C"], profile, false).unwrap_err();
            assert!(error.to_string().contains("C complex storage"), "{error}");
        }
        let error = generate(
            "typedef __typeof__(1.0Q) Wide; Wide f(Wide);",
            &["Wide"],
            profile,
            false,
        )
        .unwrap_err();
        let error = error.to_string();
        assert!(
            error.contains("long double") || error.contains("extended floating-point"),
            "{profile:?}: {error}"
        );
    }
}

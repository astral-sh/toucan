use toucan_bindings::{Options, RustTarget, generate};
use toucan_semantic::analyze;
use toucan_target::{Compiler, CompilerProfile, Target};

fn bindings(
    source: &str,
    target: Target,
    options: &Options,
) -> Result<String, toucan_bindings::Error> {
    generate(&analyze(source, target).unwrap(), options).map(|b| b.source)
}
#[test]
fn atomic_storage_and_scalar_call_boundaries_are_separate() {
    let source = "typedef _Atomic(int) A;typedef _Atomic(_Bool) B;typedef _Atomic(int*) P;typedef _Atomic(float) F;enum E{LOW=-2,HIGH=1};typedef _Atomic(enum E) AE;A f(A);B b(B);P p(P);F g(F);AE e(AE);typedef A(*Callback)(A);void use_callback(Callback);struct S{A a;B b;P p;F f;};";
    for target in Target::ALL {
        let narrow_calls = CompilerProfile::default_for(target).compiler() == Compiler::Gnu;
        for rustified_enums in [false, true] {
            let source = bindings(
                source,
                target,
                &Options {
                    rustified_enums,
                    blocklist_functions: if !narrow_calls {
                        vec!["b".into()]
                    } else {
                        vec![]
                    },
                    ..Options::default()
                },
            )
            .unwrap();
            assert!(source.contains("pub type A = ::core::sync::atomic::AtomicI32;"));
            assert!(source.contains("pub type B = ::core::sync::atomic::AtomicBool;"));
            assert!(
                source
                    .contains("pub type P = ::core::sync::atomic::AtomicPtr<::core::ffi::c_int>;")
            );
            assert!(source.contains("pub fn f(arg0: ::core::ffi::c_int) -> ::core::ffi::c_int;"));
            assert_eq!(
                source.contains(
                    "pub fn b(arg0: ::core::primitive::bool) -> ::core::primitive::bool;"
                ),
                narrow_calls,
            );
            assert!(
                source
                    .contains("pub fn g(arg0: ::core::primitive::f32) -> ::core::primitive::f32;")
            );
            assert!(
                source
                    .contains("pub fn e(arg0: ::core::primitive::i32) -> ::core::primitive::i32;")
            );
            assert!(!source.contains("#[derive(Clone, Copy)]\npub struct S"));
            assert!(source.contains(
                "unsafe extern \"C\" fn(arg0: ::core::ffi::c_int) -> ::core::ffi::c_int"
            ));
            assert!(source.contains("PhantomData<*mut ()>"));
        }
    }
}
#[test]
fn noncopy_containment_and_opaque_qualifiers_preserve_storage() {
    let source = "typedef _Atomic(int) A;struct S{A a;};struct Outer{struct S values[2];};union U{A a;long long b;};const A constant;volatile A device;typedef _Atomic(const int*) CP;typedef _Atomic(int(*)(A)) FP;struct Bad{long double x;};typedef _Atomic(struct Bad) Opaque;";
    for target in Target::ALL {
        let output = bindings(
            source,
            target,
            &Options {
                allowlist: vec![
                    "Outer".into(),
                    "U".into(),
                    "constant".into(),
                    "device".into(),
                    "CP".into(),
                    "FP".into(),
                    "Opaque".into(),
                ],
                ..Options::default()
            },
        )
        .unwrap();
        for name in ["S", "Outer", "U"] {
            assert!(!output.contains(&format!("#[derive(Clone, Copy)]\npub struct {name}")));
        }
        assert!(
            output.contains("pub a: ::core::mem::ManuallyDrop<::core::sync::atomic::AtomicI32>")
        );
        assert!(!output.contains("pub struct Bad"));
        assert!(output.contains("pub static constant: __toucan_atomic_"));
        assert!(output.contains("pub static mut device: __toucan_atomic_"));
        assert!(output.contains("pub type CP = __toucan_atomic_"));
        assert!(output.contains("pub type FP = __toucan_atomic_"));
    }
}
#[test]
fn aggregate_values_and_packed_atomic_objects_diagnose() {
    let cases = [
        "struct S{float x,y;};_Atomic(struct S) f(_Atomic(struct S));",
        "_Atomic(__int128) wide(_Atomic(__int128));",
        "struct S{_Atomic(int) a;};struct S f(struct S);",
        "struct S{_Atomic(int) a;};struct O{struct S a[2];};struct O f(void);",
        "union U{_Atomic(int) a;long long b;};void f(union U);",
        "struct S{_Atomic(int) a;};typedef void(*Callback)(struct S);",
        "typedef _Atomic(int) A __attribute__((aligned(1)));A f(A);",
        "typedef int I __attribute__((aligned(16)));_Atomic(I) f(_Atomic(I));",
        "struct __attribute__((packed)) S{_Atomic(int) a;};",
        "struct S{_Atomic(int) a;};struct __attribute__((packed)) O{struct S a;};",
    ];
    for target in Target::ALL {
        for source in cases {
            let error = bindings(source, target, &Options::default()).unwrap_err();
            assert!(
                error.0.contains("atomic") || error.0.contains("typedef alignment"),
                "{source}: {error}"
            );
        }
    }
}
#[test]
fn atomic_alignment_and_names_are_checked_before_output() {
    let error = bindings(
        "enum E{A=-1,B=~0ULL};typedef _Atomic(enum E) Wide;Wide f(Wide);",
        Target::X86_64UnknownLinuxGnu,
        &Options::default(),
    )
    .unwrap_err();
    assert!(error.0.contains("128-bit atomic scalar"), "{error}");
    for target in Target::ALL {
        let error = bindings(
            "typedef int I __attribute__((aligned(16)));_Atomic(I) f(_Atomic(I));",
            target,
            &Options {
                allowlist: vec!["f".into()],
                ..Options::default()
            },
        )
        .unwrap_err();
        assert!(error.0.contains("atomic"));
        let low = bindings(
            "typedef _Atomic(int) A __attribute__((aligned(1)));A value;",
            target,
            &Options::default(),
        );
        if target == Target::X86_64PcWindowsMsvc {
            // The Microsoft ABI keeps four-byte field alignment despite this
            // one-byte type alignment; one Rust type cannot express both.
            assert!(low.unwrap_err().0.contains("alignment"));
        } else {
            let output = low.unwrap();
            assert!(output.contains("#[repr(C, align(1))]"));
            assert!(output.contains("MaybeUninit<[::core::primitive::u8; 4]>"));
        }
        let error = bindings(
            "typedef _Atomic(int) A __attribute__((aligned(16)));A value;",
            target,
            &Options::default(),
        )
        .unwrap_err();
        assert!(error.0.contains("alignment"));
        let output = bindings(
            "typedef int __toucan_test_atomic_0;typedef _Atomic(float) A;",
            target,
            &Options {
                helper_namespace: Some("test".into()),
                ..Options::default()
            },
        )
        .unwrap();
        assert!(output.contains("pub struct __toucan_test_atomic_0_"));
        let output = bindings(
            "typedef _Atomic(__int128) Wide;void f(Wide*);",
            target,
            &Options {
                rust_target: RustTarget::RUST_1_64,
                ..Options::default()
            },
        )
        .unwrap();
        assert!(output.contains("MaybeUninit<[::core::primitive::u8; 16]>"));
        assert!(
            bindings(
                "typedef _Atomic(__int128) Wide;void f(Wide);",
                target,
                &Options {
                    rust_target: RustTarget::RUST_1_64,
                    ..Options::default()
                }
            )
            .unwrap_err()
            .0
            .contains("1.78")
        );
    }
}
#[test]
fn atomic_function_pointer_value_dependencies_are_collected() {
    let source = "struct S{int x;};typedef int I;typedef _Atomic(int(*)(struct S*,I)) Callback;Callback f(Callback);";
    for target in Target::ALL {
        let output = bindings(
            source,
            target,
            &Options {
                allowlist: vec!["f".into()],
                ..Options::default()
            },
        )
        .unwrap();
        assert!(output.contains("pub struct S"));
        assert!(output.contains("pub type I ="));
        assert!(output.contains("Option<unsafe extern \"C\" fn(arg0: *mut S, arg1: I)"));
    }
}
#[test]
fn caller_built_deep_atomic_keys_are_bounded_before_hashing() {
    use toucan_semantic::{DeclarationKind, IntegerKind, Type, TypeKind};
    let mut unit = analyze("typedef int A;", Target::X86_64UnknownLinuxGnu).unwrap();
    let mut value = Type::new(TypeKind::Integer(IntegerKind::Int));
    for _ in 0..500 {
        value = value.pointer();
    }
    unit.typedefs
        .insert("A".into(), Type::new(TypeKind::Atomic(Box::new(value))));
    assert_eq!(unit.declarations[0].kind, DeclarationKind::Typedef);
    let error = generate(&unit, &Options::default()).unwrap_err();
    assert!(error.0.contains("nesting"), "{error}");
}

#[test]
fn shared_atomic_record_subgraphs_are_checked_once() {
    let mut source = String::from("struct S0{_Atomic(int) value;};");
    for n in 1..=24 {
        source.push_str(&format!("struct S{n}{{struct S{} a,b;}};", n - 1));
    }
    source.push_str("void access(struct S24*);");
    let output = bindings(&source, Target::X86_64UnknownLinuxGnu, &Options::default()).unwrap();
    assert_eq!(output.matches("pub struct S").count(), 25);
    assert!(!output.contains("#[derive(Clone, Copy)]\npub struct S"));
    source.push_str("struct S24 copy(struct S24);");
    let error = bindings(&source, Target::X86_64UnknownLinuxGnu, &Options::default()).unwrap_err();
    assert!(error.0.contains("atomic storage cannot cross"), "{error}");
}

#[test]
fn clang_narrow_atomic_values_reject_calls_but_preserve_storage() {
    for profile in CompilerProfile::ALL {
        for value in [
            "_Bool",
            "char",
            "signed char",
            "unsigned char",
            "short",
            "unsigned short",
        ] {
            for declaration in [
                "void f(A);",
                "A f(void);",
                "typedef A (*Callback)(A);void f(Callback);",
                "struct S{A(*callback)(A);};",
                "typedef _Atomic(A(*)(A)) Callback;Callback f(void);",
            ] {
                let source = format!("typedef _Atomic({value}) A;{declaration}");
                let analysis =
                    toucan_semantic::analyze_with_profile(&source, profile, &Default::default())
                        .unwrap();
                let output = generate(analysis.unit(), &Options::default());
                if profile.compiler() == Compiler::Clang {
                    assert!(
                        output
                            .unwrap_err()
                            .0
                            .contains("narrow atomic scalar calls under Clang"),
                        "{source}"
                    );
                } else {
                    output.unwrap();
                }
            }
            let source = format!(
                "typedef _Atomic({value}) A;A global;struct S{{A array[3];}};void access(A*,struct S*);"
            );
            let analysis =
                toucan_semantic::analyze_with_profile(&source, profile, &Default::default())
                    .unwrap();
            let output = generate(analysis.unit(), &Options::default())
                .unwrap()
                .source;
            assert!(output.contains("pub fn access("));
            assert!(output.contains("::core::sync::atomic::Atomic"));
            assert!(!output.contains("#[derive(Clone, Copy)]\npub struct S"));
        }
    }
}

const API: &str = include_str!("fixtures/atomic/api.h");
fn fixture(profile: CompilerProfile) -> String {
    let analysis =
        toucan_semantic::analyze_with_profile(API, profile, &Default::default()).unwrap();
    generate(
        analysis.unit(),
        &Options {
            rust_target: RustTarget::RUST_1_64,
            rustified_enums: true,
            // Selection is explicit: the unchanged C fixture still checks these
            // valid declarations, but their Clang call ABI is rejected above.
            blocklist_functions: if profile.compiler() == Compiler::Clang {
                [
                    "c_i8",
                    "c_i16",
                    "c_b",
                    "c_many",
                    "c_callback_i8",
                    "c_callback_i16",
                    "c_callback_b",
                    "c_narrow_stress",
                ]
                .map(str::to_owned)
                .to_vec()
            } else {
                vec![]
            },
            ..Options::default()
        },
    )
    .unwrap()
    .source
}
fn rustc() -> String {
    std::env::var("TOUCAN_TEST_RUSTC").unwrap_or_else(|_| "rustc".into())
}
#[test]
#[ignore = "requires native GNU GCC, Clang, Rust and the platform atomic runtime"]
fn generated_storage_and_scalar_values_cross_the_c_rust_abi() {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return;
    }
    use std::process::Command;
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let directory = tempfile::tempdir().unwrap();
    for (name, source) in [
        ("api.h", API),
        (
            "implementation.c",
            include_str!("fixtures/atomic/implementation.c"),
        ),
        ("consumer.rs", include_str!("fixtures/atomic/consumer.rs")),
    ] {
        std::fs::write(directory.path().join(name), source).unwrap();
    }
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        identity.status.success() && !String::from_utf8_lossy(&identity.stdout).contains("clang")
    );
    for compiler in [gcc, "clang".into()] {
        let profile = if compiler == "clang" {
            CompilerProfile::new(target, Compiler::Clang).unwrap()
        } else {
            // GNU Darwin profiles are not provided; exercise the common wider
            // scalar/storage interface against both installed C compilers.
            CompilerProfile::default_for(target)
        };
        std::fs::write(directory.path().join("bindings.rs"), fixture(profile)).unwrap();
        for opt in ["0", "2"] {
            let out = Command::new(&compiler)
                .current_dir(directory.path())
                .args([
                    "-std=gnu11",
                    &format!("-O{opt}"),
                    "-c",
                    "implementation.c",
                    "-o",
                    "native.o",
                ])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            // A native archive participates in normal library ordering. A raw
            // link-arg object appears after libc and can lose its dependencies.
            let out = Command::new("ar")
                .current_dir(directory.path())
                .args(["crs", "libnative.a", "native.o"])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            for rust_opt in ["0", "3"] {
                let mut command = Command::new(rustc());
                command.current_dir(directory.path()).args([
                    "--edition=2021",
                    "-C",
                    &format!("opt-level={rust_opt}"),
                    "-L",
                    "native=.",
                    "-l",
                    "static=native",
                    "consumer.rs",
                    "-o",
                    "consumer",
                ]);
                if profile.compiler() == Compiler::Gnu {
                    command.args(["--cfg", "toucan_atomic_narrow_calls"]);
                }
                if cfg!(target_os = "linux") {
                    command.args(["-l", "atomic"]);
                }
                let out = command.output().unwrap();
                assert!(
                    out.status.success(),
                    "{compiler}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                let status = Command::new(directory.path().join("consumer"))
                    .status()
                    .unwrap();
                assert!(
                    status.success(),
                    "{compiler} -O{opt}, Rust opt-level={rust_opt}: {status}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires native Rust and Clang target backends; TOUCAN_TEST_ALL_RUST_TARGETS also checks installed cross libraries"]
fn generated_layouts_match_compiler_targets() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let directory = tempfile::tempdir().unwrap();
    let version = Command::new(rustc()).arg("-vV").output().unwrap();
    let identity = String::from_utf8(version.stdout).unwrap();
    let host = identity
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .unwrap();
    let all = std::env::var("TOUCAN_TEST_ALL_RUST_TARGETS").as_deref() == Ok("1");
    let mut rust_targets = 0;
    for target in Target::ALL {
        let output = fixture(CompilerProfile::default_for(target));
        std::fs::write(
            directory.path().join("bindings.rs"),
            format!("#![allow(non_camel_case_types)]\n{output}"),
        )
        .unwrap();
        if all || target.triple() == host {
            let out = Command::new(rustc())
                .current_dir(directory.path())
                .args([
                    "--edition=2021",
                    "--target",
                    target.triple(),
                    "--crate-type=lib",
                    "--emit=llvm-ir",
                    "bindings.rs",
                    "-o",
                    "bindings.ll",
                ])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{target}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            rust_targets += 1;
        }
        let mut source = API.to_owned();
        for (ty, size, align) in [
            ("AtomicInt", 4, 4),
            ("AtomicBool", 1, 1),
            ("AtomicPointer", 8, 8),
            ("AtomicPair", 8, 8),
            ("struct Fields", 16, 8),
            ("union U", 8, 8),
        ] {
            source.push_str(&format!("_Static_assert(sizeof({ty})=={size},\"size\");_Static_assert(_Alignof({ty})=={align},\"alignment\");"));
        }
        source.push_str("_Static_assert(__builtin_offsetof(struct Fields,p)==8,\"offset\");");
        let mut child = Command::new("clang")
            .args([
                "-target",
                target.triple(),
                "-std=gnu11",
                "-fsyntax-only",
                "-x",
                "c",
                "-",
            ])
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert_eq!(rust_targets, if all { 5 } else { 1 });
}

#[test]
#[ignore = "requires native Rust"]
fn generated_atomic_storage_rejects_copy_and_opaque_thread_traits() {
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let version = Command::new(rustc()).arg("-vV").output().unwrap();
    let identity = String::from_utf8(version.stdout).unwrap();
    let host = identity
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .unwrap();
    let target = Target::parse(host).unwrap();
    std::fs::write(
        directory.path().join("bindings.rs"),
        fixture(CompilerProfile::default_for(target)),
    )
    .unwrap();
    let prelude = "#![allow(non_camel_case_types,dead_code)]\nmod bindings{include!(\"bindings.rs\");}use bindings::*;";
    for (name, body, expected) in [
        (
            "base",
            "pub fn construct(){let _=AtomicPair::uninit();let _=AtomicInt::new(1);}",
            true,
        ),
        (
            "copy",
            "fn need<T:Copy>(){}pub fn bad(){need::<Fields>();}",
            false,
        ),
        (
            "nested_copy",
            "fn need<T:Copy>(){}pub fn bad(){need::<Outer>();}",
            false,
        ),
        (
            "union_copy",
            "fn need<T:Copy>(){}pub fn bad(){need::<U>();}",
            false,
        ),
        (
            "send",
            "fn need<T:Send>(){}pub fn bad(){need::<AtomicPair>();}",
            false,
        ),
        (
            "sync",
            "fn need<T:Sync>(){}pub fn bad(){need::<AtomicPair>();}",
            false,
        ),
    ] {
        let path = directory.path().join(format!("{name}.rs"));
        std::fs::write(&path, format!("{prelude}\n{body}")).unwrap();
        let out = Command::new(rustc())
            .current_dir(directory.path())
            .args(["--edition=2021", "--crate-type=lib", "--emit=metadata"])
            .arg(&path)
            .args(["-o", "probe.rmeta"])
            .output()
            .unwrap();
        assert_eq!(
            out.status.success(),
            expected,
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if !expected {
            assert!(String::from_utf8_lossy(&out.stderr).contains("E0277"));
        }
    }
}

use std::process::Command;

use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

#[test]
fn local_tags_and_enumerators_do_not_match_global_allowlists() {
    let unit = analyze(
        "void f(struct Hidden *); void g(enum Local { LOCAL = 1 } value);",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let bindings = generate(
        &unit,
        &Options {
            allowlist: vec!["Hidden".into(), "Local".into(), "LOCAL".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(bindings.declarations, 0);
    assert!(!bindings.source.contains("pub "));

    let bindings = generate(
        &unit,
        &Options {
            allowlist: vec!["f".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(bindings.declarations, 1);
    assert!(bindings.source.contains("pub struct __toucan_record_"));
    assert!(!bindings.source.contains("pub type "));
    assert!(!bindings.source.contains("pub const "));

    // An unselected prototype's unsupported fields cannot prevent selecting a
    // later file-scope tag with the same spelling.
    let unit = analyze(
        "void f(struct S { long double local; } *); struct S { int global; };",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let bindings = generate(
        &unit,
        &Options {
            allowlist: vec!["S".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(bindings.source.contains("pub struct S {"));
    assert!(!bindings.source.contains("__toucan_record_"));
}

#[test]
fn local_names_do_not_rename_global_identifiers() {
    let unit = analyze(
        "typedef int self; void f(struct __toucan_self *);",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let bindings = generate(
        &unit,
        &Options {
            allowlist: vec!["self".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(bindings.source.contains("pub type __toucan_self = "));
    assert!(!bindings.source.contains("pub type __toucan_self_ = "));
}

#[test]
#[ignore = "requires native rustc; run with --include-ignored"]
fn compiled_bindings_preserve_prototype_tag_identity() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => panic!("native compilation test is configured only for Linux and macOS"),
    };
    let unit = analyze(
        r#"
        void first(struct S *);
        void second(struct S *);
        struct S { int global; };
        void third(struct S *);
        void shadow(struct S { double local; } *);
        void enum_first(enum E { A = -1 } value);
        void enum_second(enum E { A = 4294967296ULL } value);
        enum E { A = 3 };
        void enum_third(enum E value);
        "#,
        target,
    )
    .unwrap();
    let source = generate(&unit, &Options::default()).unwrap().source;
    assert!(source.contains("pub struct S {"));
    assert!(source.contains("pub type E = ::core::primitive::u32;"));
    let local_records = unit
        .records
        .iter()
        .enumerate()
        .filter(|(_, record)| record.scope == toucan_semantic::Scope::Prototype)
        .map(|(id, _)| format!("__toucan_record_{id}"))
        .collect::<Vec<_>>();
    assert_eq!(local_records.len(), 3);
    let first = &local_records[0];
    let second = &local_records[1];
    let shadow = &local_records[2];
    let checks = format!(
        r#"
        fn check() {{
            let _: unsafe extern "C" fn(*mut {first}) = first;
            let _: unsafe extern "C" fn(*mut {second}) = second;
            let _: unsafe extern "C" fn(*mut S) = third;
            let _: unsafe extern "C" fn(*mut {shadow}) = shadow;
            let _: unsafe extern "C" fn(i32) = enum_first;
            let _: unsafe extern "C" fn(u64) = enum_second;
            let _: unsafe extern "C" fn(E) = enum_third;
            let _ = S {{ global: 1 }};
            let _ = {shadow} {{ local: 1.0 }};
        }}
    "#
    );
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("bindings.rs");
    let compile = |extra: &str| {
        std::fs::write(&input, format!("{source}\n{checks}\n{extra}")).unwrap();
        Command::new("rustc")
            .current_dir(directory.path())
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "-D",
                "improper_ctypes",
                "bindings.rs",
            ])
            .output()
            .unwrap()
    };
    let output = compile("");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for destination in [second.as_str(), "S"] {
        let output = compile(&format!(
            "fn wrong() {{ let _: unsafe extern \"C\" fn(*mut {destination}) = first; }}"
        ));
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E0308"));
    }
}

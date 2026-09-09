use toucan_bindgen::{Builder, Formatter};

#[test]
fn selected_and_blocked_callback_aliases_keep_their_dependency_names() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("callbacks.h");
    std::fs::write(
        &header,
        "typedef int Number; typedef Number Callback(Number); extern Callback *global;",
    )
    .unwrap();
    let bindings = Builder::default()
        .header(header.to_str().unwrap())
        .clang_arg("--target=x86_64-unknown-linux-gnu")
        .allowlist_var("global")
        .blocklist_type("Callback")
        .formatter(Formatter::None)
        .generate()
        .unwrap();
    let source = bindings.to_string();
    assert!(source.contains("pub type Number ="));
    assert!(source.contains("pub static mut global: Callback;"));
    assert!(!source.contains("pub type Callback ="));
    let external = &bindings.report().blocked_types[0];
    assert_eq!(external.c_name, "Callback");
    assert!(external.referenced);
    assert!(!external.layout_required);
}

#[test]
#[ignore = "requires rustc"]
fn callback_aliases_accept_null_values_across_rust_api_positions() {
    let version = std::process::Command::new("rustc")
        .args(["--version", "--verbose"])
        .output()
        .unwrap();
    assert!(version.status.success());
    let version = String::from_utf8(version.stdout).unwrap();
    let target = version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("callbacks.h");
    std::fs::write(
        &header,
        "typedef int Callback(int); typedef Callback Alias; typedef Callback *Pointer; typedef Callback *Factory(Callback *); struct Holder {Callback *one; Callback **two; Callback *array[2];}; void take(Callback,Alias *); Callback *get(void); void nested(Factory *); Callback declared;",
    )
    .unwrap();
    let source = Builder::default()
        .header(header.to_str().unwrap())
        .clang_arg(format!("--target={target}"))
        .formatter(Formatter::None)
        .layout_tests(false)
        .generate()
        .unwrap()
        .to_string();
    let rust = directory.path().join("callbacks.rs");
    std::fs::write(
        &rust,
        source
            + r#"
const EMPTY: Callback = None;
const ALIAS: Alias = EMPTY;
const POINTER: Pointer = ALIAS;
const FACTORY: Factory = None;
const VALUES: [Callback; 2] = [None, POINTER];
unsafe extern "C" fn local(value: ::core::ffi::c_int) -> ::core::ffi::c_int { value }
unsafe fn exercise(holder: &mut Holder) {
    holder.one = Some(local);
    holder.array = VALUES;
    holder.two = &mut holder.one;
    take(EMPTY, ALIAS);
    let _: Callback = get();
    nested(FACTORY);
    let _: Callback = Some(declared);
}
"#,
    )
    .unwrap();
    let compiled = std::process::Command::new("rustc")
        .args([
            "--edition=2021",
            "--crate-type=lib",
            "--emit=metadata",
            "-A",
            "warnings",
        ])
        .arg(&rust)
        .arg("-o")
        .arg(directory.path().join("callbacks.rmeta"))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
}

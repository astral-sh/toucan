use std::path::Path;
use toucan_bindgen::callbacks::{ItemInfo, ParseCallbacks};
use toucan_bindgen::{Builder, Formatter};

fn builder(path: &Path) -> Builder {
    Builder::default()
        .header(path.to_str().unwrap())
        .clang_arg("--target=x86_64-unknown-linux-gnu")
        .formatter(Formatter::None)
        .layout_tests(false)
}

#[test]
fn scalar_initializers_preserve_declared_types_and_first_occurrences() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("objects.h");
    std::fs::write(&path,"extern const int early; const int early=7; static const int value=3; int mutable=4; static signed char narrow=255; static const _Bool yes=7; typedef double D; const D d=-0.0; enum E{V=9}; static const enum E e=V; static const unsigned long long max=18446744073709551615ULL; static const unsigned long long expr=9223372036854775808ULL+1; _Thread_local const _Atomic(int) atomic=7;").unwrap();
    let bindings = builder(&path).generate().unwrap().to_string();
    for expected in [
        "pub static early: ::core::ffi::c_int;",
        "pub const value: ::core::ffi::c_int = 3;",
        "pub const mutable: ::core::ffi::c_int = 4;",
        "pub const narrow: ::core::ffi::c_schar = -1;",
        "pub const yes: ::core::primitive::bool = true;",
        "pub const d: D",
        "pub static e: E;",
        "pub const max: ::core::ffi::c_ulonglong = 18446744073709551615;",
        "pub static expr: ::core::ffi::c_ulonglong;",
        "pub const atomic: ::core::ffi::c_int = 7;",
    ] {
        assert!(
            bindings.contains(expected),
            "missing {expected}: {bindings}"
        );
    }
    assert!(!bindings.contains("pub const early:"));
}

#[derive(Debug)]
struct Rename;
impl ParseCallbacks for Rename {
    fn generated_name_override(&self, item: ItemInfo<'_>) -> Option<String> {
        Some(format!("renamed_{}", item.name))
    }
}

#[test]
fn file_selection_and_callbacks_use_the_first_selected_written_object() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("root.h");
    std::fs::write(
        dir.path().join("first.h"),
        "extern const int object; static const int internal=3;\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("second.h"),
        "#line 99 \"logical.h\"\nconst int object=7;\n",
    )
    .unwrap();
    std::fs::write(&path, "#include \"first.h\"\n#include \"second.h\"\n").unwrap();
    let all = builder(&path)
        .parse_callbacks(Box::new(Rename))
        .generate()
        .unwrap()
        .to_string();
    assert!(all.contains("pub static renamed_object:"));
    assert!(all.contains("pub const internal:"));
    assert!(!all.contains("renamed_internal"));
    let second = builder(&path)
        .allowlist_file(r".*[/\\]second\.h")
        .parse_callbacks(Box::new(Rename))
        .generate()
        .unwrap()
        .to_string();
    assert!(
        second.contains("pub const renamed_object: ::core::ffi::c_int = 7;"),
        "{second}"
    );
    assert!(!second.contains("pub const internal:"));
    assert!(
        !builder(&path)
            .allowlist_file("logical.h")
            .generate()
            .unwrap()
            .to_string()
            .contains("pub const object:")
    );
}

#[test]
fn unsupported_reference_values_are_diagnostics_and_core_defaults_stay_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("object.h");
    for source in [
        "static const __int128 object=((__int128)1)<<100;",
        "static const long double object=0.1L;",
    ] {
        std::fs::write(&path, source).unwrap();
        assert!(builder(&path).generate().is_err());
    }
    std::fs::write(&path, "static const int object=7; int external=3;").unwrap();
    let compilation = toucan::parse_file(
        &path,
        &toucan::Config::new(toucan::Target::X86_64UnknownLinuxGnu),
    )
    .unwrap();
    let (source, _) = compilation
        .bindings(&toucan::BindingOptions::default())
        .unwrap();
    assert!(!source.contains("pub const object:"));
    assert!(source.contains("pub static mut external:"));
}

#[test]
fn object_occurrences_cannot_cross_target_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("object.h");
    std::fs::write(&path, "static unsigned long object=4294967296ULL;").unwrap();
    let mut config = toucan::Config::new(toucan::Target::X86_64UnknownLinuxGnu);
    config.analysis.retain_object_values = true;
    let first = toucan::parse_file(&path, &config).unwrap();
    let mut options = toucan::BindingOptions::default();
    let object = first.object_values().unwrap().entries()[0].clone();
    options.object_bindings.insert(object.name().into(), object);
    let other = toucan::parse_file(
        &path,
        &toucan::Config::new(toucan::Target::X86_64PcWindowsMsvc),
    )
    .unwrap();
    assert!(
        other
            .bindings(&options)
            .unwrap_err()
            .to_string()
            .contains("different compiler profile")
    );
}

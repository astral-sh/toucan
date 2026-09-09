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
        "pub const max: ::core::ffi::c_ulonglong = 18446744073709551615;",
        "pub const atomic: ::core::ffi::c_int = 7;",
    ] {
        assert!(
            bindings.contains(expected),
            "missing {expected}: {bindings}"
        );
    }
    assert!(!bindings.contains("pub const early:"));
    assert!(!bindings.contains("pub static e:"));
    assert!(!bindings.contains("pub static expr:"));
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
        "extern __int128 object;",
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

#[test]
fn string_objects_project_terminated_prefixes_and_reject_unterminated_arrays() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("strings.h");
    std::fs::write(&path,"static const char embedded[]=\"a\\0bc\"; static char padded[8]=\"abc\"; static const unsigned char raw[]=\"\\xff\\x80\"; static const char *parenthesized=(\"abc\"); static const char braced[]={\"abc\"}; static const unsigned short wide[]=u\"abc\";").unwrap();
    let source = builder(&path).generate().unwrap().to_string();
    for expected in [
        "pub const embedded: &[::core::primitive::u8; 2] = &[97, 0, ];",
        "pub const padded: &[::core::primitive::u8; 4] = &[97, 98, 99, 0, ];",
        "pub const raw: &[::core::primitive::u8; 3] = &[255, 128, 0, ];",
    ] {
        assert!(source.contains(expected), "missing {expected}: {source}");
    }
    for name in ["parenthesized", "braced", "wide"] {
        assert!(!source.contains(&format!("pub static {name}:")));
        assert!(!source.contains(&format!("pub static mut {name}:")));
    }
    std::fs::write(&path, "static const char exact[3]=\"abc\";").unwrap();
    assert!(
        builder(&path)
            .generate()
            .unwrap_err()
            .to_string()
            .contains("no terminating NUL within its 3-byte C array")
    );
}

#[test]
fn selected_internal_objects_keep_constants_and_report_unavailable_symbols() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("internal.h");
    std::fs::write(
        &path,
        r#"
static int hidden;
static int scalar=7;
static const char bytes[]="ok";
static struct Aggregate {int x;} aggregate={7};
static int *pointer=&scalar;
static unsigned long long wide_literal=18446744073709551615ULL;
static unsigned long long wide_binary=9223372036854775808ULL+1;
static unsigned long long wide_cast=(unsigned long long)-1;
static unsigned long long wide_unary=-1ULL;
static int braced={7};
"#,
    )
    .unwrap();
    let bindings = builder(&path).allowlist_var(".*").generate().unwrap();
    let source = bindings.to_string();
    assert_eq!(
        bindings.report().skipped_declarations,
        ["hidden", "aggregate", "pointer", "wide_binary", "wide_cast"]
    );
    assert!(!source.contains("pub static"));
    assert!(source.contains("pub struct Aggregate"));
    for name in ["scalar", "bytes", "wide_literal", "wide_unary", "braced"] {
        assert!(source.contains(&format!("pub const {name}:")), "{source}");
    }
    assert_eq!(source.matches("18446744073709551615;").count(), 2);
}

#[test]
fn string_objects_keep_file_order_and_external_name_callbacks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("root.h");
    std::fs::write(
        dir.path().join("first.h"),
        "extern const char object[4]; static const char internal[]=\"own\";\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("second.h"),
        "#line 99 \"logical.h\"\nconst char object[4]=\"abc\";\n",
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
    let second = builder(&path)
        .allowlist_file(r".*[/\\]second\.h")
        .parse_callbacks(Box::new(Rename))
        .generate()
        .unwrap()
        .to_string();
    assert!(
        second.contains("pub const renamed_object: &[::core::primitive::u8; 4]"),
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
fn integer_128_constants_preserve_full_values_without_weakening_c_abi_checks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("integer.h");
    std::fs::write(&path,"static const __int128 high=((__int128)1)<<100; static const unsigned __int128 all=~(unsigned __int128)0;").unwrap();
    let source = builder(&path).generate().unwrap().to_string();
    assert!(
        source
            .contains("pub const high: ::core::primitive::i128 = 1267650600228229401496703205376;"),
        "{source}"
    );
    assert!(
        source.contains(
            "pub const all: ::core::primitive::u128 = 340282366920938463463374607431768211455;"
        ),
        "{source}"
    );
    for declaration in [
        "extern __int128 value;",
        "__int128 function(__int128);",
        "struct Record {__int128 value;};",
        "typedef __int128 Wide; static const Wide value=1;",
    ] {
        std::fs::write(&path, declaration).unwrap();
        assert!(
            builder(&path)
                .generate()
                .unwrap_err()
                .to_string()
                .contains("128-bit C ABI types require Rust 1.78"),
            "{declaration}"
        );
    }
}

#[test]
fn earlier_const_values_follow_selected_occurrences_and_name_callbacks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("root.h");
    std::fs::write(
        dir.path().join("first.h"),
        "extern const int object; static const int first=7;\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("second.h"),
        "const int object=first+1; static const int internal=object*2;\n",
    )
    .unwrap();
    std::fs::write(&path, "#include \"first.h\"\n#include \"second.h\"\n").unwrap();
    let all = builder(&path)
        .parse_callbacks(Box::new(Rename))
        .generate()
        .unwrap()
        .to_string();
    assert!(all.contains("pub static renamed_object:"), "{all}");
    assert!(
        all.contains("pub const internal: ::core::ffi::c_int = 16;"),
        "{all}"
    );
    let selected = builder(&path)
        .allowlist_file(r".*[/\\]second\.h")
        .parse_callbacks(Box::new(Rename))
        .generate()
        .unwrap()
        .to_string();
    assert!(
        selected.contains("pub const renamed_object: ::core::ffi::c_int = 8;"),
        "{selected}"
    );
    assert!(
        selected.contains("pub const internal: ::core::ffi::c_int = 16;"),
        "{selected}"
    );
    assert!(!selected.contains("pub const first:"), "{selected}");
}

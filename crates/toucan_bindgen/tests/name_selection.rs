use std::cell::Cell;
use std::path::Path;

use toucan_bindgen::callbacks::{ItemInfo, ParseCallbacks};
use toucan_bindgen::{Builder, Formatter};

fn builder(path: &Path) -> Builder {
    Builder::default()
        .header(path.to_str().unwrap())
        .clang_args(["-std=c11", "--target=x86_64-unknown-linux-gnu"])
        .layout_tests(false)
        .generate_comments(false)
        .formatter(Formatter::None)
}

fn input(source: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.h");
    std::fs::write(&path, source).unwrap();
    (dir, path)
}

#[test]
fn anchored_patterns_union_categories_without_crossing_c_namespaces() {
    let (_dir, path) = input(
        "typedef int Scalar; struct Shared { Scalar x; }; int Shared(struct Shared *); extern int value;\n#define MACRO 3\n",
    );
    let types = builder(&path)
        .allowlist_type("S(hared|calar)")
        .generate()
        .unwrap()
        .to_string();
    assert!(types.contains("pub struct Shared"));
    assert!(types.contains("pub type Scalar"));
    assert!(!types.contains("pub fn Shared"));
    assert!(!types.contains("pub static mut value:"));
    assert!(!types.contains("pub const MACRO:"));
    let function = builder(&path)
        .allowlist_function("Shared")
        .generate()
        .unwrap()
        .to_string();
    assert!(function.contains("pub fn Shared("));
    assert!(function.contains("pub struct Shared"));
    let union = builder(&path)
        .allowlist_type("^Shared$")
        .allowlist_var("value")
        .allowlist_var("MACRO")
        .generate()
        .unwrap()
        .to_string();
    assert!(union.contains("pub static mut value:"));
    assert!(union.contains("pub const MACRO:"));
    assert!(!union.contains("pub fn Shared"));
    for filtered in [
        builder(&path).allowlist_type("Shar"),
        builder(&path).allowlist_function("value"),
        builder(&path).allowlist_var("Shared"),
    ] {
        let source = filtered.generate().unwrap().to_string();
        assert!(!source.contains("pub "), "{source}");
    }
    for (filtered, kind) in [
        (builder(&path).allowlist_type("["), "type"),
        (builder(&path).allowlist_function("["), "function"),
        (builder(&path).allowlist_var("["), "var"),
    ] {
        assert!(
            filtered
                .generate()
                .unwrap_err()
                .to_string()
                .contains(&format!("invalid allowlist_{kind} pattern"))
        );
    }
}

#[test]
fn lexical_tags_and_anonymous_enum_roots_follow_reference_names() {
    let (_dir, path) = input(
        "struct Outer { struct Inner {int x;} member; enum Nested {NESTED=1} e; }; enum Named {NAMED=2}; enum {ANON_A=3, ANON_B=4}; typedef enum {FIRST=5} First, Later;",
    );
    for name in ["Inner", "Nested", "NAMED", "FIRST"] {
        let source = builder(&path)
            .allowlist_type(name)
            .generate()
            .unwrap()
            .to_string();
        assert!(!source.contains("pub "), "{source}");
    }
    let source = builder(&path)
        .allowlist_type("Outer_Inner")
        .generate()
        .unwrap()
        .to_string();
    assert!(source.contains("pub struct Outer_Inner"));
    assert!(!source.contains("pub struct Outer {"));
    for name in ["NAMED", "Named_NAMED", "FIRST", "First_FIRST", "NESTED"] {
        let source = builder(&path)
            .allowlist_var(name)
            .generate()
            .unwrap()
            .to_string();
        assert!(!source.contains("pub "), "{source}");
    }
    let source = builder(&path)
        .allowlist_var("ANON_A")
        .rustified_enum(".*")
        .generate()
        .unwrap()
        .to_string();
    assert!(source.contains("pub const ANON_A:"));
    assert!(source.contains("pub const ANON_B:"));
    assert!(source.contains("pub enum _bindgen_ty_1"));
    let source = builder(&path)
        .allowlist_type("Later")
        .generate()
        .unwrap()
        .to_string();
    assert!(source.contains("pub type First"));
    assert!(source.contains("pub const First_FIRST:"));
    assert!(source.contains("pub type Later"));
}

#[derive(Debug, Default)]
struct Rename(Cell<usize>);
impl ParseCallbacks for Rename {
    fn generated_name_override(&self, item: ItemInfo<'_>) -> Option<String> {
        if item.name == "shared" {
            let count = self.0.get();
            self.0.set(count + 1);
            Some(format!("shared_{count}"))
        } else {
            item.name.strip_prefix("pre_").map(str::to_owned)
        }
    }
}

#[test]
fn callback_occurrences_are_matched_before_file_and_name_union() {
    let (dir, path) = input("#include \"a.h\"\n#include \"b.h\"\nint pre_call(void);");
    std::fs::write(
        dir.path().join("a.h"),
        "int shared(int); extern int from_a;",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.h"),
        "int shared(int); extern int from_b;",
    )
    .unwrap();
    for (name, emitted) in [
        ("shared_0", "shared_0"),
        ("shared_1", "shared_1"),
        ("shared", "shared_1"),
    ] {
        let source = builder(&path)
            .allowlist_file(r".*[/\\]b\.h")
            .allowlist_function(name)
            .parse_callbacks(Box::<Rename>::default())
            .generate()
            .unwrap()
            .to_string();
        assert!(source.contains(&format!("pub fn {emitted}(")), "{source}");
        assert!(source.contains("#[link_name = \"shared\"]"));
        assert!(source.contains("pub static mut from_b:"));
        assert!(!source.contains("pub static mut from_a:"));
    }
    for (name, emitted) in [("pre_call", false), ("call", true)] {
        let source = builder(&path)
            .allowlist_function(name)
            .parse_callbacks(Box::<Rename>::default())
            .generate()
            .unwrap()
            .to_string();
        assert_eq!(source.contains("pub fn call("), emitted);
    }
}

#[test]
fn selected_object_occurrence_supplies_both_name_and_initializer() {
    let (dir, path) = input("#include \"a.h\"\n#include \"b.h\"\n");
    std::fs::write(dir.path().join("a.h"), "extern const int shared;").unwrap();
    std::fs::write(dir.path().join("b.h"), "const int shared=7;").unwrap();
    for (name, constant) in [("shared_0", false), ("shared_1", true)] {
        let source = builder(&path)
            .allowlist_var(name)
            .parse_callbacks(Box::<Rename>::default())
            .generate()
            .unwrap()
            .to_string();
        assert_eq!(
            source.contains(&format!("pub const {name}:")),
            constant,
            "{source}"
        );
        assert_eq!(
            source.contains(&format!("pub static {name}:")),
            !constant,
            "{source}"
        );
    }
    let error = builder(&path)
        .allowlist_file(r".*[/\\]a\.h")
        .allowlist_var("shared_1")
        .parse_callbacks(Box::<Rename>::default())
        .generate()
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("multiple selected generated names")
    );
}

#[test]
fn macro_names_select_historical_values_and_keep_excluded_context() {
    let (dir, path) = input("#include \"a.h\"\n#include \"b.h\"\n");
    std::fs::write(
        dir.path().join("a.h"),
        "#define BASE 3\n#define VALUE BASE\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.h"),
        "#undef VALUE\n#define VALUE 9\n#define LATER VALUE\n",
    )
    .unwrap();
    let source = builder(&path)
        .allowlist_var("VALUE|LATER")
        .generate()
        .unwrap()
        .to_string();
    assert!(
        source.contains("pub const VALUE: ::core::primitive::u32 = 3;"),
        "{source}"
    );
    assert!(
        source.contains("pub const LATER: ::core::primitive::u32 = 9;"),
        "{source}"
    );
    assert!(!source.contains("pub const BASE:"));
    let source = builder(&path)
        .allowlist_file(r".*[/\\]b\.h")
        .allowlist_var("VALUE")
        .generate()
        .unwrap()
        .to_string();
    assert!(source.contains("pub const VALUE: ::core::primitive::u32 = 3;"));
    assert!(source.contains("pub const LATER:"));
}

#[test]
fn omitted_symbols_keep_selected_type_dependencies_and_cycles_terminate() {
    let (_dir, path) = input(
        "typedef int Scalar; struct Shared {Scalar x; struct Shared *next;}; static void hidden(struct Shared*); inline void ignored(struct Shared *s) {(void)s;} void pre_call(struct Shared*);",
    );
    let source = builder(&path)
        .allowlist_function("hidden")
        .generate()
        .unwrap()
        .to_string();
    assert!(source.contains("pub struct Shared"));
    assert!(source.contains("pub type Scalar"));
    assert!(!source.contains("pub fn hidden"));
    assert!(
        !builder(&path)
            .allowlist_function("ignored")
            .generate()
            .unwrap()
            .to_string()
            .contains("pub ")
    );
    let source = builder(&path)
        .allowlist_function("call")
        .blocklist_function("call")
        .parse_callbacks(Box::<Rename>::default())
        .generate()
        .unwrap()
        .to_string();
    assert!(source.contains("pub struct Shared"));
    assert!(!source.contains("pub fn call"));
    let source = builder(&path)
        .allowlist_type("Shared")
        .blocklist_type("Shared")
        .generate()
        .unwrap()
        .to_string();
    assert!(source.contains("pub type Scalar"));
    assert!(!source.contains("pub struct Shared"));
}

#[test]
#[ignore = "requires rustc"]
fn selected_bindings_compile_as_rust() {
    let (dir, path) = input(
        "typedef int Scalar; struct Shared {Scalar x;}; void call(struct Shared*); enum {ANON_A=1, ANON_B=2};\n#define MACRO 4\n",
    );
    let source = builder(&path)
        .allowlist_function("call")
        .allowlist_var("ANON_A|MACRO")
        .generate()
        .unwrap();
    let rust = dir.path().join("bindings.rs");
    source.write_to_file(&rust).unwrap();
    let output = std::process::Command::new("rustc")
        .args([
            "--edition=2021",
            "--crate-type=lib",
            "--emit=metadata",
            "-A",
            "warnings",
        ])
        .arg(&rust)
        .arg("-o")
        .arg(dir.path().join("bindings.rmeta"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn adjusted_parameter_aliases_follow_selected_callback_occurrences() {
    let (dir, path) = input("#include \"types.h\"\n#include \"a.h\"\n#include \"b.h\"\n");
    std::fs::write(
        dir.path().join("types.h"),
        "typedef int A[4]; typedef int D[8];",
    )
    .unwrap();
    std::fs::write(dir.path().join("a.h"), "void shared(void(*cb)(A));").unwrap();
    std::fs::write(dir.path().join("b.h"), "void shared(void(*cb)(D));").unwrap();
    for (name, a, d) in [("shared_0", true, false), ("shared_1", false, true)] {
        let source = builder(&path)
            .allowlist_function(name)
            .parse_callbacks(Box::<Rename>::default())
            .generate()
            .unwrap()
            .to_string();
        assert_eq!(source.contains("pub type A ="), a, "{source}");
        assert_eq!(source.contains("pub type D ="), d, "{source}");
        assert!(source.contains(&format!("pub fn {name}(")));
    }
    let source = builder(&path)
        .allowlist_function("shared_0")
        .allowlist_file(r".*[/\\]b\.h")
        .parse_callbacks(Box::<Rename>::default())
        .generate()
        .unwrap()
        .to_string();
    assert!(source.contains("pub type A ="));
    assert!(source.contains("pub type D ="));
    assert!(source.contains("pub fn shared_0("));
    let (_dir, path) = input(
        "typedef int Array[4]; typedef void Callback(Array); struct Holder {void(*callback)(Array);};",
    );
    for name in ["Callback", "Holder"] {
        let source = builder(&path)
            .allowlist_type(name)
            .blocklist_type(name)
            .generate()
            .unwrap()
            .to_string();
        assert!(source.contains("pub type Array ="), "{source}");
        assert!(!source.contains("pub struct Holder"));
        assert!(!source.contains("pub type Callback"));
    }
}

#[test]
fn shared_dependencies_of_blocked_cyclic_records_are_collected_once() {
    let (_dir, path) = input(
        "typedef int Scalar; struct Root; struct Leaf {Scalar x; struct Root *root;}; struct Left {struct Leaf *leaf;}; struct Right {struct Leaf *leaf;}; struct Root {struct Left left; struct Right right;};",
    );
    let source = builder(&path)
        .allowlist_type("Root")
        .blocklist_type("Root")
        .generate()
        .unwrap()
        .to_string();
    for name in ["Leaf", "Left", "Right"] {
        assert_eq!(
            source.matches(&format!("pub struct {name} ")).count(),
            1,
            "{source}"
        );
    }
    assert_eq!(source.matches("pub type Scalar =").count(), 1);
    assert!(!source.contains("pub struct Root"));
}

#[test]
fn reached_enum_constants_cannot_collide_with_selected_macros() {
    let (_dir, path) = input("enum Named { A=1 };\n#define Named_A 2\n");
    let error = builder(&path)
        .allowlist_type("Named")
        .allowlist_var("Named_A")
        .generate()
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("generated Rust name `Named_A` conflicts")
    );
}

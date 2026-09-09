use std::cell::Cell;
use toucan_bindgen::callbacks::{ItemInfo, ParseCallbacks};
use toucan_bindgen::{Builder, Formatter};

#[derive(Debug, Default)]
struct Rename {
    count: Cell<usize>,
    repeat: bool,
}
impl ParseCallbacks for Rename {
    fn generated_name_override(&self, item: ItemInfo<'_>) -> Option<String> {
        if item.name != "shared" {
            return None;
        }
        let index = self.count.get();
        self.count.set(index + 1);
        Some(format!(
            "shared_{}",
            if self.repeat { index % 2 } else { index }
        ))
    }
}

fn builder(path: &std::path::Path, repeat: bool) -> Builder {
    Builder::default()
        .header(path.to_str().unwrap())
        .clang_args(["-std=c11", "--target=x86_64-unknown-linux-gnu"])
        .formatter(Formatter::None)
        .generate_comments(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(Rename {
            repeat,
            ..Rename::default()
        }))
}

#[test]
fn selected_names_keep_their_written_values_and_share_the_c_symbol() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.h");
    for (source, constant) in [
        ("extern const int shared; const int shared=7;", "shared_1"),
        ("const int shared=7; extern const int shared;", "shared_0"),
    ] {
        std::fs::write(&path, source).unwrap();
        let bindings = builder(&path, false)
            .allowlist_var("shared_.*")
            .generate()
            .unwrap();
        assert_eq!(bindings.report().declarations, 2);
        let source = bindings.to_string();
        assert!(
            source.contains(&format!("pub const {constant}: ::core::ffi::c_int = 7;")),
            "{source}"
        );
        let external = if constant == "shared_0" {
            "shared_1"
        } else {
            "shared_0"
        };
        assert!(
            source.contains(&format!("pub static {external}:")),
            "{source}"
        );
        assert_eq!(source.matches("#[link_name = \"shared\"]").count(), 1);
    }
}

#[test]
fn repeated_generated_names_use_the_first_selected_occurrence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.h");
    std::fs::write(
        &path,
        "extern int shared; extern int shared; int shared=7; extern int shared;",
    )
    .unwrap();
    let bindings = builder(&path, true)
        .allowlist_var("shared_.*")
        .generate()
        .unwrap();
    assert_eq!(bindings.report().declarations, 2);
    let source = bindings.to_string();
    for name in ["shared_0", "shared_1"] {
        assert_eq!(
            source.matches(&format!("pub static mut {name}:")).count(),
            1,
            "{source}"
        );
        assert!(!source.contains(&format!("pub const {name}:")));
    }
}

#[test]
fn extra_string_occurrences_preserve_byte_projection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.h");
    std::fs::write(
        &path,
        "extern const char shared[4]; const char shared[4]=\"abc\";",
    )
    .unwrap();
    let source = builder(&path, false)
        .allowlist_var("shared.*")
        .generate()
        .unwrap()
        .to_string();
    assert!(
        source.contains("pub static shared_0: [::core::ffi::c_char; 4]"),
        "{source}"
    );
    assert!(
        source.contains("pub const shared_1: &[::core::primitive::u8; 4] = &[97, 98, 99, 0, ];")
    );
}

#[test]
fn extra_names_cannot_collide_with_functions_or_macros() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.h");
    for source in [
        "extern int shared; extern int shared; void shared_1(void);",
        "extern int shared; extern int shared;\n#define shared_1 3\n",
    ] {
        std::fs::write(&path, source).unwrap();
        let error = builder(&path, false)
            .allowlist_var("shared.*")
            .allowlist_function("shared_1")
            .generate()
            .unwrap_err();
        assert!(error.to_string().contains("conflicts"), "{error}");
    }
}

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use toucan_bindgen::Builder;
use toucan_bindgen::callbacks::{ItemInfo, ParseCallbacks};

#[derive(Debug)]
struct Counter {
    count: Cell<usize>,
    log: Rc<RefCell<Vec<String>>>,
    thread: std::thread::ThreadId,
}
impl ParseCallbacks for Counter {
    fn generated_name_override(&self, item: ItemInfo<'_>) -> Option<String> {
        assert_eq!(std::thread::current().id(), self.thread);
        self.log.borrow_mut().push(item.name.into());
        if item.name == "shared" {
            let value = self.count.get();
            self.count.set(value + 1);
            Some(format!("shared_{value}"))
        } else {
            item.name.strip_prefix("pre_").map(str::to_owned)
        }
    }
}
fn builder(header: &std::path::Path) -> Builder {
    Builder::default()
        .header(header.to_str().unwrap())
        .clang_arg("--target=x86_64-unknown-linux-gnu")
        .layout_tests(false)
}

#[test]
fn file_filters_use_physical_paths_redeclarations_and_source_order_callbacks() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.h");
    let b = dir.path().join("b.h");
    let root = dir.path().join("root.h");
    std::fs::write(a,"struct Shared; extern int shared(int); extern int pre_a; typedef int Number; enum A{A_VALUE=2};\n#define A_MACRO 1\n").unwrap();
    std::fs::write(b,"#line 400 \"logical.h\"\nstruct Shared {int value;}; extern int shared(int); extern int pre_b; typedef int Number;\n#define B_MACRO 3\n").unwrap();
    std::fs::write(
        &root,
        "#include \"a.h\"\n#include \"b.h\"\nextern int pre_root;\n",
    )
    .unwrap();
    for (pattern, expected) in [(r".*[/\\]a\.h", "shared_0"), (r".*[/\\]b\.h", "shared_1")] {
        let log = Rc::new(RefCell::new(Vec::new()));
        let output = builder(&root)
            .allowlist_file(pattern)
            .parse_callbacks(Box::new(Counter {
                count: Cell::new(0),
                log: Rc::clone(&log),
                thread: std::thread::current().id(),
            }))
            .generate()
            .unwrap()
            .to_string();
        assert!(output.contains(&format!("pub fn {expected}(")), "{output}");
        assert!(output.contains("#[link_name = \"shared\"]"));
        assert!(output.contains("pub struct Shared"));
        assert!(output.contains("pub type Number"));
        assert!(!output.contains("pub static mut root:"));
        assert_eq!(
            &*log.borrow(),
            &["shared", "pre_a", "shared", "pre_b", "pre_root"]
        );
        if expected == "shared_0" {
            assert!(output.contains("pub const A_MACRO"));
            assert!(!output.contains("pub const B_MACRO"));
        } else {
            assert!(output.contains("pub const B_MACRO"));
            assert!(!output.contains("pub const A_MACRO"));
        }
    }
    let absent = builder(&root)
        .allowlist_file("logical.h")
        .generate()
        .unwrap()
        .to_string();
    assert!(!absent.contains("pub fn "));
    assert!(!absent.contains("pub struct "));
    assert!(builder(&root).allowlist_file("[").generate().is_err());
}

#[test]
fn tag_reference_only_headers_do_not_select_existing_types() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("definition.h"), "struct Existing{int x;};").unwrap();
    std::fs::write(
        dir.path().join("reference.h"),
        "_Static_assert(sizeof(struct Existing)==4,\"\"); struct Existing;",
    )
    .unwrap();
    let root = dir.path().join("root.h");
    std::fs::write(
        &root,
        "#include \"definition.h\"\n#include \"reference.h\"\n",
    )
    .unwrap();
    let output = builder(&root)
        .allowlist_file(r".*[/\\]reference\.h")
        .generate()
        .unwrap()
        .to_string();
    assert!(!output.contains("pub struct "), "{output}");
}

#[test]
fn callbacks_follow_native_inline_and_linkage_eligibility() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root.h");
    std::fs::write(&root,"static int pre_static=1; static int pre_static_prototype(int); inline int pre_inline(int x){return x;} extern inline int pre_extern_inline(int x){return x;} int pre_later(int); inline int pre_later(int x){return x;} int pre_definition(void){return 1;} extern int pre_external;").unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    let output = builder(&root)
        .parse_callbacks(Box::new(Counter {
            count: Cell::new(0),
            log: Rc::clone(&log),
            thread: std::thread::current().id(),
        }))
        .generate()
        .unwrap()
        .to_string();
    assert_eq!(
        &*log.borrow(),
        &["pre_static_prototype", "pre_definition", "pre_external"]
    );
    assert!(output.contains("pub fn definition("));
    assert!(output.contains("pub static mut external:"));
    assert!(!output.contains("pub fn pre_inline("));
    assert!(!output.contains("pub fn pre_later("));
}

#[derive(Debug)]
struct ChooseName {
    label: &'static str,
    rename: bool,
    log: Rc<RefCell<Vec<String>>>,
}
impl ParseCallbacks for ChooseName {
    fn generated_name_override(&self, item: ItemInfo<'_>) -> Option<String> {
        self.log
            .borrow_mut()
            .push(format!("{}:{}", self.label, item.name));
        self.rename.then(|| "chosen".into())
    }
}
#[test]
fn newest_callback_wins_and_selected_renames_cannot_collide() {
    let dir = tempfile::tempdir().unwrap();
    let header = dir.path().join("input.h");
    std::fs::write(&header, "int one(void);").unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    let output = builder(&header)
        .parse_callbacks(Box::new(ChooseName {
            label: "older",
            rename: true,
            log: Rc::clone(&log),
        }))
        .parse_callbacks(Box::new(ChooseName {
            label: "newer",
            rename: false,
            log: Rc::clone(&log),
        }))
        .generate()
        .unwrap()
        .to_string();
    assert_eq!(&*log.borrow(), &["newer:one", "older:one"]);
    assert!(output.contains("pub fn chosen("));
    log.borrow_mut().clear();
    let output = builder(&header)
        .parse_callbacks(Box::new(ChooseName {
            label: "older",
            rename: true,
            log: Rc::clone(&log),
        }))
        .parse_callbacks(Box::new(ChooseName {
            label: "newer",
            rename: true,
            log: Rc::clone(&log),
        }))
        .generate()
        .unwrap()
        .to_string();
    assert_eq!(&*log.borrow(), &["newer:one"]);
    assert!(output.contains("pub fn chosen("));
    std::fs::write(&header, "int one(void); int two(void);").unwrap();
    let error = builder(&header)
        .parse_callbacks(Box::new(ChooseName {
            label: "same",
            rename: true,
            log,
        }))
        .generate()
        .unwrap_err();
    assert!(error.to_string().contains("conflicts between"));
}

#[test]
fn function_blocklists_match_the_callback_adjusted_name() {
    let dir = tempfile::tempdir().unwrap();
    let header = dir.path().join("input.h");
    std::fs::write(&header, "int one(void);").unwrap();
    for (pattern, emitted) in [("chosen", false), ("one", true)] {
        let result = builder(&header)
            .blocklist_function(pattern)
            .parse_callbacks(Box::new(ChooseName {
                label: "rename",
                rename: true,
                log: Rc::new(RefCell::new(Vec::new())),
            }))
            .generate()
            .unwrap();
        assert_eq!(result.to_string().contains("pub fn chosen("), emitted);
    }
}

#[test]
fn all_files_excludes_the_compiler_alias_prelude_but_keeps_written_redeclarations() {
    let dir = tempfile::tempdir().unwrap();
    let header = dir.path().join("input.h");
    std::fs::write(&header, "int ordinary(void);").unwrap();
    let output = builder(&header)
        .allowlist_file(".*")
        .generate()
        .unwrap()
        .to_string();
    assert!(output.contains("pub fn ordinary("));
    assert!(!output.contains("pub type __int128_t"));
    std::fs::write(&header, "typedef __int128 __int128_t;").unwrap();
    let error = builder(&header)
        .allowlist_file(".*")
        .generate()
        .unwrap_err();
    assert!(error.to_string().contains("1.78"));
}

#[derive(Debug)]
struct EmptyName;
impl ParseCallbacks for EmptyName {
    fn generated_name_override(&self, _: ItemInfo<'_>) -> Option<String> {
        Some(String::new())
    }
}
#[test]
fn empty_callback_results_are_diagnosed_before_file_selection() {
    let dir = tempfile::tempdir().unwrap();
    let header = dir.path().join("input.h");
    std::fs::write(&header, "int one(void);").unwrap();
    let error = builder(&header)
        .allowlist_file("unselected")
        .parse_callbacks(Box::new(EmptyName))
        .generate()
        .unwrap_err();
    assert!(error.to_string().contains("cannot be empty"));
}

#[test]
fn relative_main_and_include_names_match_file_patterns() {
    const CHILD: &str = "TOUCAN_FILE_SELECTION_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        // This child runs one test, so changing cwd cannot affect parallel tests.
        std::env::set_current_dir(path).unwrap();
        for (pattern, root, included) in [
            (r"root\.h", true, false),
            (r".*[/\\]root\.h", false, false),
            (r".*[/\\]a\.h", false, true),
        ] {
            let output = builder(std::path::Path::new("root.h"))
                .allowlist_file(pattern)
                .generate()
                .unwrap()
                .to_string();
            assert_eq!(output.contains("pub static mut root:"), root, "{output}");
            assert_eq!(
                output.contains("pub static mut included:"),
                included,
                "{output}"
            );
        }
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("root.h"),
        "#include \"a.h\"\nextern int root;\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("a.h"),
        "#line 90 \"logical.h\"\nextern int included;\n",
    )
    .unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "relative_main_and_include_names_match_file_patterns",
            "--nocapture",
        ])
        .env(CHILD, dir.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[cfg(unix)]
#[test]
fn header_path_spelling_is_passed_as_data() {
    let dir = tempfile::tempdir().unwrap();
    let header = dir.path().join("quoted\"header.h");
    std::fs::write(&header, "extern int direct_path;\n").unwrap();
    let result = builder(&header).generate().unwrap().to_string();
    assert!(result.contains("pub static mut direct_path:"));
}

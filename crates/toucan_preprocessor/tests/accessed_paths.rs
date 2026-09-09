use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use toucan_preprocessor::{Config, Preprocessor};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "toucan-accessed-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, path: &str, source: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn config(clang: bool) -> Config {
    let mut config = Config {
        record_file_origins: true,
        ..Default::default()
    };
    if clang {
        config.defines.insert("__clang__".into(), "1".into());
    }
    config
}
fn compact(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn literal_paths_macro_origins_and_logical_names_are_separate() {
    let files = Files::new();
    files.write(
        "root.h",
        "#include \"sub/./nested.h\"\n#include \"leaf.h\"\n",
    );
    files.write("sub/nested.h", "#include \"../leaf.h\"\n");
    files.write(
        "leaf.h",
        "#define VALUE 1\nconst char *name=__FILE__;\n#line 80 \"logical.h\"\nint value;\n",
    );
    for clang in [false, true] {
        let result = Preprocessor::new(config(clang))
            .preprocess(&files.0.join("root.h"))
            .unwrap();
        let expected = files.0.join("sub/./../leaf.h");
        assert!(
            result.source.contains(&format!("{:?}", expected)),
            "{}",
            result.source
        );
        let origins = result.file_origins().unwrap();
        let physical = std::fs::canonicalize(files.0.join("leaf.h")).unwrap();
        let offset = result.source.find("value").unwrap();
        assert_eq!(origins.source_file(offset), Some(physical.as_path()));
        assert_eq!(
            origins.source_name(offset).unwrap().as_os_str(),
            expected.as_os_str()
        );
        assert_eq!(
            result.resolve_location(offset).unwrap().path.as_ref(),
            Path::new("logical.h")
        );
        let last = if clang {
            expected
        } else {
            files.0.join("leaf.h")
        };
        assert_eq!(
            origins.macro_definition_name("VALUE").unwrap().as_os_str(),
            last.as_os_str()
        );
        assert_eq!(
            origins.macro_definition("VALUE").unwrap().path.as_ref(),
            physical.as_path()
        );
        assert_eq!(origins.macro_definition("VALUE").unwrap().line, 1);
        assert_eq!(result.dependencies.len(), 3);
        assert!(origins.source_name(result.source.len()).is_none());
    }
}

#[test]
fn ordered_inputs_share_macros_and_main_bypasses_prior_once() {
    let files = Files::new();
    files.write("first.h", "#pragma once\n#define FIRST 7\nint once;\n");
    files.write("main.h", "int result[FIRST];\n");
    for clang in [false, true] {
        let mut processor = Preprocessor::new(config(clang));
        let paths = [
            files.0.join("first.h"),
            files.0.join("./first.h"),
            files.0.join("main.h"),
        ];
        let result = processor.preprocess_files(&paths).unwrap();
        assert_eq!(compact(&result.source), "intonce;intresult[7];");
        assert_eq!(result.dependencies.len(), 2);
        let result = processor
            .preprocess_files(&[files.0.join("first.h"), files.0.join("first.h")])
            .unwrap();
        assert_eq!(compact(&result.source), "intonce;intonce;");
        assert_eq!(result.dependencies.len(), 1);
        assert!(
            processor
                .preprocess_files(&[])
                .unwrap_err()
                .message
                .contains("at least one")
        );
        assert_eq!(
            compact(
                &processor
                    .preprocess(&files.0.join("first.h"))
                    .unwrap()
                    .source
            ),
            "intonce;"
        );
    }
    let mut config = config(false);
    config.include_dirs.push(files.0.clone());
    assert!(
        Preprocessor::new(config.clone())
            .preprocess_files(&["first.h".into(), files.0.join("main.h")])
            .is_ok()
    );
    assert!(
        Preprocessor::new(config)
            .preprocess(Path::new("first.h"))
            .is_err()
    );
    let mut processor = Preprocessor::new(Config {
        allow_filesystem: false,
        ..Default::default()
    });
    assert!(
        processor
            .preprocess_files(&[files.0.join("first.h")])
            .unwrap_err()
            .message
            .contains("filesystem access is disabled")
    );
}

#[cfg(unix)]
#[test]
fn symlink_parents_and_queries_follow_the_compiler_access_name() {
    let files = Files::new();
    files.write(
        "root.h",
        "#include \"links/alias.h\"\n#include \"target/actual.h\"\n",
    );
    files.write("target/actual.h", "#if !__has_include(\"sibling.h\")\n#error missing sibling\n#endif\n#include \"sibling.h\"\n");
    files.write("target/sibling.h", "int target;\n");
    files.write("links/sibling.h", "int link;\n");
    std::os::unix::fs::symlink("../target/actual.h", files.0.join("links/alias.h")).unwrap();
    for clang in [false, true] {
        let result = Preprocessor::new(config(clang))
            .preprocess(&files.0.join("root.h"))
            .unwrap();
        assert_eq!(
            compact(&result.source),
            if clang {
                "intlink;intlink;"
            } else {
                "intlink;inttarget;"
            }
        );
        assert_eq!(result.dependencies.len(), if clang { 3 } else { 4 });
    }
    files.write("target/actual.h", "#pragma once\nint once;\n");
    for clang in [false, true] {
        let result = Preprocessor::new(config(clang))
            .preprocess(&files.0.join("root.h"))
            .unwrap();
        assert_eq!(compact(&result.source), "intonce;");
        assert_eq!(result.dependencies.len(), 2);
    }
}

// The macOS runner rejects the invalid UTF-8 directory name before preprocessing.
#[cfg(target_os = "linux")]
#[test]
fn non_utf8_header_names_keep_literal_parent_components() {
    use std::os::unix::ffi::OsStrExt;
    let files = Files::new();
    let directory = files.0.join(std::ffi::OsStr::from_bytes(b"non-utf8-\xff"));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("main.h"), "#include \"./child.h\"\n").unwrap();
    std::fs::write(directory.join("child.h"), "#include \"./leaf.h\"\n").unwrap();
    std::fs::write(directory.join("leaf.h"), "int leaf;\n").unwrap();
    let result = Preprocessor::new(config(false))
        .preprocess(&directory.join("main.h"))
        .unwrap();
    assert_eq!(
        result
            .file_origins()
            .unwrap()
            .source_name(0)
            .unwrap()
            .as_os_str(),
        directory.join("././leaf.h").as_os_str()
    );
}

#[test]
#[ignore = "requires GCC and Clang; child process isolates its working directory"]
fn accessed_path_spelling_matches_native() {
    const CHILD: &str = "TOUCAN_ACCESSED_PATH_ORACLE_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        // Only this test runs in the child; changing its cwd cannot race other tests.
        std::env::set_current_dir(path).unwrap();
        native_cases();
        return;
    }
    let files = Files::new();
    files.write(
        "root.h",
        "const char *main_name=__FILE__;\n#include \"sub/./nested.h\"\n#include <order.h>\n",
    );
    files.write(
        "sub/nested.h",
        "const char *nested_name=__FILE__;\n#include \"../leaf.h\"\n",
    );
    files.write("leaf.h", "const char *leaf_name=__FILE__;\n#line 90 \"logical.h\"\nconst char *logical_name=__FILE__;\n");
    files.write("first/order.h", "const char *order_name=__FILE__;\n");
    files.write("second/order.h", "#error wrong include directory\n");
    files.write("once.h", "#pragma once\nconst char *once_name=__FILE__;\n");
    files.write(
        "self.h",
        "#pragma once\nconst char *self_name=__FILE__;\n#include \"self.h\"\n",
    );
    files.write("empty.h", "int end;\n");
    files.write("repeat.h", "const char *repeat_name=__FILE__;\n");
    files.write(
        "aliases.h",
        "#include \"repeat.h\"\n#include \"./repeat.h\"\n",
    );
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "accessed_path_spelling_matches_native",
            "--nocapture",
        ])
        .env(CHILD, &files.0)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

fn native_cases() {
    let absolute = std::env::current_dir().unwrap().join("root.h");
    let cases: Vec<Vec<PathBuf>> = vec![
        vec!["root.h".into()],
        vec!["./root.h".into()],
        vec![absolute],
        vec!["self.h".into()],
        vec!["aliases.h".into()],
        vec!["once.h".into(), "once.h".into()],
        vec!["once.h".into(), "./once.h".into(), "empty.h".into()],
        vec!["repeat.h".into(), "./repeat.h".into()],
        vec!["./repeat.h".into(), "repeat.h".into()],
        vec!["./repeat.h".into(), "empty.h".into()],
        vec!["order.h".into(), "empty.h".into()],
    ];
    for clang in [false, true] {
        let compiler = std::env::var(if clang { "TOUCAN_CLANG" } else { "TOUCAN_GCC" })
            .unwrap_or_else(|_| if clang { "clang" } else { "gcc" }.into());
        for paths in &cases {
            let mut config = config(clang);
            config.include_dirs = vec!["first".into(), "second".into()];
            let actual = Preprocessor::new(config).preprocess_files(paths).unwrap();
            let mut command = Command::new(&compiler);
            command.args(["-E", "-P", "-x", "c", "-Ifirst", "-Isecond"]);
            for path in &paths[..paths.len() - 1] {
                command.arg("-include").arg(path);
            }
            let native = command.arg(paths.last().unwrap()).output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&native),
                Ok(true),
                "{command:?}: {}",
                String::from_utf8_lossy(&native.stderr)
            );
            assert_eq!(
                compact(&actual.source),
                compact(&String::from_utf8_lossy(&native.stdout)),
                "{command:?}"
            );
        }
    }
}

#[test]
fn exact_access_spellings_do_not_collapse_during_origin_interning() {
    let files = Files::new();
    files.write("root.h", "#include \"leaf.h\"\n#include \"./leaf.h\"\n");
    files.write("leaf.h", "#define VALUE 1\nint leaf;\n");
    let result = Preprocessor::new(config(false))
        .preprocess(&files.0.join("root.h"))
        .unwrap();
    let origins = result.file_origins().unwrap();
    let first = result.source.find("leaf").unwrap();
    let second = result.source.rfind("leaf").unwrap();
    assert_ne!(first, second);
    assert_eq!(
        origins.source_name(first).unwrap().as_os_str(),
        files.0.join("leaf.h").as_os_str()
    );
    assert_eq!(
        origins.source_name(second).unwrap().as_os_str(),
        files.0.join("./leaf.h").as_os_str()
    );
    assert_eq!(
        origins.macro_definition_name("VALUE").unwrap().as_os_str(),
        files.0.join("./leaf.h").as_os_str()
    );
}

#[cfg(unix)]
#[test]
fn hardlinks_share_once_and_clang_names_but_retain_dependency_paths() {
    let files = Files::new();
    files.write(
        "root.h",
        "#include \"a/header.h\"\n#include \"b/header.h\"\n",
    );
    files.write("a/header.h", "");
    files.write("a/choice.h", "int a_choice;\n");
    files.write("b/choice.h", "int b_choice;\n");
    std::fs::hard_link(files.0.join("a/header.h"), files.0.join("b/header.h")).unwrap();
    let first = std::fs::canonicalize(files.0.join("a/header.h")).unwrap();
    let second = std::fs::canonicalize(files.0.join("b/header.h")).unwrap();
    for once in [false, true] {
        files.write(
            "a/header.h",
            &format!(
                "{}#define MARKER 1\nconst char *name=__FILE__;\n#include \"choice.h\"\n",
                if once { "#pragma once\n" } else { "" }
            ),
        );
        for clang in [false, true] {
            let mut processor = Preprocessor::new(config(clang));
            let result = processor.preprocess(&files.0.join("root.h")).unwrap();
            assert_eq!(
                result.source.matches("const char").count(),
                if once { 1 } else { 2 }
            );
            assert_eq!(
                result.source.matches("a_choice").count(),
                if clang && !once { 2 } else { 1 }
            );
            assert_eq!(
                result.source.matches("b_choice").count(),
                usize::from(!clang && !once)
            );
            assert!(result.dependencies.contains(&first));
            assert_eq!(result.dependencies.contains(&second), clang || !once);
            let origins = result.file_origins().unwrap();
            assert_eq!(
                origins.macro_definition("MARKER").unwrap().path.as_ref(),
                if once { &first } else { &second }
            );
            assert_eq!(
                origins.macro_definition_name("MARKER").unwrap(),
                files.0.join(if clang || once {
                    "a/header.h"
                } else {
                    "b/header.h"
                })
            );

            // Register the main hard link first, even though the forced input is
            // processed first; the main input bypasses an earlier once marker.
            let result = processor
                .preprocess_files(&[first.clone(), second.clone()])
                .unwrap();
            assert_eq!(result.source.matches("const char").count(), 2);
            assert_eq!(
                result.source.matches("a_choice").count(),
                usize::from(!clang)
            );
            assert_eq!(
                result.source.matches("b_choice").count(),
                if clang { 2 } else { 1 }
            );
            assert!(result.dependencies.contains(&first));
            assert!(result.dependencies.contains(&second));
        }
    }
}

#[test]
fn identical_distinct_headers_are_not_file_aliases() {
    let files = Files::new();
    files.write(
        "root.h",
        "#include \"a/header.h\"\n#include \"b/header.h\"\n",
    );
    for prefix in ["a", "b"] {
        files.write(&format!("{prefix}/header.h"), "#pragma once\nint value;\n");
    }
    for clang in [false, true] {
        let result = Preprocessor::new(config(clang))
            .preprocess(&files.0.join("root.h"))
            .unwrap();
        assert_eq!(compact(&result.source), "intvalue;intvalue;");
        assert_eq!(result.dependencies.len(), 3);
    }
}

#[cfg(windows)]
#[test]
fn windows_hardlinks_have_an_explicit_identity_boundary() {
    let files = Files::new();
    files.write("original.h", "int value;\n");
    std::fs::hard_link(files.0.join("original.h"), files.0.join("alias.h")).unwrap();
    for clang in [false, true] {
        let error = Preprocessor::new(config(clang))
            .preprocess(&files.0.join("original.h"))
            .unwrap_err();
        assert!(error.message.contains("128-bit file identity"), "{error}");
    }
}

#[cfg(unix)]
#[test]
#[ignore = "requires GCC and Clang"]
fn hardlink_names_and_ordered_inputs_match_native() {
    let files = Files::new();
    files.write(
        "root.h",
        "#include \"a/header.h\"\n#include \"b/header.h\"\n",
    );
    files.write("a/header.h", "");
    files.write("a/choice.h", "int a_choice;\n");
    files.write("b/choice.h", "int b_choice;\n");
    std::fs::hard_link(files.0.join("a/header.h"), files.0.join("b/header.h")).unwrap();
    for once in [false, true] {
        files.write(
            "a/header.h",
            &format!(
                "{}const char *name=__FILE__;\n#include \"choice.h\"\n",
                if once { "#pragma once\n" } else { "" }
            ),
        );
        for clang in [false, true] {
            let compiler = std::env::var(if clang { "TOUCAN_CLANG" } else { "TOUCAN_GCC" })
                .unwrap_or_else(|_| if clang { "clang" } else { "gcc" }.into());
            for paths in [
                vec![files.0.join("root.h")],
                vec![files.0.join("a/header.h"), files.0.join("b/header.h")],
            ] {
                let actual = Preprocessor::new(config(clang))
                    .preprocess_files(&paths)
                    .unwrap();
                let mut command = Command::new(&compiler);
                command.args(["-E", "-P", "-x", "c"]);
                for path in &paths[..paths.len() - 1] {
                    command.arg("-include").arg(path);
                }
                let native = command.arg(paths.last().unwrap()).output().unwrap();
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&native),
                    Ok(true),
                    "{command:?}: {}",
                    String::from_utf8_lossy(&native.stderr)
                );
                assert_eq!(
                    compact(&actual.source),
                    compact(&String::from_utf8_lossy(&native.stdout)),
                    "{command:?}"
                );
            }
        }
    }
}

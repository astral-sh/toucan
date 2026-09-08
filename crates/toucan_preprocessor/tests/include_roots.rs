use std::path::{Path, PathBuf};

use toucan_preprocessor::{Config, Preprocessor};

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "toucan-include-roots-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn setup() -> (Files, PathBuf, PathBuf, PathBuf) {
    let dir = Files::new();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    std::fs::write(a.join("chain.h"), "int from_a;\n#include_next <chain.h>\n").unwrap();
    std::fs::write(b.join("chain.h"), "int from_b;\n").unwrap();
    let main = dir.path().join("main.h");
    std::fs::write(&main, "#include <chain.h>\n").unwrap();
    (dir, a, b, main)
}

#[test]
fn repeated_regular_roots_do_not_reenter_a_header_with_include_next() {
    let (_dir, a, b, main) = setup();
    let config = Config {
        include_dirs: vec![a.clone(), a, b],
        ..Default::default()
    };
    let source = Preprocessor::new(config).preprocess(&main).unwrap().source;
    assert_eq!(source.matches("from_a").count(), 1, "{source}");
    assert_eq!(source.matches("from_b").count(), 1, "{source}");
}

#[test]
fn system_duplicates_follow_other_regular_roots() {
    let (_dir, a, b, main) = setup();
    for regular in [
        vec![a.clone(), b.clone()],
        vec![a.clone(), b.clone(), a.clone()],
    ] {
        let config = Config {
            include_dirs: regular,
            system_include_dirs: vec![a.clone()],
            ..Default::default()
        };
        let source = Preprocessor::new(config).preprocess(&main).unwrap().source;
        assert!(!source.contains("from_a"), "{source}");
        assert_eq!(source.matches("from_b").count(), 1, "{source}");
    }
}

#[test]
fn system_status_is_inherited_and_not_persisted_across_later_user_includes() {
    let (_dir, a, b, main) = setup();
    std::fs::write(
        a.join("system.h"),
        "extern int from_system;\n#include <user.h>\n",
    )
    .unwrap();
    std::fs::write(b.join("user.h"), "extern int from_user;\n").unwrap();
    std::fs::write(&main, "#include <system.h>\n#include <user.h>\n").unwrap();
    let config = Config {
        include_dirs: vec![b],
        system_include_dirs: vec![a],
        record_file_origins: true,
        ..Default::default()
    };
    let output = Preprocessor::new(config).preprocess(&main).unwrap();
    let origins = output.file_origins().unwrap();
    let system = output.source.find("from_system").unwrap();
    assert_eq!(origins.source_is_system_include(system), Some(true));
    let users: Vec<_> = output
        .source
        .match_indices("from_user")
        .map(|(offset, _)| origins.source_is_system_include(offset))
        .collect();
    assert_eq!(users, [Some(true), Some(false)]);
    assert!(
        origins
            .mappings()
            .iter()
            .any(|mapping| mapping.is_system_include())
    );
}

#[cfg(unix)]
#[test]
fn symlink_directory_identity_and_reuse_follow_each_new_run() {
    let (dir, a, b, main) = setup();
    let alias = dir.path().join("alias");
    let c = dir.path().join("c");
    std::fs::create_dir(&c).unwrap();
    std::fs::write(c.join("chain.h"), "int from_c;\n").unwrap();
    std::os::unix::fs::symlink(&a, &alias).unwrap();
    let config = Config {
        include_dirs: vec![a.clone(), alias.clone(), b],
        record_file_origins: true,
        ..Default::default()
    };
    let mut preprocessor = Preprocessor::new(config);
    let first = preprocessor.preprocess(&main).unwrap();
    assert_eq!(first.source.matches("from_a").count(), 1);
    assert!(first.source.contains("from_b"));
    std::fs::remove_file(&alias).unwrap();
    std::os::unix::fs::symlink(&c, &alias).unwrap();
    let second = preprocessor.preprocess(&main).unwrap();
    assert!(second.source.contains("from_c"));
    assert!(!second.source.contains("from_b"));
    let offset = second.source.find("from_c").unwrap();
    assert_eq!(
        second.file_origins().unwrap().source_name(offset),
        Some(alias.join("chain.h").as_path())
    );
    assert_eq!(
        second.file_origins().unwrap().source_file(offset),
        Some(std::fs::canonicalize(c.join("chain.h")).unwrap().as_path())
    );
}

#[test]
fn root_count_is_bounded_before_resolution_and_filesystem_disabled_stays_usable() {
    let mut preprocessor = Preprocessor::new(Config {
        include_dirs: vec![PathBuf::new(); 65_537],
        ..Default::default()
    });
    assert!(
        preprocessor
            .preprocess_str(Path::new("limit.h"), "int value;")
            .unwrap_err()
            .message
            .contains("65536-entry")
    );
    let mut preprocessor = Preprocessor::new(Config {
        include_dirs: vec![PathBuf::from("absent"); 100],
        system_include_dirs: vec![PathBuf::from("also-absent")],
        allow_filesystem: false,
        ..Default::default()
    });
    assert!(
        preprocessor
            .preprocess_str(Path::new("input.h"), "int value;")
            .unwrap()
            .source
            .contains("value")
    );
}

#[test]
#[ignore = "requires GCC and Clang"]
fn root_priority_and_include_next_queries_match_native_compilers() {
    let (_dir, a, b, main) = setup();
    std::fs::write(a.join("only.h"), "").unwrap();
    std::fs::write(
        a.join("chain.h"),
        "#if __has_include_next(<only.h>)\n#error duplicate root was searched again\n#endif\nint from_a;\n#include_next <chain.h>\n",
    )
    .unwrap();
    for clang in [false, true] {
        let compiler = std::env::var(if clang { "TOUCAN_CLANG" } else { "TOUCAN_GCC" })
            .unwrap_or_else(|_| if clang { "clang" } else { "gcc" }.into());
        for (regular, system) in [
            (vec![a.clone(), b.clone()], vec![]),
            (vec![a.clone(), a.clone(), b.clone()], vec![]),
            (vec![a.clone(), b.clone()], vec![a.clone()]),
            (vec![b.clone()], vec![a.clone(), a.clone()]),
        ] {
            let actual = Preprocessor::new(Config {
                include_dirs: regular.clone(),
                system_include_dirs: system.clone(),
                ..Default::default()
            })
            .preprocess(&main)
            .unwrap();
            let mut command = std::process::Command::new(&compiler);
            command.args(["-E", "-P", "-x", "c"]);
            for root in &regular {
                command.arg("-I").arg(root);
            }
            for root in &system {
                command.arg("-isystem").arg(root);
            }
            let native = command.arg(&main).output().unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&native),
                Ok(true),
                "{command:?}: {}",
                String::from_utf8_lossy(&native.stderr)
            );
            let compact = |source: &str| source.split_whitespace().collect::<String>();
            assert_eq!(
                compact(&actual.source),
                compact(&String::from_utf8_lossy(&native.stdout)),
                "{command:?}"
            );
        }
    }
}

//! Shared setup for Toucan's regression benchmarks.

use std::path::PathBuf;

use serde::Deserialize;
use toucan_bindgen::{Builder, Formatter};

/// Select the instrumented harness only when built by `cargo codspeed`.
pub mod criterion {
    #[cfg(codspeed)]
    pub use codspeed_criterion_compat::*;
    #[cfg(not(codspeed))]
    pub use criterion::*;
}

#[derive(Deserialize)]
struct Corpus {
    schema_version: u32,
    projects: Vec<Project>,
}

#[derive(Deserialize)]
struct Project {
    name: String,
    version: String,
    sha256: String,
    #[serde(default)]
    header: String,
    #[serde(default)]
    include_dirs: Vec<String>,
}

/// A prepared public-header workload and an entry point to check before timing.
pub struct Workload {
    pub name: String,
    pub builder: Builder,
    pub entry_point: &'static str,
}

/// Load all four pinned corpus projects, failing if preparation is incomplete or stale.
pub fn workloads() -> Vec<Workload> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(
        std::env::var_os("TOUCAN_BENCH_CORPUS")
            .unwrap_or_else(|| "corpus/cache/prepared.json".into()),
    );
    let source = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "Cannot read {}: {error}. Run `python3 scripts/prepare_corpus.py` first.",
            path.display()
        )
    });
    let mut prepared: Corpus = serde_json::from_str(&source).expect("invalid prepared corpus");
    let manifest: Corpus = serde_json::from_str(include_str!("../../../corpus/manifest.json"))
        .expect("invalid corpus manifest");
    assert_eq!(
        prepared.schema_version, 1,
        "unsupported prepared corpus schema"
    );
    assert_eq!(
        prepared.projects.len(),
        manifest.projects.len(),
        "incomplete corpus"
    );
    let clang_include = std::env::var("TOUCAN_BENCH_CLANG_INCLUDE")
        .unwrap_or_else(|_| "/usr/lib/llvm-18/lib/clang/18/include".into());
    assert!(
        PathBuf::from(&clang_include).join("stddef.h").is_file(),
        "Install clang-18 or set TOUCAN_BENCH_CLANG_INCLUDE to its resource include directory"
    );

    manifest
        .projects
        .into_iter()
        .map(|expected| {
            let index = prepared
                .projects
                .iter()
                .position(|project| project.name == expected.name)
                .unwrap_or_else(|| panic!("Missing corpus project: {}", expected.name));
            let project = prepared.projects.remove(index);
            assert_eq!(project.version, expected.version, "stale corpus version");
            assert_eq!(project.sha256, expected.sha256, "stale corpus archive");
            assert!(
                PathBuf::from(&project.header).is_file(),
                "Missing header: {}",
                project.header
            );
            assert!(!project.include_dirs.is_empty(), "missing project includes");

            let mut builder = Builder::default()
                .header(project.header)
                .clang_args([
                    "--target=x86_64-unknown-linux-gnu",
                    "--sysroot=/",
                    "-x",
                    "c",
                    "-std=c11",
                ])
                .formatter(Formatter::None)
                .use_core()
                .layout_tests(false);
            for include in project.include_dirs {
                assert!(
                    PathBuf::from(&include).is_dir(),
                    "Missing include directory: {include}"
                );
                // Select every reached project header, including zconf.h and
                // the public headers included by libgit2's git2.h umbrella.
                builder = builder
                    .allowlist_file(format!("{}/.*", regex::escape(&include)))
                    .clang_args(["-I", &include]);
            }
            builder = builder.clang_args(["-isystem", &clang_include]);
            let entry_point = match project.name.as_str() {
                "zlib" => "pub fn deflate(",
                "sqlite" => "pub fn sqlite3_open(",
                "zstd" => "pub fn ZSTD_compress(",
                "libgit2" => "pub fn git_libgit2_init(",
                name => panic!("Add an entry-point check for new corpus project {name}"),
            };
            Workload {
                name: format!("{}-{}", project.name, project.version),
                builder,
                entry_point,
            }
        })
        .collect()
}

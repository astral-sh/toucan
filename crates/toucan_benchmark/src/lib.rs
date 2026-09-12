//! Shared setup for Toucan's regression benchmarks.

use std::path::PathBuf;
use std::process::Command;

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

#[derive(Clone, Deserialize)]
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

/// A C source preprocessed once, ready for each parser to consume unchanged.
pub struct ParserWorkload {
    pub name: String,
    pub source: String,
}

fn prepared_projects() -> Vec<Project> {
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
            for include in &project.include_dirs {
                assert!(
                    PathBuf::from(include).is_dir(),
                    "Missing include directory: {include}"
                );
            }
            project
        })
        .collect()
}

fn clang_include() -> String {
    let include = std::env::var("TOUCAN_BENCH_CLANG_INCLUDE")
        .unwrap_or_else(|_| "/usr/lib/llvm-18/lib/clang/18/include".into());
    assert!(
        PathBuf::from(&include).join("stddef.h").is_file(),
        "Install clang-18 or set TOUCAN_BENCH_CLANG_INCLUDE to its resource include directory"
    );
    include
}

/// Load all four pinned corpus projects, failing if preparation is incomplete or stale.
pub fn workloads() -> Vec<Workload> {
    let projects = prepared_projects();
    let clang_include = clang_include();
    projects
        .into_iter()
        .map(|project| {
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

/// Preprocess pinned headers and a source file outside timing, retaining exact inputs.
///
/// `TOUCAN_BENCH_CC` selects the preprocessor (default: `gcc`). Inputs and
/// their provenance are saved under `benchmark-results/parser-inputs`, or the
/// directory selected by `TOUCAN_BENCH_PARSER_INPUTS`.
pub fn parser_workloads() -> Vec<ParserWorkload> {
    let mut projects = prepared_projects();
    let mut adler32 = projects
        .iter()
        .find(|project| project.name == "zlib")
        .expect("missing zlib corpus project")
        .clone();
    let source = PathBuf::from(&adler32.header).with_file_name("adler32.c");
    assert!(source.is_file(), "Missing source: {}", source.display());
    adler32.name.push_str("-adler32");
    adler32.header = source.to_string_lossy().into_owned();
    projects.push(adler32);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join(
        std::env::var_os("TOUCAN_BENCH_PARSER_INPUTS")
            .unwrap_or_else(|| "benchmark-results/parser-inputs".into()),
    );
    let compiler = std::env::var("TOUCAN_BENCH_CC").unwrap_or_else(|_| "gcc".into());
    let version = Command::new(&compiler)
        .arg("--version")
        .output()
        .unwrap_or_else(|error| panic!("Cannot run {compiler}: {error}"));
    assert!(
        version.status.success(),
        "{compiler} --version failed: {}",
        String::from_utf8_lossy(&version.stderr)
    );
    let target = Command::new(&compiler)
        .arg("-dumpmachine")
        .output()
        .unwrap_or_else(|error| panic!("Cannot determine {compiler} target: {error}"));
    assert!(
        target.status.success(),
        "{compiler} -dumpmachine failed: {}",
        String::from_utf8_lossy(&target.stderr)
    );
    let target = String::from_utf8_lossy(&target.stdout);
    assert_eq!(
        target.trim(),
        "x86_64-linux-gnu",
        "Parser benchmarks require GCC targeting x86_64-linux-gnu"
    );
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("Cannot create {}: {error}", directory.display()));

    let mut inputs = Vec::new();
    let workloads = projects
        .into_iter()
        .map(|project| {
            let name = format!("{}-{}", project.name, project.version);
            let mut args: Vec<String> = ["--sysroot=/", "-x", "c", "-std=gnu11", "-E", "-P"]
                .into_iter()
                .map(String::from)
                .collect();
            for include in &project.include_dirs {
                args.extend(["-I".into(), include.clone()]);
            }
            args.push(project.header.clone());
            let output = Command::new(&compiler)
                .args(&args)
                .current_dir(&root)
                .output()
                .unwrap_or_else(|error| panic!("Cannot preprocess {name}: {error}"));
            assert!(
                output.status.success(),
                "Cannot preprocess {name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let source = String::from_utf8(output.stdout)
                .unwrap_or_else(|error| panic!("Preprocessed {name} is not UTF-8: {error}"));
            assert!(!source.is_empty(), "Preprocessed {name} is empty");
            let path = directory.join(format!("{name}.i"));
            std::fs::write(&path, &source)
                .unwrap_or_else(|error| panic!("Cannot write {}: {error}", path.display()));
            let command: Vec<_> = std::iter::once(&compiler).chain(&args).collect();
            inputs.push(serde_json::json!({
                "name": name,
                "source": project.header,
                "archive_sha256": project.sha256,
                "command": command,
                "input": path,
                "bytes": source.len(),
                "stderr": String::from_utf8_lossy(&output.stderr),
            }));
            ParserWorkload { name, source }
        })
        .collect();
    let manifest = serde_json::json!({
        "schema_version": 1,
        "preprocessor": compiler,
        "preprocessor_version": String::from_utf8_lossy(&version.stdout),
        "target": target.trim(),
        "cwd": root,
        "inputs": inputs,
    });
    let path = directory.join("manifest.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&manifest).expect("cannot serialize parser input manifest"),
    )
    .unwrap_or_else(|error| panic!("Cannot write {}: {error}", path.display()));
    workloads
}

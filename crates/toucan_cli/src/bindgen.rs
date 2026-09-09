//! The supported C subset of bindgen-cli's executable interface.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use toucan_bindgen::{Builder, Formatter, MacroTypeVariation, RustTarget};

#[cfg(all(feature = "performance-allocator", unix, not(target_os = "openbsd")))]
#[global_allocator]
static ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(all(feature = "performance-allocator", windows))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser)]
#[command(
    name = "bindgen",
    version = concat!(env!("CARGO_PKG_VERSION"), " (Toucan)"),
    about = "Generate C bindings with Toucan, without libclang"
)]
struct Cli {
    /// Prefix native function and variable symbols; keep their Rust names.
    #[arg(long)]
    prefix_link_name: Option<String>,
    /// Select declarations originating in matching C headers.
    #[arg(long)]
    allowlist_file: Vec<String>,
    /// Represent a matching enum as a Rust enum.
    #[arg(long)]
    rustified_enum: Vec<String>,
    /// Use signed or unsigned storage for integer macro constants.
    #[arg(long)]
    default_macro_constant_type: Option<MacroTypeVariation>,
    /// Derive Default for representations where zero is a valid Rust value.
    #[arg(long)]
    with_derive_default: bool,
    #[arg(long)]
    with_derive_partialeq: bool,
    #[arg(long)]
    with_derive_eq: bool,
    /// Insert Rust verbatim before the generated declarations.
    #[arg(long)]
    raw_line: Vec<String>,
    /// C categories: functions,types,vars,methods,constructors,destructors.
    #[arg(long)]
    generate: Option<String>,
    /// First input C header.
    header: PathBuf,
    /// Minimum Rust version for generated declarations.
    #[arg(long)]
    rust_target: Option<RustTarget>,
    /// Output Rust file. Without this flag, write to stdout.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Format generated declarations with rustfmt, or leave them unchanged.
    #[arg(long)]
    formatter: Option<Formatter>,
    /// C frontend options, separated from bindgen options by `--`.
    #[arg(last = true, allow_hyphen_values = true)]
    clang_args: Vec<String>,
}

impl Cli {
    fn generate(self) -> Result<()> {
        if let Some(categories) = &self.generate {
            anyhow::ensure!(
                matches!(
                    categories.as_str(),
                    "functions,types,vars"
                        | "functions,types,vars,methods,constructors,destructors"
                ),
                "unsupported --generate categories `{categories}`; C requires functions,types,vars (methods,constructors,destructors have no C declarations)"
            );
        }
        let mut builder = Builder::default()
            .header(self.header.to_string_lossy())
            .clang_args(self.clang_args)
            .derive_default(self.with_derive_default)
            .derive_partialeq(self.with_derive_partialeq || self.with_derive_eq)
            .derive_eq(self.with_derive_eq);
        for pattern in self.allowlist_file {
            builder = builder.allowlist_file(pattern);
        }
        for pattern in self.rustified_enum {
            builder = builder.rustified_enum(pattern);
        }
        for raw in self.raw_line {
            builder = builder.raw_line(raw);
        }
        if let Some(prefix) = self.prefix_link_name {
            // AWS-LC's FIPS module exports its integrity check without the
            // ordinary symbol prefix. Its generated symbol list is the source
            // of truth for whether this exception applies to the pinned input.
            if prefix.starts_with("aws_lc_fips_") && fips_integrity_is_unprefixed(&self.header)? {
                builder = builder
                    .link_name_override("BORINGSSL_integrity_test", "BORINGSSL_integrity_test");
            }
            builder = builder.prefix_link_name(prefix);
        }
        if let Some(variation) = self.default_macro_constant_type {
            builder = builder.default_macro_constant_type(variation);
        }
        if let Some(target) = self.rust_target {
            builder = builder.rust_target(target);
        }
        if let Some(formatter) = self.formatter {
            builder = builder.formatter(formatter);
        }
        let bindings = builder.generate()?;
        if let Some(path) = self.output {
            bindings
                .write_to_file(&path)
                .with_context(|| format!("could not write {}", path.display()))?;
        } else {
            bindings.write(Box::new(io::stdout().lock()))?;
        }
        Ok(())
    }
}

fn fips_integrity_is_unprefixed(header: &Path) -> Result<bool> {
    anyhow::ensure!(
        header
            .file_name()
            .is_some_and(|name| name == "rust_wrapper.h")
            && header
                .parent()
                .is_some_and(|parent| parent.ends_with("include")),
        "AWS-LC FIPS symbol prefix requires its include/rust_wrapper.h"
    );
    let source = header
        .parent()
        .and_then(Path::parent)
        .context("missing AWS-LC FIPS source")?;
    let manifest = fs::read_to_string(source.join("Cargo.toml"))
        .with_context(|| format!("cannot check AWS-LC FIPS source at {}", source.display()))?;
    anyhow::ensure!(
        manifest.contains("name = \"aws-lc-fips-sys\""),
        "symbol prefix requires aws-lc-fips-sys source"
    );
    let prefix_header = source.join("generated-include/openssl/boringssl_prefix_symbols.h");
    let symbols = fs::read_to_string(&prefix_header)
        .with_context(|| format!("cannot check FIPS symbols at {}", prefix_header.display()))?;
    anyhow::ensure!(
        symbols
            .lines()
            .any(|line| line.starts_with("#define BORINGSSL_self_test ")),
        "FIPS symbol list does not include BORINGSSL_self_test"
    );
    Ok(!symbols
        .lines()
        .any(|line| line.starts_with("#define BORINGSSL_integrity_test ")))
}

fn main() -> ExitCode {
    match Cli::parse().generate() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe)
            {
                return ExitCode::SUCCESS;
            }
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fips_integrity_is_unprefixed;
    use std::fs;

    #[test]
    fn fips_integrity_follows_the_upstream_symbol_list() {
        let source = tempfile::tempdir().unwrap();
        let include = source.path().join("include");
        let prefix = source.path().join("generated-include/openssl");
        fs::create_dir(&include).unwrap();
        fs::create_dir_all(&prefix).unwrap();
        fs::write(
            source.path().join("Cargo.toml"),
            "name = \"aws-lc-fips-sys\"\n",
        )
        .unwrap();
        let header = include.join("rust_wrapper.h");
        fs::write(
            prefix.join("boringssl_prefix_symbols.h"),
            "#define BORINGSSL_self_test x\n",
        )
        .unwrap();
        assert!(fips_integrity_is_unprefixed(&header).unwrap());
        fs::write(
            prefix.join("boringssl_prefix_symbols.h"),
            "#define BORINGSSL_self_test x\n#define BORINGSSL_integrity_test y\n",
        )
        .unwrap();
        assert!(!fips_integrity_is_unprefixed(&header).unwrap());
        fs::remove_file(prefix.join("boringssl_prefix_symbols.h")).unwrap();
        assert!(fips_integrity_is_unprefixed(&header).is_err());
    }
}

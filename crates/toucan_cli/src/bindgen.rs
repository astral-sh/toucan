//! The supported C subset of bindgen-cli's executable interface.

use std::io;
use std::path::PathBuf;
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

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use toucan::{BindingOptions, Config, MacroType, RustTarget, Target};

#[cfg(all(feature = "performance-allocator", unix, not(target_os = "openbsd")))]
#[global_allocator]
static ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(all(feature = "performance-allocator", windows))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser)]
#[command(
    name = "toucan",
    version,
    about = "Analyze C headers and generate Rust bindings without libclang"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate Rust declarations and supported macro constants.
    Bindgen {
        #[command(flatten)]
        input: Input,
        /// Write bindings to a file instead of standard output.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Select an exact C name or a prefix ending in '*'. Repeat to add names.
        #[arg(long)]
        allowlist: Vec<String>,
        /// Write dependency, timing, and skipped-macro information as JSON.
        #[arg(long)]
        report: Option<PathBuf>,
        /// Fail if a selected macro cannot be emitted as a constant.
        #[arg(long)]
        deny_skipped_macros: bool,
        /// Emit Rust enums with named variants; undeclared values are invalid Rust values.
        #[arg(long)]
        rustified_enums: bool,
        /// Represent a pointer-sized unsigned size_t typedef as Rust usize.
        #[arg(long)]
        size_t_is_usize: bool,
        /// Namespace generated helper types and tests when including multiple binding files.
        #[arg(long)]
        helper_namespace: Option<String>,
        /// Preserve C macro types, or infer unsigned types for nonnegative values.
        #[arg(long, value_parser = ["c", "unsigned"], default_value = "c")]
        macro_type: String,
        /// Override an integer macro policy: NAME=c|unsigned, or PREFIX*=c|unsigned.
        #[arg(long)]
        macro_type_for: Vec<String>,
        /// Omit a function by exact name or prefix ending in '*'. Repeat to add names.
        #[arg(long = "blocklist-function")]
        blocklist_functions: Vec<String>,
        /// Append caller-provided Rust from a UTF-8 file, without parsing or ABI checks.
        #[arg(long)]
        raw_lines_file: Vec<PathBuf>,
        /// Emit byte string macros as CStr; reject interior NUL bytes.
        #[arg(long)]
        generate_cstr: bool,
        /// Minimum Rust version for generated declarations (1.64 or newer).
        #[arg(long, default_value_t = RustTarget::default())]
        rust_target: RustTarget,
    },
    /// Print native preprocessor output without invoking a C compiler.
    Preprocess {
        #[command(flatten)]
        input: Input,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Write the analyzed declarations and types as versioned JSON.
    Inspect {
        #[command(flatten)]
        input: Input,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Include checked expressions, bodies, initializers, and source references.
        #[arg(long)]
        checked_code: bool,
    },
    /// Check declarations, initializers, and function bodies.
    Check {
        #[command(flatten)]
        input: Input,
    },
}

#[derive(Args)]
struct Input {
    /// Header to read. Include paths and definitions apply to the entire translation unit.
    header: PathBuf,
    /// C ABI target. Defaults to the host on supported platforms.
    #[arg(long)]
    target: Option<String>,
    /// Add an include search directory. Search order follows the argument order.
    #[arg(short = 'I', long = "include-dir")]
    include_dirs: Vec<PathBuf>,
    /// Define NAME or NAME=VALUE before preprocessing.
    #[arg(short = 'D', long = "define")]
    defines: Vec<String>,
    /// Remove a predefined macro.
    #[arg(short = 'U', long = "undefine")]
    undefines: Vec<String>,
    /// Root of target system headers; adds usr/include and its target subdirectory.
    #[arg(long)]
    sysroot: Option<PathBuf>,
    /// Limit tokens processed during preprocessing.
    #[arg(long, default_value_t = 2_000_000)]
    max_tokens: usize,
}

impl Input {
    fn config(&self) -> Result<Config> {
        let target = match &self.target {
            Some(target) => Target::parse(target)?,
            None => host_target()?,
        };
        let mut config = Config::new(target);
        config.preprocessor.timestamp = match std::env::var("SOURCE_DATE_EPOCH") {
            Ok(value) => value.parse().context("invalid SOURCE_DATE_EPOCH")?,
            Err(std::env::VarError::NotPresent) => {
                let seconds = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .context("system clock is before the Unix epoch")?
                    .as_secs();
                toucan::PreprocessingTimestamp::from_unix_seconds(seconds)?
            }
            Err(error) => return Err(error).context("invalid SOURCE_DATE_EPOCH"),
        };
        config.preprocessor.include_dirs = self.include_dirs.clone();
        config.preprocessor.max_tokens = self.max_tokens;
        if let Some(sysroot) = &self.sysroot {
            let include = sysroot.join("usr/include");
            let multiarch = match target.triple() {
                "x86_64-unknown-linux-gnu" => Some("x86_64-linux-gnu"),
                "aarch64-unknown-linux-gnu" => Some("aarch64-linux-gnu"),
                _ => None,
            };
            if let Some(multiarch) = multiarch {
                config
                    .preprocessor
                    .include_dirs
                    .push(include.join(multiarch));
            }
            config.preprocessor.include_dirs.push(include);
        }
        for define in &self.defines {
            let (name, value) = define.split_once('=').unwrap_or((define, "1"));
            anyhow::ensure!(!name.is_empty(), "macro name must not be empty");
            config
                .preprocessor
                .defines
                .insert(name.into(), value.into());
        }
        for name in &self.undefines {
            config.preprocessor.defines.remove(name);
        }
        Ok(config)
    }
}

fn host_target() -> Result<Target> {
    Target::ALL
        .into_iter()
        .find(|target| match target.triple() {
            "x86_64-unknown-linux-gnu" => cfg!(all(
                target_arch = "x86_64",
                target_os = "linux",
                target_env = "gnu"
            )),
            "aarch64-unknown-linux-gnu" => cfg!(all(
                target_arch = "aarch64",
                target_os = "linux",
                target_env = "gnu"
            )),
            "x86_64-apple-darwin" => cfg!(all(target_arch = "x86_64", target_os = "macos")),
            "aarch64-apple-darwin" => cfg!(all(target_arch = "aarch64", target_os = "macos")),
            "x86_64-pc-windows-msvc" => cfg!(all(
                target_arch = "x86_64",
                target_os = "windows",
                target_env = "msvc"
            )),
            _ => false,
        })
        .context("the host target is unsupported; supply --target explicitly")
}

fn write_output(path: Option<PathBuf>, source: &str) -> Result<()> {
    if let Some(path) = path {
        std::fs::write(&path, source)
            .with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        io::stdout().lock().write_all(source.as_bytes())?;
    }
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Preprocess { input, output } => {
            let config = input.config()?;
            let preprocessed =
                toucan::Preprocessor::new(config.preprocessor).preprocess(&input.header)?;
            write_output(output, &preprocessed.source)
        }
        Command::Inspect {
            input,
            output,
            checked_code,
        } => {
            let mut config = input.config()?;
            config.analysis.retain_code = checked_code;
            let compilation = toucan::parse_file(&input.header, &config)?;
            let manifest = if checked_code {
                let preprocessed = compilation.preprocessed();
                let mappings: Vec<_> = preprocessed.mappings.iter().map(|mapping| {
                    let origin = &mapping.origin;
                    let kind = match origin.kind {
                        toucan::OriginKind::Token => "token",
                        toucan::OriginKind::MacroInvocation => "macro_invocation",
                        toucan::OriginKind::Directive => "directive",
                    };
                    serde_json::json!({
                        "generated": mapping.generated,
                        "origin": {"path": origin.path.as_ref(), "line": origin.line, "column": origin.column, "kind": kind},
                    })
                }).collect();
                serde_json::json!({
                    "schema_version": 2,
                    "translation_unit": compilation.unit(),
                    "checked_code": compilation.checked(),
                    "preprocessed": {"source": preprocessed.source, "mappings": mappings},
                })
            } else {
                serde_json::json!({ "schema_version": 1, "translation_unit": compilation.unit() })
            };
            write_output(output, &(serde_json::to_string_pretty(&manifest)? + "\n"))
        }
        Command::Check { input } => {
            let compilation = toucan::parse_file(&input.header, &input.config()?)?;
            eprintln!(
                "Analyzed {} declarations for {}",
                compilation.unit().declarations.len(),
                compilation.unit().target
            );
            Ok(())
        }
        Command::Bindgen {
            input,
            output,
            allowlist,
            report,
            deny_skipped_macros,
            rustified_enums,
            size_t_is_usize,
            helper_namespace,
            macro_type,
            macro_type_for,
            blocklist_functions,
            raw_lines_file,
            generate_cstr,
            rust_target,
        } => {
            let mut macro_type_overrides = std::collections::BTreeMap::new();
            for item in macro_type_for {
                let (name, policy) = item
                    .split_once('=')
                    .context("macro policy must be NAME=c or NAME=unsigned")?;
                anyhow::ensure!(!name.is_empty(), "macro policy name must not be empty");
                let policy = match policy {
                    "c" => MacroType::C,
                    "unsigned" => MacroType::Unsigned,
                    _ => anyhow::bail!("macro policy must be c or unsigned"),
                };
                macro_type_overrides.insert(name.to_owned(), policy);
            }
            let raw_lines = raw_lines_file
                .iter()
                .map(std::fs::read_to_string)
                .collect::<Result<Vec<_>, _>>()?;
            let compilation = toucan::parse_file(&input.header, &input.config()?)?;
            let (source, metadata) = compilation.bindings(&BindingOptions {
                allowlist,
                rustified_enums,
                size_t_is_usize,
                helper_namespace,
                macro_type_overrides,
                blocklist_functions,
                raw_lines,
                generate_cstr,
                rust_target,
                macro_type: if macro_type == "unsigned" {
                    MacroType::Unsigned
                } else {
                    MacroType::C
                },
            })?;
            if let Some(report) = report {
                write_output(
                    Some(report),
                    &(serde_json::to_string_pretty(&metadata)? + "\n"),
                )?;
            }
            anyhow::ensure!(
                !deny_skipped_macros || metadata.skipped_macros.is_empty(),
                "{} selected macros cannot be emitted; use --report to inspect them",
                metadata.skipped_macros.len()
            );
            write_output(output, &source)?;
            if !metadata.skipped_macros.is_empty() {
                eprintln!(
                    "Skipped {} macros that are not supported constants; use --report for details",
                    metadata.skipped_macros.len()
                );
            }
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe)
            {
                return ExitCode::SUCCESS;
            }
            if let Some(error) = error.downcast_ref::<toucan::Error>() {
                eprintln!("error: {error}");
            } else {
                eprintln!("error: {error:#}");
            }
            ExitCode::FAILURE
        }
    }
}

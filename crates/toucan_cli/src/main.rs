mod inspection;

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{ArgMatches, Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use toucan::{
    BindingOptions, Compiler, CompilerProfile, Config, LanguageMode, MacroType, RustTarget, Target,
};

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
        input: Box<Input>,
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
        /// Use caller-supplied Rust definitions for matching C types.
        #[arg(long = "blocklist-type")]
        blocklist_types: Vec<String>,
        /// Append caller-provided Rust from a UTF-8 file, without parsing or ABI checks.
        #[arg(long)]
        raw_lines_file: Vec<PathBuf>,
        /// Link selected dllimport symbols: NAME=LIBRARY or PREFIX*=LIBRARY. Repeat for multiple DLLs.
        #[arg(long = "dll-import-library", value_name = "PATTERN=LIBRARY")]
        dll_import_libraries: Vec<String>,
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
        /// Limit retained graph nodes; requires --checked-code.
        #[arg(long, requires = "checked_code")]
        max_retained_nodes: Option<usize>,
        /// Limit retained graph edges; requires --checked-code.
        #[arg(long, requires = "checked_code")]
        max_retained_edges: Option<usize>,
        /// Limit owned retained payload bytes; requires --checked-code.
        #[arg(long, requires = "checked_code")]
        max_retained_bytes: Option<usize>,
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
    /// Compiler semantics and header macros: gcc or clang. Defaults to the target's compiler.
    #[arg(long)]
    compiler: Option<Compiler>,
    /// C keyword and preprocessing defaults; this does not enable pedantic diagnostics.
    #[arg(long = "std", default_value_t = LanguageMode::Gnu11, overrides_with = "language_mode")]
    language_mode: LanguageMode,
    /// Override trigraph replacement; use --trigraphs=false to disable it.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true", overrides_with = "trigraphs")]
    trigraphs: Option<bool>,
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
    fn config(&self, arguments: &ArgMatches, preprocessing: bool) -> Result<Config> {
        let target = match &self.target {
            Some(target) => Target::parse(target)?,
            None => host_target()?,
        };
        let profile = match self.compiler {
            Some(compiler) => CompilerProfile::new(target, compiler)?,
            None => CompilerProfile::default_for(target),
        };
        let mut config = Config::with_profile(profile.with_language_mode(self.language_mode));
        if preprocessing {
            config.preprocessor.line_comments = match config.preprocessor.line_comments {
                toucan::LineComments::GnuC90 => toucan::LineComments::GnuC90Preprocessing,
                toucan::LineComments::ClangC90 => toucan::LineComments::ClangC90Preprocessing,
                mode => mode,
            };
        }
        // As in compiler drivers, a later -std resets earlier trigraph flags.
        if let Some(enabled) = self.trigraphs {
            let standard = arguments
                .indices_of("language_mode")
                .and_then(|mut i| i.next_back())
                .unwrap_or(0);
            let explicit = arguments
                .indices_of("trigraphs")
                .and_then(|mut i| i.next_back())
                .unwrap_or(0);
            if arguments.value_source("language_mode")
                != Some(clap::parser::ValueSource::CommandLine)
                || explicit > standard
            {
                config.preprocessor.trigraphs = enabled;
            }
        }
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
                "i686-unknown-linux-gnu" => Some("i386-linux-gnu"),
                "aarch64-unknown-linux-gnu" => Some("aarch64-linux-gnu"),
                "x86_64-unknown-linux-musl" => Some("x86_64-linux-musl"),
                "aarch64-unknown-linux-musl" => Some("aarch64-linux-musl"),
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
        let mut defines = arguments
            .indices_of("defines")
            .into_iter()
            .flatten()
            .zip(&self.defines)
            .peekable();
        let mut undefines = arguments
            .indices_of("undefines")
            .into_iter()
            .flatten()
            .zip(&self.undefines)
            .peekable();
        let mut macros = (config.preprocessor.line_comments == toucan::LineComments::ClangC90)
            .then(|| toucan::CommandLineMacroNormalizer::new(&config.preprocessor));
        while defines.peek().is_some() || undefines.peek().is_some() {
            let define_next = match (defines.peek(), undefines.peek()) {
                (Some((d, _)), Some((u, _))) => d < u,
                (Some(_), None) => true,
                _ => false,
            };
            if define_next {
                let (_, define) = defines.next().expect("peeked definition");
                let (name, value) = define.split_once('=').unwrap_or((define, "1"));
                anyhow::ensure!(!name.is_empty(), "macro name must not be empty");
                let prepared = macros
                    .as_mut()
                    .map(|macros| macros.prepare(name, value))
                    .transpose()
                    .map_err(anyhow::Error::msg)?;
                let (name, value) = prepared.as_ref().map_or((name, value), |(name, value)| {
                    (name.as_str(), value.as_str())
                });
                let identifier = name.split('(').next().unwrap_or(name);
                config
                    .preprocessor
                    .defines
                    .retain(|key, _| key.split('(').next() != Some(identifier));
                config
                    .preprocessor
                    .defines
                    .insert(name.into(), value.into());
            } else {
                let (_, name) = undefines.next().expect("peeked undefinition");
                config.preprocessor.undefine(name);
            }
        }
        if macros.is_some() {
            config.preprocessor.predefined_macro_mode = toucan::PredefinedMacroMode::Tokens;
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
            "i686-unknown-linux-gnu" => cfg!(all(
                target_arch = "x86",
                target_os = "linux",
                target_env = "gnu"
            )),
            "aarch64-unknown-linux-gnu" => cfg!(all(
                target_arch = "aarch64",
                target_os = "linux",
                target_env = "gnu"
            )),
            "x86_64-unknown-linux-musl" => cfg!(all(
                target_arch = "x86_64",
                target_os = "linux",
                target_env = "musl"
            )),
            "aarch64-unknown-linux-musl" => cfg!(all(
                target_arch = "aarch64",
                target_os = "linux",
                target_env = "musl"
            )),
            "x86_64-apple-darwin" => cfg!(all(target_arch = "x86_64", target_os = "macos")),
            "aarch64-apple-darwin" => cfg!(all(target_arch = "aarch64", target_os = "macos")),
            "x86_64-pc-windows-msvc" => cfg!(all(
                target_arch = "x86_64",
                target_os = "windows",
                target_env = "msvc"
            )),
            "aarch64-pc-windows-msvc" => cfg!(all(
                target_arch = "aarch64",
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

fn run(cli: Cli, arguments: &ArgMatches) -> Result<()> {
    let arguments = arguments.subcommand().context("missing command")?.1;
    match cli.command {
        Command::Preprocess { input, output } => {
            let config = input.config(arguments, true)?;
            let preprocessed =
                toucan::Preprocessor::new(config.preprocessor).preprocess(&input.header)?;
            write_output(output, &preprocessed.source)
        }
        Command::Inspect {
            input,
            output,
            checked_code,
            max_retained_nodes,
            max_retained_edges,
            max_retained_bytes,
        } => {
            let mut config = input.config(arguments, false)?;
            config.analysis.retain_code = checked_code;
            if let Some(nodes) = max_retained_nodes {
                config.analysis.limits.nodes = nodes;
            }
            if let Some(edges) = max_retained_edges {
                config.analysis.limits.edges = edges;
            }
            if let Some(bytes) = max_retained_bytes {
                config.analysis.limits.payload_bytes = bytes;
            }
            let compilation = toucan::parse_file(&input.header, &config)?;
            write_output(
                output,
                &(inspection::serialize(&compilation, checked_code)? + "\n"),
            )
        }
        Command::Check { input } => {
            let compilation = toucan::parse_file(&input.header, &input.config(arguments, false)?)?;
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
            blocklist_types,
            raw_lines_file,
            dll_import_libraries,
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
            let dll_import_libraries = dll_import_libraries
                .into_iter()
                .map(|rule| {
                    let (pattern, library) = rule
                        .split_once('=')
                        .context("DLL import library rule must be PATTERN=LIBRARY")?;
                    Ok((pattern.to_owned(), library.to_owned()))
                })
                .collect::<anyhow::Result<std::collections::BTreeMap<_, _>>>()?;
            let raw_lines = raw_lines_file
                .iter()
                .map(std::fs::read_to_string)
                .collect::<Result<Vec<_>, _>>()?;
            let compilation = toucan::parse_file(&input.header, &input.config(arguments, false)?)?;
            let (source, metadata) = compilation.bindings(&BindingOptions {
                type_dependencies: None,
                selection: None,
                object_bindings: Default::default(),
                additional_objects: Default::default(),
                documentation: None,
                generated_names: Default::default(),
                link_name_prefix: None,
                emit_function_definitions: false,
                exclude_inline_functions: false,
                nullable_function_typedefs: false,
                no_layout_tests: false,
                allowlist,
                rustified_enums,
                rustified_enum_patterns: Vec::new(),
                prepend_enum_name: false,
                enum_constant_style: Default::default(),
                derives: Default::default(),
                size_t_is_usize,
                helper_namespace,
                macro_type_overrides,
                blocklist_functions,
                blocklist_types,
                raw_lines,
                dll_import_libraries,
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
    let arguments = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&arguments).unwrap_or_else(|error| error.exit());
    match run(cli, &arguments) {
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

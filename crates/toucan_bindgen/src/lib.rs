//! A libclang-free builder for C binding build scripts.
//!
//! This crate implements a supported subset of bindgen's builder API. Unsupported
//! arguments and patterns produce errors during generation. It never starts a C
//! compiler, loads libclang, or discovers a compiler installation. Supply target
//! headers with include arguments and a sysroot. Cargo's `TARGET` selects the ABI
//! unless an explicit `--target` argument overrides it.

mod arguments;
pub mod callbacks;
mod documentation;
mod formatting;
mod macro_compat;
mod macro_projection;
mod macro_values;
mod objects;
mod parameter_dependencies;
mod selection;

use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub use formatting::Formatter;
pub use macro_projection::MacroTypeVariation;

use toucan::{BindingOptions, Compiler, CompilerProfile, Config, MacroType, Target};

/// Errors from builder configuration, preprocessing, checking, or emission.
#[derive(Debug, thiserror::Error)]
pub enum BindgenError {
    #[error("{0}")]
    Configuration(String),
    #[error(transparent)]
    Frontend(#[from] toucan::Error),
}

fn configuration(message: impl Into<String>) -> BindgenError {
    BindgenError::Configuration(message.into())
}

/// Frontend identity, using bindgen's version-reporting API shape.
#[derive(Debug)]
pub struct ClangVersion {
    /// No libclang version is present when Toucan performs the analysis.
    pub parsed: Option<(u32, u32)>,
    /// The Toucan package version and frontend identity.
    pub full: String,
}

/// Identify Toucan without loading or reporting a native libclang installation.
pub fn clang_version() -> ClangVersion {
    ClangVersion {
        parsed: None,
        full: format!("Toucan {} (no libclang)", env!("CARGO_PKG_VERSION")),
    }
}

/// Minimum Rust release for generated declarations, independently of this crate's MSRV.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RustTarget(toucan::RustTarget);

impl RustTarget {
    /// Select Rust 1.minor.patch. Patch releases share the same language features.
    pub fn stable(minor: u64, _patch: u64) -> Result<Self, BindgenError> {
        let minor = u16::try_from(minor)
            .map_err(|_| configuration("Rust target minor version exceeds 65535"))?;
        toucan::RustTarget::stable(minor)
            .map(Self)
            .map_err(|error| configuration(error.to_string()))
    }
}

impl Default for RustTarget {
    fn default() -> Self {
        Self(toucan::RustTarget::RUST_1_64)
    }
}

/// Binding generation configuration with build-script-compatible method names.
#[derive(Debug, Clone)]
pub struct Builder {
    headers: Vec<String>,
    arguments: Vec<String>,
    options: BindingOptions,
    allowlist_files: Vec<String>,
    allowlist_types: Vec<String>,
    allowlist_functions: Vec<String>,
    allowlist_vars: Vec<String>,
    callbacks: Vec<Rc<dyn callbacks::ParseCallbacks>>,
    formatting: formatting::Options,
    macro_type_variation: MacroTypeVariation,
    fit_macro_constants: bool,
    generate_comments: bool,
    error: Option<String>,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            headers: Vec::new(),
            arguments: Vec::new(),
            options: BindingOptions {
                prepend_enum_name: true,
                enum_constant_style: toucan::EnumConstantStyle::Bindgen,
                size_t_is_usize: true,
                macro_type: MacroType::Unsigned,
                rust_target: RustTarget::default().0,
                derives: toucan::DeriveOptions {
                    debug: Some(true),
                    ..Default::default()
                },
                ..BindingOptions::default()
            },
            allowlist_files: Vec::new(),
            allowlist_types: Vec::new(),
            allowlist_functions: Vec::new(),
            allowlist_vars: Vec::new(),
            callbacks: Vec::new(),
            error: None,
            formatting: formatting::Options::default(),
            macro_type_variation: MacroTypeVariation::default(),
            fit_macro_constants: false,
            generate_comments: true,
        }
    }
}

impl Builder {
    /// Choose signed or unsigned storage for parsed integer macro values.
    /// Negative values always use signed storage.
    pub fn default_macro_constant_type(mut self, variation: MacroTypeVariation) -> Self {
        self.macro_type_variation = variation;
        self
    }

    /// Permit 8- and 16-bit macro constants when their values fit. Disabled by default.
    pub fn fit_macro_constants(mut self, fit: bool) -> Self {
        self.fit_macro_constants = fit;
        self
    }

    /// Choose how generated declarations are formatted when written or displayed.
    pub fn formatter(mut self, formatter: Formatter) -> Self {
        self.formatting.formatter = formatter;
        self
    }

    /// Select rustfmt, or disable formatting, using bindgen's legacy option.
    #[deprecated(note = "Use `formatter` instead")]
    pub fn rustfmt_bindings(self, enabled: bool) -> Self {
        self.formatter(if enabled {
            Formatter::Rustfmt
        } else {
            Formatter::None
        })
    }

    /// Choose the rustfmt executable, overriding `RUSTFMT` and the default PATH lookup.
    pub fn with_rustfmt<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.formatting.path = Some(path.into());
        self
    }

    /// Set a rustfmt configuration file and enable rustfmt, including for `None`.
    pub fn rustfmt_configuration_file(mut self, path: Option<PathBuf>) -> Self {
        self.formatting.configuration = path;
        self.formatting.formatter = Formatter::Rustfmt;
        self
    }

    /// Add a header, in translation-unit order.
    pub fn header(mut self, header: impl Into<String>) -> Self {
        self.headers.push(header.into());
        self
    }

    /// Select declarations physically originating in matching headers. Patterns
    /// are Rust regular expressions anchored to the compiler-visible access name. Dependencies
    /// are included; logical `#line` filenames do not change file selection.
    pub fn allowlist_file(mut self, pattern: impl AsRef<str>) -> Self {
        self.allowlist_files.push(pattern.as_ref().into());
        self
    }

    /// Select types by fully anchored Rust regular expressions. Nested tags use
    /// their lexical names (for example, `Outer_Inner`); dependencies are included.
    /// All file and name allowlists are combined by union.
    pub fn allowlist_type(mut self, pattern: impl AsRef<str>) -> Self {
        self.allowlist_types.push(pattern.as_ref().into());
        self
    }

    /// Select functions by fully anchored Rust regular expressions matched after
    /// `generated_name_override`. Dependencies are included.
    pub fn allowlist_function(mut self, pattern: impl AsRef<str>) -> Self {
        self.allowlist_functions.push(pattern.as_ref().into());
        self
    }

    /// Select variables and macros by fully anchored Rust regular expressions.
    /// External objects match after `generated_name_override`. An enumerator of
    /// an anonymous top-level enum without a typedef selects its whole enum.
    pub fn allowlist_var(mut self, pattern: impl AsRef<str>) -> Self {
        self.allowlist_vars.push(pattern.as_ref().into());
        self
    }

    /// Register caller-thread policies. Later callbacks are consulted first.
    pub fn parse_callbacks(mut self, callback: Box<dyn callbacks::ParseCallbacks>) -> Self {
        self.callbacks.push(Rc::from(callback));
        self
    }

    /// Append one C frontend argument. Unsupported options fail generation.
    pub fn clang_arg(mut self, argument: impl AsRef<str>) -> Self {
        self.arguments.push(argument.as_ref().into());
        self
    }

    /// Append C frontend arguments without interpreting shell quoting.
    pub fn clang_args<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.arguments
            .extend(arguments.into_iter().map(|item| item.as_ref().into()));
        self
    }

    /// Enable generated runtime layout tests. Compile-time ABI assertions remain enabled.
    pub fn layout_tests(mut self, enabled: bool) -> Self {
        self.options.no_layout_tests = !enabled;
        self
    }

    /// Map a compatible C size_t to Rust usize.
    pub fn size_t_is_usize(mut self, enabled: bool) -> Self {
        self.options.size_t_is_usize = enabled;
        self
    }

    /// Choose the Rust language version used by generated declarations.
    pub fn rust_target(mut self, target: RustTarget) -> Self {
        self.options.rust_target = target.0;
        self
    }

    /// Use core paths. All generated declarations already use core.
    pub fn use_core(self) -> Self {
        self
    }

    /// Emit named Rust enum variants for an exact C name or trailing `.*` prefix.
    ///
    /// Named enums match their lexical record-qualified name; anonymous enums
    /// match a direct typedef, their generated helper name, or their original
    /// enumerators when no typedef names them. Later aliases do not select an enum.
    pub fn rustified_enum(mut self, pattern: impl AsRef<str>) -> Self {
        match identifier_pattern(pattern.as_ref()) {
            Ok(pattern) if pattern == "*" => self.options.rustified_enums = true,
            Ok(pattern) => self.options.rustified_enum_patterns.push(pattern),
            Err(error) => self.fail(error),
        }
        self
    }

    /// Prepend the enum tag or first anonymous typedef name to integer constants.
    /// Enabled by default. Rust enum variants keep their original names.
    pub fn prepend_enum_name(mut self, enabled: bool) -> Self {
        self.options.prepend_enum_name = enabled;
        self
    }

    /// Derive Copy and Clone where the generated storage supports copying.
    pub fn derive_copy(mut self, enabled: bool) -> Self {
        self.options.derives.copy = enabled;
        self
    }

    /// Derive Debug when supported by every generated field.
    pub fn derive_debug(mut self, enabled: bool) -> Self {
        self.options.derives.debug = Some(enabled);
        self
    }

    /// Provide Default only when zero initializes a valid Rust representation.
    pub fn derive_default(mut self, enabled: bool) -> Self {
        self.options.derives.default = enabled;
        self
    }

    /// Derive Eq where possible; enabling it also requests PartialEq.
    pub fn derive_eq(mut self, enabled: bool) -> Self {
        self.options.derives.eq = enabled;
        if enabled {
            self.options.derives.partial_eq = true;
        }
        self
    }

    /// Derive PartialEq where possible; disabling it also disables Eq.
    pub fn derive_partialeq(mut self, enabled: bool) -> Self {
        self.options.derives.partial_eq = enabled;
        if !enabled {
            self.options.derives.eq = false;
        }
        self
    }

    /// Omit functions matching an exact C identifier or a trailing `.*` prefix.
    pub fn blocklist_function(mut self, pattern: impl AsRef<str>) -> Self {
        match identifier_pattern(pattern.as_ref()) {
            Ok(pattern) => self.options.blocklist_functions.push(pattern),
            Err(error) => self.fail(error),
        }
        self
    }

    /// Omit matching definitions and refer to caller-provided Rust types.
    ///
    /// Use `raw_line` or imports to provide the reported Rust names. The caller
    /// owns representation, validity, and call-ABI compatibility; generated layout
    /// assertions alone cannot prove that contract.
    pub fn blocklist_type(mut self, pattern: impl AsRef<str>) -> Self {
        match identifier_pattern(pattern.as_ref()) {
            Ok(pattern) => self.options.blocklist_types.push(pattern),
            Err(error) => self.fail(error),
        }
        self
    }

    /// Add caller-owned Rust before generated declarations, without formatting it.
    /// Its syntax and ABI remain the caller's responsibility.
    pub fn raw_line(mut self, line: impl Into<String>) -> Self {
        self.options.raw_lines.push(line.into());
        self
    }

    /// Associate matching C `dllimport` declarations with one native DLL library.
    ///
    /// Patterns use the same exact-name or trailing `.*` syntax as `blocklist_function`.
    /// Exact names take precedence, then the longest prefix; a later identical
    /// pattern replaces its library. Only selected imported declarations receive
    /// `#[link]`, on their actual foreign block. No library is inferred from C.
    pub fn dll_import_library(
        mut self,
        pattern: impl AsRef<str>,
        library: impl Into<String>,
    ) -> Self {
        match identifier_pattern(pattern.as_ref()) {
            Ok(pattern) => {
                self.options
                    .dll_import_libraries
                    .insert(pattern, library.into());
            }
            Err(error) => self.fail(error),
        }
        self
    }

    fn fail(&mut self, message: impl Into<String>) {
        self.error.get_or_insert_with(|| message.into());
    }

    /// Emit declaration, field, and enumerator documentation. Defaults to true.
    /// Macro constant documentation is not currently projected.
    pub fn generate_comments(mut self, enabled: bool) -> Self {
        self.generate_comments = enabled;
        self
    }

    /// Preprocess all headers together and generate bindings using the selected target.
    pub fn generate(mut self) -> Result<Bindings, BindgenError> {
        if let Some(error) = self.error {
            return Err(configuration(error));
        }
        if self.headers.is_empty() {
            return Err(configuration("at least one header is required"));
        }
        let patterns = selection::Patterns::new(
            &self.allowlist_files,
            &self.allowlist_types,
            &self.allowlist_functions,
            &self.allowlist_vars,
        )?;
        let (mut config, comments) = arguments::configuration(&self.arguments)?;
        config.analysis.retain_documentation_origins = self.generate_comments;
        config.preprocessor.documentation =
            self.generate_comments
                .then_some(toucan::DocumentationOptions {
                    parse_all_comments: comments.parse_all_comments,
                });
        config.analysis.retain_object_values = true;
        config.analysis.retain_parameter_type_dependencies = true;
        config.analysis.retain_declaration_origins =
            patterns.is_restricted() || !self.callbacks.is_empty();
        config.preprocessor.record_file_origins =
            patterns.files.is_some() || patterns.types.is_some();
        config.preprocessor.record_macro_definitions = true;
        config.preprocessor.macro_redefinition_policy =
            toucan::MacroRedefinitionPolicy::RecordAndReplace;
        let paths = self
            .headers
            .into_iter()
            .map(|header| {
                if header.is_empty() || header.contains('\0') {
                    return Err(configuration("header name cannot be empty or contain NUL"));
                }
                Ok(PathBuf::from(header))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let compilation = toucan::parse_files(&paths, &config)?;
        self.options.emit_function_definitions = true;
        self.options.exclude_inline_functions = true;
        let occurrences = if config.analysis.retain_declaration_origins {
            Some(selection::apply(
                &compilation,
                &patterns,
                &self.callbacks,
                &mut self.options,
            )?)
        } else {
            None
        };
        parameter_dependencies::apply(
            &compilation,
            patterns.files.as_ref(),
            patterns
                .has_names()
                .then_some(occurrences.as_ref())
                .flatten(),
            &mut self.options,
        )?;
        objects::select(&compilation, occurrences.as_ref(), &mut self.options)?;
        if self.generate_comments {
            documentation::apply(&compilation, comments, &mut self.options)?;
        }
        let macros = macro_compat::evaluate(
            &compilation,
            &patterns,
            !self.callbacks.is_empty(),
            &mut self.options,
            self.macro_type_variation,
            self.fit_macro_constants,
        )?;
        let (source, report) =
            compilation.bindings_with_macros(&self.options, &macros.values, macros.skipped)?;
        Ok(Bindings::new(source, report, self.options, self.formatting))
    }
}

/// Generated source and the frontend's declaration, macro, and dependency report.
#[derive(Debug)]
pub struct Bindings {
    source: String,
    report: toucan::Report,
    banner_length: usize,
    raw_lines: Vec<String>,
    formatting: formatting::Options,
    edition: &'static str,
}

impl Bindings {
    /// Separate the core emitter's known banner and raw-line suffix for writing.
    fn new(
        mut source: String,
        report: toucan::Report,
        options: BindingOptions,
        formatting: formatting::Options,
    ) -> Self {
        // The core emitter appends raw lines. The adapter writes them before
        // declarations, outside formatting, as bindgen does. Remove only the
        // suffix whose exact byte length is supplied by these options.
        let raw_bytes: usize = options.raw_lines.iter().map(|line| line.len() + 1).sum();
        source.truncate(source.len() - raw_bytes);
        let banner = format!(
            "// Generated by Toucan for {}.\n// Do not edit.\n\n",
            report.target
        );
        assert!(source.starts_with(&banner), "core emitter banner changed");
        Self {
            source,
            report,
            banner_length: banner.len(),
            raw_lines: options.raw_lines,
            formatting,
            edition: if options.rust_target >= toucan::RustTarget::stable(85).unwrap() {
                "2024"
            } else {
                "2021"
            },
        }
    }

    /// Write the generated Rust source, propagating filesystem errors.
    pub fn write_to_file<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        self.write(Box::new(std::fs::File::create(path)?))
    }

    /// Write bindings, propagating writer errors. Formatter failures use the
    /// unformatted declarations and emit a diagnostic, matching bindgen.
    pub fn write<'a>(&self, mut writer: Box<dyn Write + 'a>) -> io::Result<()> {
        const NL: &str = if cfg!(windows) { "\r\n" } else { "\n" };
        let (banner, source) = self.source.split_at(self.banner_length);
        if cfg!(windows) {
            writer.write_all(banner.replace('\n', NL).as_bytes())?;
        } else {
            writer.write_all(banner.as_bytes())?;
        }
        for line in &self.raw_lines {
            writer.write_all(line.as_bytes())?;
            writer.write_all(NL.as_bytes())?;
        }
        if !self.raw_lines.is_empty() {
            writer.write_all(NL.as_bytes())?;
        }
        if self.formatting.formatter == Formatter::None {
            return writer.write_all(source.as_bytes());
        }
        match self.formatting.format(source, self.edition) {
            Ok(formatted) => writer.write_all(formatted.as_bytes()),
            Err(error) => {
                eprintln!("Failed to run rustfmt: {error} (non-fatal, continuing)");
                writer.write_all(source.as_bytes())
            }
        }
    }

    /// Inspect skipped macros, dependencies, and emitted declaration counts.
    pub fn report(&self) -> &toucan::Report {
        &self.report
    }
}

impl fmt::Display for Bindings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut bytes = Vec::new();
        self.write(Box::new(&mut bytes))
            .expect("writing bindings to a Vec cannot fail");
        formatter.write_str(std::str::from_utf8(&bytes).expect("bindings are UTF-8"))
    }
}

fn identifier_pattern(pattern: &str) -> Result<String, String> {
    let pattern = pattern.strip_prefix('^').unwrap_or(pattern);
    let pattern = pattern.strip_suffix('$').unwrap_or(pattern);
    let (name, prefix) = pattern
        .strip_suffix(".*")
        .map_or((pattern, false), |name| (name, true));
    if (name.is_empty() && !prefix)
        || !name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
    {
        return Err("patterns must be C identifiers optionally followed by .*; other regex syntax is not supported yet".into());
    }
    Ok(if prefix {
        format!("{name}*")
    } else {
        name.into()
    })
}

fn host_target() -> Option<Target> {
    Target::ALL.into_iter().find(|target| match target {
        Target::X86_64UnknownLinuxGnu => cfg!(all(
            target_arch = "x86_64",
            target_os = "linux",
            target_env = "gnu"
        )),
        Target::Aarch64UnknownLinuxGnu => cfg!(all(
            target_arch = "aarch64",
            target_os = "linux",
            target_env = "gnu"
        )),
        Target::X86_64UnknownLinuxMusl => cfg!(all(
            target_arch = "x86_64",
            target_os = "linux",
            target_env = "musl"
        )),
        Target::Aarch64UnknownLinuxMusl => cfg!(all(
            target_arch = "aarch64",
            target_os = "linux",
            target_env = "musl"
        )),
        Target::X86_64AppleDarwin => cfg!(all(target_arch = "x86_64", target_os = "macos")),
        Target::Aarch64AppleDarwin => cfg!(all(target_arch = "aarch64", target_os = "macos")),
        Target::X86_64PcWindowsMsvc => cfg!(all(
            target_arch = "x86_64",
            target_os = "windows",
            target_env = "msvc"
        )),
    })
}

fn clang_config(target: Target, mode: toucan::LanguageMode) -> Config {
    Config::with_profile(
        CompilerProfile::new(target, Compiler::Clang)
            .expect("all supported targets have a Clang profile")
            .with_language_mode(mode),
    )
}

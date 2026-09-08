//! A libclang-free builder for C binding build scripts.
//!
//! This crate implements a supported subset of bindgen's builder API. Unsupported
//! arguments and patterns produce errors during generation. It never starts a C
//! compiler, loads libclang, or discovers a compiler installation. Supply target
//! headers with include arguments and a sysroot. Cargo's `TARGET` selects the ABI
//! unless an explicit `--target` argument overrides it.

mod arguments;

use std::fmt;
use std::path::Path;

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
    blocked_types: Vec<String>,
    options: BindingOptions,
    error: Option<String>,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            headers: Vec::new(),
            arguments: Vec::new(),
            blocked_types: Vec::new(),
            options: BindingOptions {
                size_t_is_usize: true,
                macro_type: MacroType::Unsigned,
                rust_target: RustTarget::default().0,
                ..BindingOptions::default()
            },
            error: None,
        }
    }
}

impl Builder {
    /// Add a header, in translation-unit order.
    pub fn header(mut self, header: impl Into<String>) -> Self {
        self.headers.push(header.into());
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

    /// Emit named Rust enum variants. Currently only the all-enums pattern is supported.
    pub fn rustified_enum(mut self, pattern: impl AsRef<str>) -> Self {
        if matches!(pattern.as_ref(), ".*" | "^.*$") {
            self.options.rustified_enums = true;
        } else {
            self.fail("selective rustified_enum patterns are not supported yet");
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

    /// Record a type blocklist. Matching declared types currently require an error.
    ///
    /// Absent types are accepted, including a build script's defensive max_align_t
    /// blocklist. Supplying arbitrary external Rust type replacements is not yet supported.
    pub fn blocklist_type(mut self, pattern: impl AsRef<str>) -> Self {
        match identifier_pattern(pattern.as_ref()) {
            Ok(pattern) => self.blocked_types.push(pattern),
            Err(error) => self.fail(error),
        }
        self
    }

    /// Append caller-owned Rust source. Its syntax and ABI remain the caller's responsibility.
    pub fn raw_line(mut self, line: impl Into<String>) -> Self {
        self.options.raw_lines.push(line.into());
        self
    }

    fn fail(&mut self, message: impl Into<String>) {
        self.error.get_or_insert_with(|| message.into());
    }

    /// Preprocess all headers together and generate bindings using the selected target.
    pub fn generate(self) -> Result<Bindings, BindgenError> {
        if let Some(error) = self.error {
            return Err(configuration(error));
        }
        if self.headers.is_empty() {
            return Err(configuration("at least one header is required"));
        }
        let config = arguments::configuration(&self.arguments)?;
        let mut source = String::new();
        for header in self.headers {
            if header.is_empty() || header.contains(['"', '\n', '\r', '\0']) {
                return Err(configuration(
                    "header name cannot be empty or contain quotes, newlines, or NUL",
                ));
            }
            use fmt::Write;
            writeln!(source, "#include \"{header}\"").unwrap();
        }
        let compilation =
            toucan::parse_source(Path::new("__toucan_bindgen__.h"), &source, &config)?;
        let unit = compilation.unit();
        for name in unit
            .typedefs
            .keys()
            .map(String::as_str)
            .chain(
                unit.records
                    .iter()
                    .filter_map(|record| record.name.as_deref()),
            )
            .chain(unit.enums.iter().filter_map(|item| item.name.as_deref()))
        {
            if self
                .blocked_types
                .iter()
                .any(|pattern| matches_identifier(pattern, name))
            {
                return Err(configuration(format!(
                    "blocklisted type `{name}` is declared; external Rust type replacements are not supported yet"
                )));
            }
        }
        let (source, report) = compilation.bindings(&self.options)?;
        Ok(Bindings { source, report })
    }
}

/// Generated source and the frontend's declaration, macro, and dependency report.
#[derive(Debug)]
pub struct Bindings {
    source: String,
    report: toucan::Report,
}

impl Bindings {
    /// Write the generated Rust source, propagating filesystem errors.
    pub fn write_to_file(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::write(path, &self.source)
    }

    /// Inspect skipped macros, dependencies, and emitted declaration counts.
    pub fn report(&self) -> &toucan::Report {
        &self.report
    }
}

impl fmt::Display for Bindings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.source)
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

fn matches_identifier(pattern: &str, name: &str) -> bool {
    pattern
        .strip_suffix('*')
        .map_or(pattern == name, |prefix| name.starts_with(prefix))
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
        Target::X86_64AppleDarwin => cfg!(all(target_arch = "x86_64", target_os = "macos")),
        Target::Aarch64AppleDarwin => cfg!(all(target_arch = "aarch64", target_os = "macos")),
        Target::X86_64PcWindowsMsvc => cfg!(all(
            target_arch = "x86_64",
            target_os = "windows",
            target_env = "msvc"
        )),
    })
}

fn clang_config(target: Target) -> Config {
    Config::with_profile(
        CompilerProfile::new(target, Compiler::Clang)
            .expect("all supported targets have a Clang profile"),
    )
}

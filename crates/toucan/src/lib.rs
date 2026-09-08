//! A reusable, target-explicit frontend for C headers.
//!
//! The frontend never launches a compiler. Callers supply include directories and
//! the target's sysroot. Declarations, initializers, and function bodies are checked
//! within the supported C feature set. Unsupported bindings produce diagnostics.

mod features;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use toucan_bindings::{Bindings, MacroType, Options as BindingOptions, RustTarget};
pub use toucan_preprocessor::{
    CommandLineMacroNormalizer, FeatureQueries, FeatureQuery, FeatureQueryProvider, ForcedInclude,
    LineComments, OriginKind, PredefinedMacroMode, QueryDialect, SourceLocation, SourceMapping,
};
pub use toucan_preprocessor::{Config as PreprocessorConfig, Preprocessed, Preprocessor};
pub use toucan_preprocessor::{PreprocessingTimestamp, TimestampError};
pub use toucan_semantic::{self as semantic, Analysis, AnalysisOptions, TranslationUnit};
pub use toucan_source as source;
pub use toucan_target::{self as target, Compiler, CompilerProfile, LanguageMode, Target};

#[derive(Debug, Clone)]
pub struct Config {
    profile: CompilerProfile,
    pub preprocessor: PreprocessorConfig,
    /// Optional owned semantic graph retention; disabled by default.
    pub analysis: AnalysisOptions,
}

impl Config {
    /// Physical ABI and platform selected for preprocessing and analysis.
    pub fn target(&self) -> Target {
        self.profile.target()
    }
    /// Compiler behavior used by this configuration.
    pub fn compiler(&self) -> Compiler {
        self.profile.compiler()
    }
    /// The validated target/compiler pair.
    pub fn profile(&self) -> CompilerProfile {
        self.profile
    }
    /// C keywords and preprocessing defaults selected with the compiler profile.
    pub fn language_mode(&self) -> LanguageMode {
        self.profile.language_mode()
    }
    /// Uses GCC on Linux and Clang on Darwin and Windows.
    pub fn new(target: Target) -> Self {
        Self::with_profile(CompilerProfile::default_for(target))
    }

    /// Builds matching preprocessing and semantic configuration before caller overrides.
    pub fn with_profile(profile: CompilerProfile) -> Self {
        let target = profile.target();
        let mut preprocessor = PreprocessorConfig {
            feature_queries: Some(features::queries(profile)),
            scope_punctuator: profile.compiler() == Compiler::Clang
                || profile.language_mode().is_gnu(),
            trigraphs: profile.default_trigraphs(),
            line_comments: if profile.language_mode() == LanguageMode::C90 {
                match profile.compiler() {
                    Compiler::Gnu => LineComments::GnuC90,
                    Compiler::Clang => LineComments::ClangC90,
                }
            } else {
                LineComments::Enabled
            },
            predefined_macro_mode: match profile.compiler() {
                Compiler::Gnu => PredefinedMacroMode::GnuCommandLine,
                Compiler::Clang => PredefinedMacroMode::ClangCommandLine,
            },
            char_unsigned: !target.char_is_signed(),
            defines: profile.predefined_macros(),
            ..PreprocessorConfig::default()
        };
        if target != Target::X86_64PcWindowsMsvc {
            preprocessor.forced_includes.push(ForcedInclude {
                path: "<builtin>/integer-types.h".into(),
                source: include_str!("../resources/integer-types.h").into(),
            });
        }
        preprocessor.virtual_headers.extend([
            (
                "limits.h".into(),
                include_str!("../resources/limits.h").into(),
            ),
            (
                "stddef.h".into(),
                include_str!("../resources/stddef.h").into(),
            ),
            (
                "stdarg.h".into(),
                include_str!("../resources/stdarg.h").into(),
            ),
            (
                "stdbool.h".into(),
                include_str!("../resources/stdbool.h").into(),
            ),
        ]);
        Self {
            profile,
            preprocessor,
            analysis: AnalysisOptions::default(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Timings {
    pub preprocessing: Duration,
    pub analysis: Duration,
}

/// Owns preprocessed source, target-specific declarations, and optional checked code.
///
/// Shared access keeps declaration IDs and source ranges valid together. Request
/// retained code through [`Config::analysis`] before parsing.
pub struct Compilation {
    analysis: Analysis,
    preprocessed: Preprocessed,
    timings: Timings,
}

/// A semantic diagnostic with its original source anchor, when available.
///
/// `error.offset` is retained as a byte offset into preprocessed source. An origin
/// identifies the start of an original token or preserved directive. Macro output
/// identifies the outer invocation, without implying a definition location or a
/// complete expansion stack.
#[derive(Debug)]
pub struct SemanticError {
    pub error: semantic::Error,
    pub origin: Option<SourceLocation>,
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(origin) = &self.origin {
            write!(formatter, "{origin}: {}", self.error.message)?;
            if origin.kind == OriginKind::MacroInvocation {
                write!(
                    formatter,
                    " (macro invocation; preprocessed byte {})",
                    self.error.offset
                )
            } else {
                write!(formatter, " (preprocessed byte {})", self.error.offset)
            }
        } else {
            write!(
                formatter,
                "{} (preprocessed byte {})",
                self.error.message, self.error.offset
            )
        }
    }
}

impl std::error::Error for SemanticError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Preprocessor(#[from] toucan_preprocessor::Error),
    #[error(transparent)]
    Semantic(#[from] SemanticError),
    #[error(transparent)]
    Bindings(#[from] toucan_bindings::Error),
}

pub fn parse_file(path: &Path, config: &Config) -> Result<Compilation, Error> {
    let start = Instant::now();
    let preprocessed = Preprocessor::new(config.preprocessor.clone()).preprocess(path)?;
    finish(preprocessed, config, start.elapsed())
}

pub fn parse_source(path: &Path, source: &str, config: &Config) -> Result<Compilation, Error> {
    let start = Instant::now();
    let preprocessed =
        Preprocessor::new(config.preprocessor.clone()).preprocess_str(path, source)?;
    finish(preprocessed, config, start.elapsed())
}

fn finish(
    preprocessed: Preprocessed,
    config: &Config,
    preprocessing: Duration,
) -> Result<Compilation, Error> {
    let start = Instant::now();
    let analysis =
        semantic::analyze_with_profile(&preprocessed.source, config.profile, &config.analysis)
            .map_err(|error| SemanticError {
                origin: preprocessed.resolve_location(error.offset).cloned(),
                error,
            })?;
    Ok(Compilation {
        analysis,
        preprocessed,
        timings: Timings {
            preprocessing,
            analysis: start.elapsed(),
        },
    })
}

#[derive(Debug, serde::Serialize)]
pub struct Report {
    pub target: String,
    pub compiler: Compiler,
    pub language_mode: LanguageMode,
    /// Minimum Rust version for generated declarations, excluding caller-provided lines.
    pub rust_target: String,
    pub dependencies: Vec<PathBuf>,
    pub declarations: usize,
    pub integer_macros: usize,
    pub floating_macros: usize,
    pub string_macros: usize,
    pub skipped_declarations: Vec<String>,
    /// Selected functions deliberately omitted by the caller's blocklist.
    pub blocked_functions: Vec<String>,
    /// Caller-owned external types; their Rust definitions are not frontend-verified.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocked_types: Vec<toucan_bindings::ExternalType>,
    /// Caller-provided Rust, excluded from declaration counts and ABI validation.
    pub raw_lines: Vec<String>,
    pub skipped_macros: Vec<SkippedMacro>,
    /// Enum constants use the compatible enum integer type in Rust. Their C
    /// expression types are retained here for independent compiler validation.
    pub enum_constants: Vec<toucan_bindings::EnumConstants>,
    /// Rust-to-C names for macro constants whose identifiers were escaped or renamed.
    pub renamed_macros: BTreeMap<String, String>,
    /// C expression types retained when an explicit macro policy changes the Rust type.
    pub macro_types: Vec<toucan_bindings::MacroIntegerType>,
    pub timings: Timings,
}

#[derive(Debug, serde::Serialize)]
pub struct SkippedMacro {
    pub name: String,
    pub reason: String,
}

impl Compilation {
    /// Returns the immutable semantic owner, including optional checked code.
    pub fn analysis(&self) -> &Analysis {
        &self.analysis
    }
    /// Returns checked target-specific declarations.
    pub fn unit(&self) -> &TranslationUnit {
        self.analysis.unit()
    }
    /// Returns the original preprocessed source and its token origins.
    pub fn preprocessed(&self) -> &Preprocessed {
        &self.preprocessed
    }
    /// Returns time spent preprocessing and checking this compilation.
    pub fn timings(&self) -> &Timings {
        &self.timings
    }
    /// Returns retained code when requested by [`Config::analysis`].
    pub fn checked(&self) -> Option<&semantic::checked::CheckedCode> {
        self.analysis.checked()
    }
    /// Resolves the token origins intersecting a retained source span.
    ///
    /// Disjoint fragments retain their mapped order. Repeated origins can occur,
    /// especially for macros: a macro origin anchors its outer invocation, not a
    /// full expansion trace. Parser-inserted spans have no original locations.
    pub fn source_locations<'a>(
        &'a self,
        span: &'a semantic::checked::SourceSpan,
    ) -> impl Iterator<Item = &'a SourceLocation> + 'a {
        std::iter::once(span.range())
            .filter(move |_| span.fragments().is_empty() && !span.synthetic())
            .chain(
                span.fragments()
                    .iter()
                    .filter(move |_| !span.synthetic())
                    .cloned(),
            )
            .flat_map(|range| {
                let mappings = &self.preprocessed.mappings;
                let first = mappings.partition_point(|entry| entry.generated.end <= range.start);
                mappings[first..]
                    .iter()
                    .take_while(move |entry| entry.generated.start < range.end)
                    .map(|entry| &entry.origin)
            })
    }

    /// Generates declarations and supported object-like macro constants. The
    /// report identifies selected macros that were not emitted. With no allowlist,
    /// reserved `__` macros are omitted unless they shadow a declaration.
    pub fn bindings(&self, options: &BindingOptions) -> Result<(String, Report), Error> {
        semantic::with_parser_stack(|| self.bindings_on_parser_stack(options)).map_err(|error| {
            SemanticError {
                error,
                origin: None,
            }
        })?
    }

    fn bindings_on_parser_stack(
        &self,
        options: &BindingOptions,
    ) -> Result<(String, Report), Error> {
        let declared_names: BTreeSet<_> = self
            .unit()
            .declarations
            .iter()
            .map(|item| item.name.as_str())
            .chain(self.unit().constants.keys().map(String::as_str))
            .chain(
                self.unit()
                    .records
                    .iter()
                    .filter(|item| item.scope == semantic::Scope::File)
                    .filter_map(|item| item.name.as_deref()),
            )
            .chain(
                self.unit()
                    .enums
                    .iter()
                    .filter(|item| item.scope == semantic::Scope::File)
                    .filter_map(|item| item.name.as_deref()),
            )
            .collect();
        let mut macros = BTreeMap::new();
        let mut skipped_macros = Vec::new();
        let mut integer_macros = 0;
        let mut floating_macros = 0;
        let mut string_macros = 0;
        for (name, definition) in &self.preprocessed.macros {
            if !options.includes(name)
                || (name.starts_with("__")
                    && !declared_names.contains(name.as_str())
                    && options.allowlist.is_empty())
            {
                continue;
            }
            if definition.parameters.is_some() {
                skipped_macros.push(SkippedMacro {
                    name: name.clone(),
                    reason: "function-like macro".into(),
                });
                continue;
            }
            // Even an unsupported replacement hides an enumerator with this name.
            // Keep the original semantic value available to evaluate other macros.
            macros.insert(name.clone(), None);
            let expression = match self.preprocessed.expand_object_macro(name) {
                Ok(Some(value)) if !value.trim().is_empty() => value,
                Ok(_) => {
                    skipped_macros.push(SkippedMacro {
                        name: name.clone(),
                        reason: "empty replacement".into(),
                    });
                    continue;
                }
                Err(error) => {
                    skipped_macros.push(SkippedMacro {
                        name: name.clone(),
                        reason: error.to_string(),
                    });
                    continue;
                }
            };
            if expression.trim() == name && self.unit().constants.contains_key(name) {
                // System headers commonly define enum members as self-aliases so
                // #ifdef can detect them. Preserve their enum-compatible Rust type.
                macros.remove(name);
                continue;
            }
            match string_literal(&expression, self.unit().target) {
                Ok(Some(value)) => {
                    macros.insert(name.clone(), Some(value));
                    string_macros += 1;
                }
                Err(error) => skipped_macros.push(SkippedMacro {
                    name: name.clone(),
                    reason: error.to_string(),
                }),
                Ok(None) => match semantic::evaluate_integer(self.unit(), &expression)
                    .map(semantic::ArithmeticConstant::Integer)
                    .or_else(|_| semantic::evaluate_arithmetic(self.unit(), &expression))
                {
                    Ok(semantic::ArithmeticConstant::Integer(value)) => {
                        macros.insert(
                            name.clone(),
                            Some(toucan_bindings::MacroValue::Integer(value)),
                        );
                        integer_macros += 1;
                    }
                    Ok(semantic::ArithmeticConstant::Floating(value)) => {
                        if matches!(
                            value.kind(),
                            semantic::FloatKind::Float
                                | semantic::FloatKind::Double
                                | semantic::FloatKind::FLOAT32
                                | semantic::FloatKind::FLOAT64
                                | semantic::FloatKind::FLOAT32X
                        ) {
                            macros.insert(
                                name.clone(),
                                Some(toucan_bindings::MacroValue::Floating(value)),
                            );
                            floating_macros += 1;
                        } else {
                            skipped_macros.push(SkippedMacro {
                                name: name.clone(),
                                reason: if value.kind().is_narrow() {
                                    format!("{} macro constants have no Rust representation; use an explicit float or double cast", if value.kind() == semantic::FloatKind::BFloat16 {"__bf16"} else {"_Float16"})
                                } else if value.kind() == semantic::FloatKind::FLOAT64X {
                                    "_Float64x macro constants have no Rust representation; use an explicit float or double cast".into()
                                } else if value.kind() == semantic::FloatKind::FLOAT128 {
                                    "binary128 macro constants have no verified Rust representation; use an explicit float or double cast".into()
                                } else {
                                    "long double macro constants have no Rust representation; use an explicit float or double cast".into()
                                },
                            });
                        }
                    }
                    Ok(semantic::ArithmeticConstant::Complex(_)) => skipped_macros
                        .push(SkippedMacro {
                        name: name.clone(),
                        reason:
                            "complex macro constants have no verified Rust storage representation"
                                .into(),
                    }),
                    Err(error) => skipped_macros.push(SkippedMacro {
                        name: name.clone(),
                        reason: error.to_string(),
                    }),
                },
            }
        }
        let bindings = toucan_bindings::generate_with_macros(self.unit(), options, &macros)?;
        let source = bindings.source;
        let report = Report {
            target: self.unit().target.triple().into(),
            compiler: self.unit().compiler,
            language_mode: self.unit().language_mode,
            rust_target: options.rust_target.to_string(),
            dependencies: self.preprocessed.dependencies.clone(),
            declarations: bindings.declarations,
            integer_macros,
            floating_macros,
            string_macros,
            skipped_declarations: bindings.skipped,
            blocked_functions: bindings.blocked_functions,
            blocked_types: bindings.blocked_types,
            raw_lines: bindings.raw_lines,
            skipped_macros,
            enum_constants: bindings.enum_constants,
            renamed_macros: bindings.renamed_macros,
            macro_types: bindings.macro_types,
            timings: self.timings.clone(),
        };
        Ok((source, report))
    }
}

/// Recognizes an entire replacement made of adjacent string tokens. The semantic
/// decoder owns escape validation, concatenation, and target character encoding.
fn string_literal(
    source: &str,
    target: Target,
) -> Result<Option<toucan_bindings::MacroValue>, semantic::Error> {
    let bytes = source.as_bytes();
    let mut offset = 0;
    let mut tokens = Vec::new();
    while offset < bytes.len() {
        while bytes.get(offset).is_some_and(u8::is_ascii_whitespace) {
            offset += 1;
        }
        if offset == bytes.len() {
            break;
        }
        let start = offset;
        if bytes[offset..].starts_with(b"u8\"") {
            offset += 2;
        } else if matches!(bytes[offset], b'u' | b'U' | b'L')
            && bytes.get(offset + 1) == Some(&b'"')
        {
            offset += 1;
        }
        if bytes.get(offset) != Some(&b'"') {
            return Ok(None);
        }
        offset += 1;
        while offset < bytes.len() && bytes[offset] != b'"' {
            if bytes[offset] == b'\\' {
                offset += 1;
            }
            offset += 1;
        }
        if offset >= bytes.len() {
            return Err(semantic::Error {
                offset: start,
                message: "unterminated string literal".into(),
            });
        }
        offset += 1;
        tokens.push(source[start..offset].to_owned());
    }
    if tokens.is_empty() {
        return Ok(None);
    }
    let mut decoded = semantic::decode_string_literals(&tokens, target, 0)?;
    if let Some(mut bytes) = decoded.to_bytes() {
        bytes.pop();
        Ok(Some(toucan_bindings::MacroValue::String(bytes)))
    } else {
        decoded.code_units.pop();
        Ok(Some(toucan_bindings::MacroValue::WideString {
            element_type: decoded.element_type,
            code_units: decoded.code_units,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_entire_string_replacements() {
        let Some(toucan_bindings::MacroValue::String(bytes)) =
            string_literal(r#""a\n" "\x62\143""#, Target::X86_64UnknownLinuxGnu).unwrap()
        else {
            panic!("expected byte string")
        };
        assert_eq!(bytes, b"a\nbc");
        assert!(string_literal(r#""\x100""#, Target::X86_64UnknownLinuxGnu).is_err());
        for source in [r#""a" + 1"#, r#"u8 + "x""#, r#""x" [0]"#, "5", ""] {
            assert!(
                string_literal(source, Target::X86_64UnknownLinuxGnu)
                    .unwrap()
                    .is_none()
            );
        }
    }
}

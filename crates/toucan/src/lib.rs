//! A reusable, target-explicit frontend for C headers.
//!
//! The frontend never launches a compiler. Callers supply include directories and
//! the target's sysroot. Declarations, initializers, and function bodies are checked
//! within the supported C feature set. Unsupported bindings produce diagnostics.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use toucan_bindings::{Bindings, MacroType, Options as BindingOptions};
pub use toucan_preprocessor::{Config as PreprocessorConfig, Preprocessed, Preprocessor};
pub use toucan_preprocessor::{ForcedInclude, OriginKind, SourceLocation, SourceMapping};
pub use toucan_semantic::{self as semantic, TranslationUnit};
pub use toucan_source as source;
pub use toucan_target::{self as target, Target};

#[derive(Debug, Clone)]
pub struct Config {
    pub target: Target,
    pub preprocessor: PreprocessorConfig,
}

impl Config {
    pub fn new(target: Target) -> Self {
        let mut preprocessor = PreprocessorConfig {
            defines: target.predefined_macros(),
            ..PreprocessorConfig::default()
        };
        // These predicates advertise only implemented syntax/semantics. They are
        // independent of the GNU version used for header compatibility.
        for name in [
            "__has_builtin(x)",
            "__has_attribute(x)",
            "__has_feature(x)",
            "__has_extension(x)",
            "__has_c_attribute(x)",
            "__has_declspec_attribute(x)",
            "__building_module(x)",
        ] {
            preprocessor.defines.insert(name.into(), "0".into());
        }
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
            target,
            preprocessor,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Timings {
    pub preprocessing: Duration,
    pub analysis: Duration,
}

pub struct Compilation {
    pub unit: TranslationUnit,
    pub preprocessed: Preprocessed,
    pub timings: Timings,
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
    finish(preprocessed, config.target, start.elapsed())
}

pub fn parse_source(path: &Path, source: &str, config: &Config) -> Result<Compilation, Error> {
    let start = Instant::now();
    let preprocessed =
        Preprocessor::new(config.preprocessor.clone()).preprocess_str(path, source)?;
    finish(preprocessed, config.target, start.elapsed())
}

fn finish(
    preprocessed: Preprocessed,
    target: Target,
    preprocessing: Duration,
) -> Result<Compilation, Error> {
    let start = Instant::now();
    let unit = semantic::analyze(&preprocessed.source, target).map_err(|error| SemanticError {
        origin: preprocessed.resolve_location(error.offset).cloned(),
        error,
    })?;
    Ok(Compilation {
        unit,
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
    pub dependencies: Vec<PathBuf>,
    pub declarations: usize,
    pub integer_macros: usize,
    pub string_macros: usize,
    pub skipped_declarations: Vec<String>,
    /// Selected functions deliberately omitted by the caller's blocklist.
    pub blocked_functions: Vec<String>,
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
    /// Generates declarations and supported object-like macro constants. The
    /// report identifies selected macros that were not emitted. With no allowlist,
    /// reserved `__` macros are omitted unless they shadow a declaration.
    pub fn bindings(&self, options: &BindingOptions) -> Result<(String, Report), Error> {
        let declared_names: BTreeSet<_> = self
            .unit
            .declarations
            .iter()
            .map(|item| item.name.as_str())
            .chain(self.unit.constants.keys().map(String::as_str))
            .chain(
                self.unit
                    .records
                    .iter()
                    .filter(|item| item.scope == semantic::Scope::File)
                    .filter_map(|item| item.name.as_deref()),
            )
            .chain(
                self.unit
                    .enums
                    .iter()
                    .filter(|item| item.scope == semantic::Scope::File)
                    .filter_map(|item| item.name.as_deref()),
            )
            .collect();
        let mut macros = BTreeMap::new();
        let mut skipped_macros = Vec::new();
        let mut integer_macros = 0;
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
            if expression.trim() == name && self.unit.constants.contains_key(name) {
                // System headers commonly define enum members as self-aliases so
                // #ifdef can detect them. Preserve their enum-compatible Rust type.
                macros.remove(name);
                continue;
            }
            if let Some(bytes) = string_literal(&expression) {
                macros.insert(
                    name.clone(),
                    Some(toucan_bindings::MacroValue::String(bytes)),
                );
                string_macros += 1;
            } else {
                match semantic::evaluate_integer(&self.unit, &expression) {
                    Ok(value) => {
                        macros.insert(
                            name.clone(),
                            Some(toucan_bindings::MacroValue::Integer(value)),
                        );
                        integer_macros += 1;
                    }
                    Err(error) => skipped_macros.push(SkippedMacro {
                        name: name.clone(),
                        reason: error.to_string(),
                    }),
                }
            }
        }
        let bindings = toucan_bindings::generate_with_macros(&self.unit, options, &macros)?;
        let source = bindings.source;
        let report = Report {
            target: self.unit.target.triple().into(),
            dependencies: self.preprocessed.dependencies.clone(),
            declarations: bindings.declarations,
            integer_macros,
            string_macros,
            skipped_declarations: bindings.skipped,
            blocked_functions: bindings.blocked_functions,
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

/// Decode ordinary, adjacent C string literals. Wide/UTF-prefixed strings need a
/// different element type and are deliberately not accepted here.
fn string_literal(source: &str) -> Option<Vec<u8>> {
    let source = source.as_bytes();
    let mut offset = 0;
    let mut bytes = Vec::new();
    let mut found = false;
    while offset < source.len() {
        while source.get(offset).is_some_and(u8::is_ascii_whitespace) {
            offset += 1;
        }
        if offset == source.len() {
            break;
        }
        if source[offset] != b'"' {
            return None;
        }
        found = true;
        offset += 1;
        loop {
            let byte = *source.get(offset)?;
            offset += 1;
            match byte {
                b'"' => break,
                b'\n' | b'\r' => return None,
                b'\\' => {
                    let escape = *source.get(offset)?;
                    offset += 1;
                    bytes.push(match escape {
                        b'a' => 7,
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => 10,
                        b'r' => 13,
                        b't' => 9,
                        b'v' => 11,
                        b'\\' | b'\'' | b'"' | b'?' => escape,
                        b'x' => {
                            let start = offset;
                            let mut value = 0u16;
                            while let Some(digit) =
                                source.get(offset).and_then(|b| char::from(*b).to_digit(16))
                            {
                                value = value.checked_mul(16)?.checked_add(digit as u16)?;
                                offset += 1;
                            }
                            if offset == start {
                                return None;
                            }
                            u8::try_from(value).ok()?
                        }
                        b'0'..=b'7' => {
                            let mut value = u16::from(escape - b'0');
                            for _ in 0..2 {
                                let Some(digit @ b'0'..=b'7') = source.get(offset) else {
                                    break;
                                };
                                value = value * 8 + u16::from(*digit - b'0');
                                offset += 1;
                            }
                            u8::try_from(value).ok()?
                        }
                        _ => return None,
                    });
                }
                _ => bytes.push(byte),
            }
        }
    }
    found.then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_c_strings_without_accepting_arbitrary_expressions() {
        assert_eq!(
            string_literal(r#""a\n" "\x62\143""#),
            Some(b"a\nbc".to_vec())
        );
        assert_eq!(string_literal(r#""\x100""#), None);
        assert_eq!(string_literal(r#""a" + 1"#), None);
        assert_eq!(string_literal(r#"L"wide""#), None);
    }
}

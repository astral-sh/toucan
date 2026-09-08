//! C declarations, target-specific types, expressions, and function-body constraints.
//!
//! This crate checks preprocessed C without invoking an external compiler. Unsupported
//! constructs return diagnostics; function bodies are checked even when a binding
//! consumer omits their definitions from its generated API.

mod analyze;
mod asm;
mod atomic;
mod builtins;
pub mod checked;
mod constant_query;
mod expression;
mod floating;
mod fortified;
mod initializer;
mod integer;
mod ir;
mod literals;
mod object_extent;
mod object_size;
mod overflow;
mod parser_extensions;
mod returns_twice;
mod statement;
mod sync;
mod transparent_union;
mod variadic_pack;
mod vector;
mod weak;
mod x86;

pub use analyze::{analyze, analyze_with_options, evaluate_arithmetic, evaluate_integer};
pub use ir::*;
pub use literals::{
    DecodedString, StringEncoding, decode_character_literal, decode_string_literals,
};

/// A source-positioned syntax, semantic, or unsupported-feature diagnostic.
#[derive(Clone, Debug, thiserror::Error, serde::Serialize)]
#[error("{message} at byte {offset}")]
pub struct Error {
    pub message: String,
    pub offset: usize,
}

impl Error {
    pub(crate) fn new(offset: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset,
        }
    }
}

/// Controls optional semantic retention. The default checks all supported C code
/// without allocating the retained graph.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnalysisOptions {
    /// Retain checked expressions, statements, initializers, and their bindings.
    pub retain_code: bool,
    /// Resource limits applied only when `retain_code` is enabled.
    pub limits: checked::Limits,
}

/// Owns declarations and, optionally, their immutable checked semantic graph.
///
/// The source text need not outlive this value. Retained source ranges refer to
/// byte offsets in that input, so keep it separately if source excerpts are needed.
///
/// The owner cannot be used to mutate declarations behind retained IDs:
///
/// ```compile_fail
/// use toucan_semantic::{analyze_with_options, AnalysisOptions};
/// use toucan_target::Target;
/// let analysis = analyze_with_options("int x;", Target::X86_64UnknownLinuxGnu,
///     &AnalysisOptions::default()).unwrap();
/// analysis.unit().declarations.clear();
/// ```
///
/// Node IDs refer only to this analysis. Consuming it with [`Self::into_unit`]
/// discards the checked graph before returning mutable declaration data.
#[derive(Debug, serde::Serialize)]
pub struct Analysis {
    unit: TranslationUnit,
    checked: Option<checked::CheckedCode>,
}

impl Analysis {
    /// Returns the checked translation unit and its target-specific types.
    pub fn unit(&self) -> &TranslationUnit {
        &self.unit
    }
    /// Returns retained code when requested by [`AnalysisOptions::retain_code`].
    pub fn checked(&self) -> Option<&checked::CheckedCode> {
        self.checked.as_ref()
    }
    /// Discards retained code and returns the owned declaration representation.
    pub fn into_unit(self) -> TranslationUnit {
        self.unit
    }
}

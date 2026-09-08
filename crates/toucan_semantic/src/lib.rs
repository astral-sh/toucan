//! C declarations, target-specific types, expressions, and function-body constraints.
//!
//! This crate checks preprocessed C without invoking an external compiler. Unsupported
//! constructs return diagnostics; function bodies are checked even when a binding
//! consumer omits their definitions from its generated API.

mod alignment_names;
mod alignof;
mod allocation;
mod analyze;
mod arm;
mod array_identity;
mod asm;
mod atomic;
mod atomic_type;
mod attributes;
mod auto_type;
mod builtin_function;
mod builtins;
mod c11_atomic;
pub mod checked;
mod complex;
mod constant_query;
mod declspec;
mod elementwise;
mod enums;
mod expression;
mod features;
mod floating;
mod fortified;
mod implicit_function;
mod initializer;
mod integer;
mod introspection;
mod ir;
mod literals;
mod narrow_float;
mod noescape;
mod nontemporal;
mod noreturn;
mod object_alignment;
mod object_extent;
mod object_size;
mod old_style;
mod overflow;
mod parameters;
mod parser_extensions;
mod prefetch;
mod returns_twice;
mod statement;
mod sync;
mod target_features;
mod target_names;
mod transparent_union;
mod type_alignment;
mod variadic_pack;
mod vector;
mod vector_constant;
mod weak;
mod wide_float;
mod x86;

pub use allocation::AllocationOperation;
pub use analyze::{
    analyze, analyze_with_options, analyze_with_profile, evaluate_arithmetic, evaluate_integer,
    evaluate_vector,
};
pub use arm::Aarch64Pcs;
pub use array_identity::VariableArrayId;
pub use attributes::has_attribute;
pub use builtin_function::BuiltinFunction;
pub use declspec::has_declspec_attribute;
pub use features::has_builtin;
pub use ir::*;
pub use literals::{
    DecodedString, StringEncoding, decode_character_literal, decode_character_literal_with_profile,
    decode_string_literals,
};
pub use noescape::{ParameterContracts, ParameterContractsId};
pub use object_alignment::DeclarationAlignment;
pub use prefetch::PrefetchHint;
pub use target_features::{FunctionOptions, FunctionTarget, X86TargetOption};
pub use type_alignment::{AlignmentOrigin, AlignmentOriginId, AlignmentOriginKind, TypeAlignment};
pub use vector_constant::{VectorConstant, VectorConstantType, VectorElement};

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

/// Runs related analysis and constant-evaluation calls on one parser stack.
///
/// This amortizes worker creation for tools evaluating many macros. Each parse
/// keeps independent limits and lexical state; nested sessions reuse the stack.
/// The worker is joined before return, and panics propagate to the caller.
pub fn with_parser_stack<T: Send>(operation: impl FnOnce() -> T + Send) -> Result<T, Error> {
    lang_c::driver::with_parser_stack(operation)
        .map_err(|error| Error::new(0, format!("unable to create parser worker thread: {error}")))
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

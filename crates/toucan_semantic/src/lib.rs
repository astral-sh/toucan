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
mod declaration_origins;
mod documentation_origins;
mod parameter_dependencies;
pub use declaration_origins::{DeclarationOrigin, DeclarationOrigins, DeclarationTarget};
pub use documentation_origins::{
    DocumentationDeclaration, DocumentationDeclarations, DocumentationTarget,
};
pub use parameter_dependencies::{ParameterTypeDependencies, ParameterTypeOccurrence};
mod tag_discovery;
pub use tag_discovery::{TagDiscoveries, TagDiscovery};
mod lexical_tags;
pub use lexical_tags::{TagLexicalOrigin, TagLexicalOrigins};
mod complex;
mod const_objects;
mod constant_query;
mod declspec;
mod dll_storage;
mod elementwise;
mod enums;
mod expression;
mod features;
mod floating;
mod fortified;
mod implicit_function;
mod initializer;
mod inline;
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
mod object_values;
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
pub use dll_storage::DllStorageClass;
pub use features::has_builtin;
pub use inline::{FunctionDefinitionKind, FunctionInlineFacts};
pub use ir::*;
pub use literals::{
    DecodedString, StringEncoding, decode_character_literal, decode_character_literal_with_profile,
    decode_string_literals,
};
pub use noescape::{ParameterContracts, ParameterContractsId};
pub use object_alignment::DeclarationAlignment;
pub use object_values::{ObjectOccurrence, ObjectValues};
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
    /// Retain file-scope declaration locations without retaining bodies or expressions.
    pub retain_declaration_origins: bool,
    /// Retain file object occurrences and checked scalar initializer values.
    pub retain_object_values: bool,
    /// Retain declaration starts, member locations, and containing declaration locations.
    pub retain_documentation_origins: bool,
    /// Retain source typedef dependencies erased by array-parameter adjustment.
    pub retain_parameter_type_dependencies: bool,
    /// Resource limits applied only when `retain_code` is enabled.
    pub limits: checked::Limits,
}

/// Owns declarations and optional checked code and declaration metadata.
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
/// discards all optional metadata before returning mutable data.
#[derive(Debug, serde::Serialize)]
pub struct Analysis {
    unit: TranslationUnit,
    checked: Option<checked::CheckedCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    declaration_origins: Option<Box<DeclarationOrigins>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    object_values: Option<Box<ObjectValues>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    documentation_origins: Option<Box<DocumentationDeclarations>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameter_type_dependencies: Option<Box<ParameterTypeDependencies>>,
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
    /// Returns source-ordered declarations when requested independently of checked code.
    pub fn declaration_origins(&self) -> Option<&DeclarationOrigins> {
        self.declaration_origins.as_deref()
    }
    /// File object occurrences, captured without retaining expressions or bodies.
    pub fn object_values(&self) -> Option<&ObjectValues> {
        self.object_values.as_deref()
    }
    /// Returns documentation attachment coordinates when requested.
    pub fn documentation_origins(&self) -> Option<&DocumentationDeclarations> {
        self.documentation_origins.as_deref()
    }
    /// Returns optional source dependencies without changing adjusted C types.
    pub fn parameter_type_dependencies(&self) -> Option<&ParameterTypeDependencies> {
        self.parameter_type_dependencies.as_deref()
    }

    /// Discards optional metadata, returning the owned declaration representation.
    pub fn into_unit(self) -> TranslationUnit {
        self.unit
    }
}

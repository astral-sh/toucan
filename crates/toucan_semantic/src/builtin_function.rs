//! Predefined functions that GNU also permits as ordinary function designators.

use serde::Serialize;
use toucan_target::Target;

use crate::{AllocationOperation, CallingConvention, FunctionType, Parameter, Type, TypeKind};

/// Source identity of a compiler builtin that can also denote an external function.
/// Clang requires direct calls to these names. GNU function addresses preserve the
/// symbol below; the prefetch symbol requires a definition from the eventual link.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BuiltinFunction {
    Allocation(AllocationOperation),
    Prefetch,
}

// Preserve the existing allocation-operation JSON spelling while adding the
// prefetch identity. No checked-code schema migration is needed for that case.
impl Serialize for BuiltinFunction {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Allocation(operation) => operation.serialize(serializer),
            Self::Prefetch => serializer.serialize_str("Prefetch"),
        }
    }
}

impl BuiltinFunction {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        if name == "__builtin_prefetch" {
            Some(Self::Prefetch)
        } else {
            AllocationOperation::from_name(name).map(Self::Allocation)
        }
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Self::Allocation(operation) => operation as usize,
            Self::Prefetch => 4,
        }
    }

    /// Default external symbol denoted by a GNU function address.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Allocation(operation) => operation.library_symbol(),
            Self::Prefetch => "__builtin_prefetch",
        }
    }

    pub(crate) fn source_name(self) -> &'static str {
        match self {
            Self::Allocation(AllocationOperation::Malloc) => "__builtin_malloc",
            Self::Allocation(AllocationOperation::Calloc) => "__builtin_calloc",
            Self::Allocation(AllocationOperation::Realloc) => "__builtin_realloc",
            Self::Allocation(AllocationOperation::Free) => "__builtin_free",
            Self::Prefetch => "__builtin_prefetch",
        }
    }

    pub(crate) fn signature(self, target: Target) -> FunctionType {
        match self {
            Self::Allocation(operation) => operation.signature(target),
            Self::Prefetch => FunctionType {
                return_type: Type::new(TypeKind::Void),
                parameters: vec![Parameter {
                    name: None,
                    ty: crate::prefetch::address_type(),
                }],
                noreturn: false,
                parameter_contracts: None,
                variadic: true,
                prototype: true,
                calling_convention: CallingConvention::C,
            },
        }
    }
}

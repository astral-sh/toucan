//! C11 atomic object types, separate from ordinary value qualification.

use serde::Serialize;
use toucan_target::{Compiler, Layout};

use crate::analyze::Analyzer;
use crate::{Error, Qualifiers, TranslationUnit, Type, TypeKind};

impl TranslationUnit {
    /// Returns the non-atomic value stored by an atomic type, resolving aliases
    /// without changing the type's storage layout or outer qualifiers.
    pub fn atomic_value<'a>(&'a self, ty: &'a Type) -> Result<Option<&'a Type>, Error> {
        let mut ty = ty;
        for _ in 0..128 {
            match &ty.kind {
                TypeKind::Atomic(value) => return Ok(Some(value)),
                TypeKind::Typedef(name) => {
                    ty = self
                        .typedefs
                        .get(name)
                        .ok_or_else(|| Error::new(0, format!("unknown typedef `{name}`")))?
                }
                _ => return Ok(None),
            }
        }
        Err(Error::new(
            0,
            "atomic typedef resolution exceeds the 128-level limit",
        ))
    }
}

impl Analyzer {
    /// Applies either the constructor spelling or an idempotent atomic qualifier.
    pub(crate) fn atomic_type(
        &self,
        ty: Type,
        specifier: bool,
        offset: usize,
    ) -> Result<Type, Error> {
        atomic_type_depth(&ty, 0, &mut 0, offset)?;
        let qualifiers = self.unit.qualifiers(&ty)?;
        let resolved = self.unit.resolve(&ty)?;
        if specifier
            && (qualifiers != Qualifiers::default() || matches!(resolved.kind, TypeKind::Atomic(_)))
        {
            return Err(Error::new(
                offset,
                "_Atomic(type-name) requires an unqualified non-atomic type",
            ));
        }
        if matches!(
            resolved.kind,
            TypeKind::Array { .. }
                | TypeKind::VariableArray { .. }
                | TypeKind::Function(_)
                | TypeKind::Sve(_)
        ) {
            return Err(Error::new(
                offset,
                "atomic types cannot contain an array, function, or sizeless value",
            ));
        }
        if !self.gnu_sync_profile() && !self.is_complete_object(&ty, 0)? {
            return Err(Error::new(
                offset,
                "this Clang profile requires a complete atomic value type",
            ));
        }
        if matches!(resolved.kind, TypeKind::Atomic(_)) {
            return Ok(ty);
        }
        let value = if qualifiers == Qualifiers::default() {
            ty
        } else {
            self.unqualified(&ty)?
        };
        let mut atomic = Type::new(TypeKind::Atomic(Box::new(value)));
        atomic.qualifiers = qualifiers;
        Ok(atomic)
    }

    /// Loads an atomic lvalue into its ordinary value type. The caller retains
    /// the source access separately; removing a type wrapper is not a load plan.
    pub(crate) fn atomic_value_type(&self, ty: &Type) -> Result<Type, Error> {
        match self.unit.atomic_value(ty)? {
            Some(value) => self.unqualified(value),
            None => self.unqualified(ty),
        }
    }
}

/// The shipped compiler profiles promote small atomic storage differently.
/// This changes the wrapper only, leaving canonical record layouts intact.
pub(crate) fn atomic_layout(compiler: Compiler, mut inner: Layout) -> Result<Layout, Error> {
    let gnu = compiler == Compiler::Gnu;
    let size = inner.size_bits;
    let (size, alignment) = if gnu {
        (
            size,
            if size.is_power_of_two() && size <= 128 {
                inner.alignment_bits.max(size)
            } else {
                inner.alignment_bits
            },
        )
    } else if size == 0 {
        (8, inner.alignment_bits)
    } else if size <= 128 {
        let size = size
            .checked_next_power_of_two()
            .ok_or_else(|| Error::new(0, "atomic storage size overflows"))?;
        (size, size)
    } else {
        (size, inner.alignment_bits)
    };
    inner.size_bits = size;
    inner.alignment_bits = alignment;
    inner.field_alignment_bits = alignment;
    inner.required_alignment_bits = 8;
    inner.fields.clear();
    Ok(inner)
}

/// A C operator's atomic write, always sequentially consistent. Initialization
/// is separate and carries no atomic store. The original operand/operator and
/// computation type describe the value; a read-modify-write evaluates its place
/// once and performs one atomic update, not independent load/store operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum AtomicAccess {
    Store,
    ReadModifyWrite,
}

// Local typedefs can embed owned derived types. Check this before adding a new
// atomic wrapper, so ordinary analysis and retained analysis share the bound.
fn atomic_type_depth(
    ty: &Type,
    depth: usize,
    nodes: &mut usize,
    offset: usize,
) -> Result<(), Error> {
    *nodes += 1;
    if depth >= 127 || *nodes > 65_536 {
        return Err(Error::new(
            offset,
            "atomic value type exceeds the 128-level or 65536-node limit",
        ));
    }
    match &ty.kind {
        TypeKind::Pointer(value)
        | TypeKind::Atomic(value)
        | TypeKind::Array { element: value, .. }
        | TypeKind::VariableArray { element: value }
        | TypeKind::Vector { element: value, .. } => {
            atomic_type_depth(value, depth + 1, nodes, offset)?
        }
        TypeKind::Function(function) => {
            atomic_type_depth(&function.return_type, depth + 1, nodes, offset)?;
            for parameter in &function.parameters {
                atomic_type_depth(&parameter.ty, depth + 1, nodes, offset)?;
            }
        }
        _ => {}
    }
    Ok(())
}

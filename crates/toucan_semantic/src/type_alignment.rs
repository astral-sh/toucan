//! Compact alignment snapshots and the type sugar needed by Clang conversions.

use std::num::NonZeroU32;

use rustc_hash::FxHashMap;
use serde::{Serialize, Serializer, ser::SerializeMap};

/// An owner-local index into [`crate::TranslationUnit::alignment_origins`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AlignmentOriginId(NonZeroU32);

impl AlignmentOriginId {
    /// Constructs an index. The owning translation unit validates its existence.
    pub fn new(index: usize) -> Option<Self> {
        let value = u32::try_from(index).ok()?.checked_add(1)?;
        (value < (1 << 27)).then_some(())?;
        NonZeroU32::new(value).map(Self)
    }
    pub fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }
}
impl Serialize for AlignmentOriginId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.0.get() - 1)
    }
}

/// A typedef alignment snapshot, with optional owner-local type ancestry.
///
/// An origin can be present without an alignment override: a later declaration
/// may add alignment to the same typedef. The byte value always describes this
/// occurrence, rather than being replaced by a later declaration's value.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TypeAlignment(u32);

impl TypeAlignment {
    /// Constructs a byte alignment without claiming a typedef identity.
    pub fn new(bytes: u32) -> Option<Self> {
        if !bytes.is_power_of_two() || bytes > (1 << 28) {
            return None;
        }
        Some(Self(bytes.trailing_zeros() + 1))
    }
    pub fn from_bytes(bytes: Option<NonZeroU32>) -> Result<Self, crate::Error> {
        match bytes {
            None => Ok(Self::default()),
            Some(bytes) => Self::new(bytes.get()).ok_or_else(|| {
                crate::Error::new(0, "typedef alignment must be a supported power of two")
            }),
        }
    }
    /// The optional explicit alignment in bytes.
    pub fn bytes(self) -> Option<NonZeroU32> {
        let exponent = self.0 & 31;
        (exponent != 0).then(|| NonZeroU32::new(1 << (exponent - 1)).unwrap())
    }
    pub fn origin(self) -> Option<AlignmentOriginId> {
        NonZeroU32::new(self.0 >> 5).map(AlignmentOriginId)
    }
    /// Attaches a validated-in-owner ancestry index while preserving the byte value.
    pub fn with_origin(self, origin: AlignmentOriginId) -> Self {
        Self((self.0 & 31) | (origin.0.get() << 5))
    }
    pub(crate) fn has_metadata(self) -> bool {
        self.0 != 0
    }
}
impl std::fmt::Debug for TypeAlignment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.origin().is_none() {
            self.bytes().fmt(f)
        } else {
            f.debug_struct("TypeAlignment")
                .field("bytes", &self.bytes())
                .field("origin", &self.origin())
                .finish()
        }
    }
}
impl Serialize for TypeAlignment {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        if let Some(bytes) = self.bytes() {
            map.serialize_entry("alignment", &bytes)?;
        }
        if let Some(origin) = self.origin() {
            map.serialize_entry("alignment_origin", &origin)?;
        }
        map.end()
    }
}

/// A preserved layer of type sugar that can affect an expression's alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[non_exhaustive]
pub enum AlignmentOriginKind {
    /// Typedef declarations share a canonical ID while retaining separate values.
    Typedef {
        canonical: AlignmentOriginId,
        previous: Option<AlignmentOriginId>,
    },
    TypeOfType,
    /// Distinct written typeof expressions do not acquire a shared identity.
    TypeOfExpression,
}

/// Immutable ancestry used by Clang's common-sugared-type conversion rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct AlignmentOrigin {
    pub kind: AlignmentOriginKind,
    pub parent: Option<AlignmentOriginId>,
    pub alignment: Option<NonZeroU32>,
}

use crate::{Error, TranslationUnit, Type, TypeKind, analyze::Analyzer};
use std::collections::BTreeSet;

const MAX_ORIGINS: usize = 65_536;

/// Caches source occurrences separately from synthesized common type layers.
#[derive(Default)]
pub(crate) struct Registry {
    pub(crate) candidates: BTreeSet<String>,
    sources: FxHashMap<(usize, u8), AlignmentOriginId>,
    common: FxHashMap<AlignmentOrigin, AlignmentOriginId>,
}

impl TranslationUnit {
    pub fn alignment_origin(&self, id: AlignmentOriginId) -> Result<&AlignmentOrigin, Error> {
        self.alignment_origins
            .get(id.index())
            .ok_or_else(|| Error::new(0, "invalid type-alignment origin ID"))
    }

    /// Validates sparse ancestry and all structurally owned type references.
    pub fn validate_alignment_origins(&self) -> Result<(), Error> {
        self.validate_alignment_origin_rows()?;
        let mut work = 0;
        for ty in self
            .declarations
            .iter()
            .map(|d| &d.ty)
            .chain(self.typedefs.values())
            .chain(
                self.records
                    .iter()
                    .flat_map(|r| r.fields.iter().flatten().map(|f| &f.ty)),
            )
        {
            self.validate_alignment_type(ty, 0, &mut work)?;
        }
        Ok(())
    }
    pub(crate) fn validate_alignment_origin_rows(&self) -> Result<(), Error> {
        if self.alignment_origins.len() > MAX_ORIGINS {
            return Err(Error::new(
                0,
                "type-alignment origins exceed the 65536-entry limit",
            ));
        }
        for (index, origin) in self.alignment_origins.iter().enumerate() {
            TypeAlignment::from_bytes(origin.alignment)?;
            if origin.parent.is_some_and(|parent| parent.index() >= index) {
                return Err(Error::new(
                    0,
                    "type-alignment parent must precede its child",
                ));
            }
            if let AlignmentOriginKind::Typedef {
                canonical,
                previous,
            } = origin.kind
            {
                if canonical.index() > index || previous.is_some_and(|p| p.index() >= index) {
                    return Err(Error::new(
                        0,
                        "invalid typedef alignment redeclaration links",
                    ));
                }
                if !matches!(self.alignment_origin(canonical)?.kind,
                    AlignmentOriginKind::Typedef { canonical: first, previous: None } if first == canonical)
                {
                    return Err(Error::new(
                        0,
                        "typedef alignment canonical ID must name its first declaration",
                    ));
                }
                if let Some(previous) = previous
                    && !matches!(self.alignment_origin(previous)?.kind,
                        AlignmentOriginKind::Typedef { canonical: first, .. } if first == canonical)
                {
                    return Err(Error::new(
                        0,
                        "typedef alignment redeclarations must share a canonical ID",
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_alignment_type(
        &self,
        ty: &Type,
        depth: usize,
        work: &mut usize,
    ) -> Result<(), Error> {
        *work += 1;
        if depth >= 128 || *work > 4_000_000 {
            return Err(Error::new(
                0,
                "type-alignment validation nesting or work limit exceeded",
            ));
        }
        self.validate_alignment_snapshot(ty.alignment)?;
        match &ty.kind {
            TypeKind::Pointer(t)
            | TypeKind::Atomic(t)
            | TypeKind::Vector { element: t, .. }
            | TypeKind::Array { element: t, .. }
            | TypeKind::VariableArray { element: t, .. } => {
                self.validate_alignment_type(t, depth + 1, work)?
            }
            TypeKind::Function(f) => {
                self.validate_alignment_type(&f.return_type, depth + 1, work)?;
                for p in &f.parameters {
                    self.validate_alignment_type(&p.ty, depth + 1, work)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn validate_alignment_snapshot(&self, value: TypeAlignment) -> Result<(), Error> {
        if let Some(origin) = value.origin()
            && self.alignment_origin(origin)?.alignment != value.bytes()
        {
            return Err(Error::new(
                0,
                "type-alignment snapshot disagrees with its origin",
            ));
        }
        Ok(())
    }
}

impl Analyzer {
    fn push_alignment_origin(
        &mut self,
        origin: AlignmentOrigin,
        offset: usize,
    ) -> Result<AlignmentOriginId, Error> {
        if self.unit.alignment_origins.len() >= MAX_ORIGINS {
            return Err(Error::new(
                offset,
                "type-alignment origins exceed the 65536-entry limit",
            ));
        }
        if let Some(checked) = &mut self.checked {
            checked.charge_alignment_origin(offset)?;
        }
        let id = AlignmentOriginId::new(self.unit.alignment_origins.len()).unwrap();
        self.unit.alignment_origins.push(origin);
        Ok(id)
    }
    fn common_alignment_origin(
        &mut self,
        origin: AlignmentOrigin,
        offset: usize,
    ) -> Result<AlignmentOriginId, Error> {
        if let Some(id) = self
            .alignment_registry
            .as_ref()
            .and_then(|r| r.common.get(&origin))
        {
            return Ok(*id);
        }
        let id = self.push_alignment_origin(origin, offset)?;
        self.alignment_registry
            .get_or_insert_with(Default::default)
            .common
            .insert(origin, id);
        Ok(id)
    }

    pub(crate) fn retain_typedef_alignment(
        &mut self,
        name: &str,
        ty: &mut Type,
        previous: Option<TypeAlignment>,
        inherited: TypeAlignment,
        explicit: bool,
        offset: usize,
    ) -> Result<(), Error> {
        if self.unit.compiler != toucan_target::Compiler::Clang {
            return Ok(());
        }
        if !explicit
            && inherited.origin().is_none()
            && !self
                .alignment_registry
                .as_ref()
                .is_some_and(|r| r.candidates.contains(name))
        {
            return Ok(());
        }
        if let Some(id) = self
            .alignment_registry
            .as_ref()
            .and_then(|r| r.sources.get(&(offset, 0)))
        {
            ty.alignment = TypeAlignment::from_bytes(self.unit.alignment_origin(*id)?.alignment)?
                .with_origin(*id);
            return Ok(());
        }
        let next = AlignmentOriginId::new(self.unit.alignment_origins.len())
            .ok_or_else(|| Error::new(offset, "type-alignment origin ID range exhausted"))?;
        let previous = previous.and_then(TypeAlignment::origin);
        let canonical = match previous {
            Some(id) => match self.unit.alignment_origin(id)?.kind {
                AlignmentOriginKind::Typedef { canonical, .. } => canonical,
                _ => {
                    return Err(Error::new(
                        offset,
                        "typedef redeclaration lost its alignment identity",
                    ));
                }
            },
            None => next,
        };
        let origin = AlignmentOrigin {
            kind: AlignmentOriginKind::Typedef {
                canonical,
                previous,
            },
            parent: inherited.origin(),
            alignment: ty.alignment.bytes(),
        };
        let id = self.push_alignment_origin(origin, offset)?;
        self.alignment_registry
            .get_or_insert_with(Default::default)
            .sources
            .insert((offset, 0), id);
        ty.alignment = ty.alignment.with_origin(id);
        Ok(())
    }

    pub(crate) fn retain_typeof_alignment(
        &mut self,
        ty: &mut Type,
        expression: bool,
        offset: usize,
    ) -> Result<(), Error> {
        let inherited = self.unit.typedef_alignment_metadata(ty)?;
        if self.unit.compiler != toucan_target::Compiler::Clang || inherited.origin().is_none() {
            return Ok(());
        }
        let key = (offset, if expression { 2 } else { 1 });
        let id = match self
            .alignment_registry
            .as_ref()
            .and_then(|r| r.sources.get(&key))
            .copied()
        {
            Some(id) => id,
            None => {
                let kind = if expression {
                    AlignmentOriginKind::TypeOfExpression
                } else {
                    AlignmentOriginKind::TypeOfType
                };
                let id = self.push_alignment_origin(
                    AlignmentOrigin {
                        kind,
                        parent: inherited.origin(),
                        alignment: inherited.bytes(),
                    },
                    offset,
                )?;
                self.alignment_registry
                    .get_or_insert_with(Default::default)
                    .sources
                    .insert(key, id);
                id
            }
        };
        ty.alignment = inherited.with_origin(id);
        Ok(())
    }

    pub(crate) fn common_type_alignment(
        &mut self,
        a: TypeAlignment,
        b: TypeAlignment,
        offset: usize,
    ) -> Result<TypeAlignment, Error> {
        fn path(
            unit: &TranslationUnit,
            mut id: Option<AlignmentOriginId>,
            out: &mut [Option<AlignmentOriginId>; 128],
        ) -> Result<usize, Error> {
            let mut len = 0;
            while let Some(current) = id {
                if len == out.len() {
                    return Err(Error::new(0, "type-alignment ancestry exceeds 128 levels"));
                }
                out[len] = Some(current);
                len += 1;
                id = unit.alignment_origin(current)?.parent;
            }
            Ok(len)
        }
        for value in [a, b] {
            self.unit.validate_alignment_snapshot(value)?;
            if value.bytes().is_some() && value.origin().is_none() {
                return Err(Error::new(
                    offset,
                    "aligned arithmetic requires retained typedef ancestry",
                ));
            }
        }
        if a == b {
            return Ok(a);
        }
        let (mut left, mut right) = ([None; 128], [None; 128]);
        let alen = path(&self.unit, a.origin(), &mut left)?;
        let blen = path(&self.unit, b.origin(), &mut right)?;
        let mut result = TypeAlignment::default();
        for (a, b) in left[..alen].iter().rev().zip(right[..blen].iter().rev()) {
            let (a, b) = (a.unwrap(), b.unwrap());
            let (x, y) = (
                *self.unit.alignment_origin(a)?,
                *self.unit.alignment_origin(b)?,
            );
            if a == b {
                result = TypeAlignment::from_bytes(x.alignment)?.with_origin(a);
                continue;
            }
            let (kind, bytes) = match (x.kind, y.kind) {
                (
                    AlignmentOriginKind::Typedef { canonical: x, .. },
                    AlignmentOriginKind::Typedef { canonical: y, .. },
                ) if x == y => (
                    AlignmentOriginKind::Typedef {
                        canonical: x,
                        previous: None,
                    },
                    self.unit
                        .alignment_origin(a)?
                        .alignment
                        .max(self.unit.alignment_origin(b)?.alignment),
                ),
                (AlignmentOriginKind::TypeOfType, AlignmentOriginKind::TypeOfType) => {
                    (AlignmentOriginKind::TypeOfType, result.bytes())
                }
                _ => break,
            };
            let id = self.common_alignment_origin(
                AlignmentOrigin {
                    kind,
                    parent: result.origin(),
                    alignment: bytes,
                },
                offset,
            )?;
            result = TypeAlignment::from_bytes(bytes)?.with_origin(id);
        }
        Ok(result)
    }
}

impl Analyzer {
    /// Object redeclarations keep newly written sugar while retaining composite
    /// array bounds, prototypes and parameter contracts.
    pub(crate) fn object_alignment_sugar(
        &self,
        composite: &mut Type,
        written: &Type,
    ) -> Result<(), Error> {
        if self.unit.compiler != toucan_target::Compiler::Clang
            || self.unit.alignment_origins.is_empty()
        {
            return Ok(());
        }
        fn project(
            unit: &TranslationUnit,
            out: &mut Type,
            input: &Type,
            depth: usize,
        ) -> Result<(), Error> {
            if depth >= 128 {
                return Err(Error::new(
                    0,
                    "object type-alignment projection exceeds 128 levels",
                ));
            }
            let alignment = unit.typedef_alignment_metadata(input)?;
            if matches!(out.kind, TypeKind::Typedef(_)) {
                let qualifiers = unit.qualifiers(out)?;
                *out = unit.resolve(out)?.clone();
                out.qualifiers = qualifiers;
            }
            out.alignment = alignment;
            match (&mut out.kind, &unit.resolve(input)?.kind) {
                (TypeKind::Pointer(a), TypeKind::Pointer(b))
                | (TypeKind::Atomic(a), TypeKind::Atomic(b))
                | (TypeKind::Array { element: a, .. }, TypeKind::Array { element: b, .. })
                | (
                    TypeKind::VariableArray { element: a, .. },
                    TypeKind::VariableArray { element: b, .. },
                )
                | (
                    TypeKind::Array { element: a, .. },
                    TypeKind::VariableArray { element: b, .. },
                )
                | (
                    TypeKind::VariableArray { element: a, .. },
                    TypeKind::Array { element: b, .. },
                ) => project(unit, a, b, depth + 1)?,
                (TypeKind::Function(a), TypeKind::Function(b)) => {
                    project(unit, &mut a.return_type, &b.return_type, depth + 1)?;
                    for (a, b) in a.parameters.iter_mut().zip(&b.parameters) {
                        project(unit, &mut a.ty, &b.ty, depth + 1)?;
                    }
                }
                _ => {}
            }
            Ok(())
        }
        project(&self.unit, composite, written, 0)
    }
}

impl Analyzer {
    pub(crate) fn promoted_integer_type(
        &self,
        info: &crate::expression::ExpressionInfo,
        offset: usize,
    ) -> Result<Type, Error> {
        let ty = self.converted_type(info, offset)?;
        let promoted = crate::integer::integer_to_type(self.promoted_integer(info, offset)?);
        Ok(if ty.kind == promoted.kind {
            ty
        } else {
            promoted
        })
    }
    pub(crate) fn integer_arithmetic_operand_type(
        &self,
        info: &crate::expression::ExpressionInfo,
        result: &Type,
        offset: usize,
    ) -> Result<Type, Error> {
        let ty = if matches!(result.kind, TypeKind::Integer(_)) {
            self.promoted_integer_type(info, offset)?
        } else {
            self.converted_type(info, offset)?
        };
        Ok(if ty.kind == result.kind {
            ty
        } else {
            result.clone()
        })
    }
}

impl Analyzer {
    pub(crate) fn integer_arithmetic_alignment(
        &mut self,
        left: &crate::expression::ExpressionInfo,
        right: &crate::expression::ExpressionInfo,
        result: &Type,
        offset: usize,
    ) -> Result<TypeAlignment, Error> {
        if self.unit.compiler != toucan_target::Compiler::Clang
            || self.unit.alignment_origins.is_empty()
        {
            return Ok(TypeAlignment::default());
        }
        let a = self.promoted_integer_type(left, offset)?;
        let b = self.promoted_integer_type(right, offset)?;
        if a.kind == b.kind {
            self.common_type_alignment(a.alignment, b.alignment, offset)
        } else if a.kind == result.kind {
            Ok(a.alignment)
        } else if b.kind == result.kind {
            Ok(b.alignment)
        } else {
            Ok(TypeAlignment::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_retains_bytes_and_optional_origin_without_type_growth() {
        assert_eq!(std::mem::size_of::<TypeAlignment>(), 4);
        assert_eq!(std::mem::size_of::<crate::Type>(), 40);
        for exponent in 0..=28 {
            let bytes = 1 << exponent;
            let value = TypeAlignment::new(bytes).unwrap();
            assert_eq!(value.bytes().unwrap().get(), bytes);
            assert_eq!(value.origin(), None);
            for index in [0, 31, 65_535, (1 << 27) - 2] {
                let origin = AlignmentOriginId::new(index).unwrap();
                let attached = value.with_origin(origin);
                assert_eq!(attached.bytes(), value.bytes());
                assert_eq!(attached.origin().unwrap().index(), index);
            }
        }
        assert!(TypeAlignment::new(0).is_none());
        assert!(TypeAlignment::new(3).is_none());
        assert!(TypeAlignment::new(1 << 29).is_none());
        assert!(AlignmentOriginId::new((1 << 27) - 1).is_none());
        let empty = TypeAlignment::default().with_origin(AlignmentOriginId::new(0).unwrap());
        assert!(empty.bytes().is_none());
        assert!(empty.origin().is_some());
    }
}

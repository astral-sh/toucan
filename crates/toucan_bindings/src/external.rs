//! Caller-owned external type definitions and their C layout contracts.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use serde::Serialize;
use toucan_semantic::{DeclarationKind, IntegerKind, RecordKind, Type, TypeKind};

use super::{Emitter, Error, check_depth};

/// The C namespace and kind of an omitted type definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ExternalTypeKind {
    Typedef,
    Struct,
    Union,
    Enum,
}

/// A caller-owned Rust replacement. C layout is known independently of whether
/// the caller's Rust definition satisfies it; equal layout is not an ABI proof.
#[derive(Debug, Clone, Serialize)]
pub struct ExternalType {
    pub c_name: String,
    pub rust_name: String,
    pub kind: ExternalTypeKind,
    /// Reached while collecting an emitted declaration or type alias.
    pub referenced: bool,
    /// A generated value, field, or extern object needs matching complete storage.
    /// Pointer-only opaque uses do not require the Rust pointee layout to match.
    pub layout_required: bool,
    pub size_bytes: Option<u64>,
    pub alignment_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Key {
    Typedef(String),
    Record(usize),
    Enum(usize),
}

#[derive(Default)]
pub(super) struct ExternalTypes {
    pub(super) types: BTreeMap<Key, ExternalType>,
    pub(super) layout_aliases: BTreeSet<String>,
    records: Vec<Option<bool>>,
    enumerators: BTreeSet<String>,
}

impl Emitter<'_> {
    pub(super) fn external_key(&self, ty: &Type) -> Result<Option<Key>, Error> {
        if self.options.blocklist_types.is_empty() {
            return Ok(None);
        }
        let anonymous = match &ty.kind {
            TypeKind::Typedef(name) => {
                return Ok(self
                    .options
                    .blocks_type(name)
                    .then(|| Key::Typedef(name.clone())));
            }
            TypeKind::Record(id) => {
                let record = self
                    .unit
                    .records
                    .get(*id)
                    .ok_or_else(|| Error("invalid record identity".into()))?;
                if let Some(name) = &record.name {
                    return Ok(self.options.blocks_type(name).then_some(Key::Record(*id)));
                }
                true
            }
            TypeKind::Enum(id) => {
                let enumeration = self
                    .unit
                    .enums
                    .get(*id)
                    .ok_or_else(|| Error("invalid enum identity".into()))?;
                if let Some(name) = &enumeration.name {
                    return Ok(self.options.blocks_type(name).then_some(Key::Enum(*id)));
                }
                true
            }
            _ => false,
        };
        // The first typedef gives an anonymous definition its public name.
        // Later aliases refer to that same external definition, not a new type.
        if anonymous {
            for declaration in &self.unit.declarations {
                if declaration.kind == DeclarationKind::Typedef
                    && self.unit.resolve(&declaration.ty)?.kind == ty.kind
                {
                    return Ok(self
                        .options
                        .blocks_type(&declaration.name)
                        .then(|| Key::Typedef(declaration.name.clone())));
                }
            }
        }
        Ok(None)
    }

    fn external_identity(
        &self,
        key: &Key,
    ) -> Result<(String, String, ExternalTypeKind, Type), Error> {
        Ok(match key {
            Key::Typedef(name) => (
                name.clone(),
                self.names.identifier(name)?,
                ExternalTypeKind::Typedef,
                Type::new(TypeKind::Typedef(name.clone())),
            ),
            Key::Record(id) => {
                let record = &self.unit.records[*id];
                (
                    record.name.clone().expect("blocked record is named"),
                    self.record_name(*id)?,
                    if record.kind == RecordKind::Struct {
                        ExternalTypeKind::Struct
                    } else {
                        ExternalTypeKind::Union
                    },
                    Type::new(TypeKind::Record(*id)),
                )
            }
            Key::Enum(id) => (
                self.unit.enums[*id]
                    .name
                    .clone()
                    .expect("blocked enum is named"),
                self.enum_name(*id)?,
                ExternalTypeKind::Enum,
                Type::new(TypeKind::Enum(*id)),
            ),
        })
    }

    pub(super) fn external_name(&self, ty: &Type) -> Result<Option<String>, Error> {
        self.external_key(ty)?
            .map(|key| self.external_identity(&key).map(|(_, name, _, _)| name))
            .transpose()
    }

    pub(super) fn register_external(
        &mut self,
        ty: &Type,
        referenced: bool,
        layout_required: bool,
    ) -> Result<bool, Error> {
        let Some(key) = self.external_key(ty)? else {
            return Ok(false);
        };
        if let Some(existing) = self.external.types.get(&key)
            && (!referenced || existing.referenced)
            && (!layout_required || existing.layout_required)
        {
            return Ok(true);
        }
        let (c_name, rust_name, kind, ty) = self.external_identity(&key)?;
        if referenced {
            self.validate_external_storage(&ty, &mut BTreeSet::new(), 0, layout_required)?;
        }
        let (size_bytes, alignment_bytes) = match &self.unit.resolve(&ty)?.kind {
            TypeKind::Record(id) if self.unit.records[*id].fields.is_none() => (None, None),
            TypeKind::Array { length: None, .. } => (None, Some(self.unit.alignment(&ty)?)),
            TypeKind::Void
            | TypeKind::Function(_)
            | TypeKind::Sve(_)
            | TypeKind::VariableArray { .. } => (None, None),
            _ => {
                let layout = self.unit.layout(&ty)?;
                if layout_required
                    && (layout.alignment_bits != layout.field_alignment_bits
                        || self.unit.alignment(&ty)? != layout.alignment_bytes())
                {
                    return Err(Error(format!(
                        "external type `{c_name}` has distinct C object/field alignment that one Rust type cannot express"
                    )));
                }
                (Some(layout.size_bytes()), Some(layout.alignment_bytes()))
            }
        };
        self.external.types.insert(
            key,
            ExternalType {
                c_name,
                rust_name,
                kind,
                referenced,
                layout_required,
                size_bytes,
                alignment_bytes,
            },
        );
        Ok(true)
    }

    fn validate_external_storage(
        &self,
        ty: &Type,
        active: &mut BTreeSet<(usize, bool)>,
        depth: usize,
        layout_required: bool,
    ) -> Result<(), Error> {
        check_depth(depth)?;
        match &self.unit.resolve(ty)?.kind {
            TypeKind::Complex(_) if layout_required => return Err(crate::complex::storage_error()),
            TypeKind::Integer(IntegerKind::Int128 | IntegerKind::UnsignedInt128)
                if layout_required =>
            {
                self.check_128_bit_abi()?
            }
            TypeKind::Enum(id) if layout_required && self.enum_integer(*id)?.0 == 128 => {
                self.check_128_bit_abi()?
            }
            TypeKind::Sve(_) if layout_required => return Err(Error(
                "sizeless SVE types have no stable Rust representation, including external types"
                    .into(),
            )),
            TypeKind::VariableArray { .. } if layout_required => {
                return Err(Error(
                    "variable-length arrays have no fixed external Rust representation".into(),
                ));
            }
            TypeKind::Pointer(inner) => {
                self.validate_external_storage(inner, active, depth + 1, false)?
            }
            TypeKind::Atomic(inner)
            | TypeKind::Array { element: inner, .. }
            | TypeKind::Vector { element: inner, .. } => {
                self.validate_external_storage(inner, active, depth + 1, layout_required)?
            }
            TypeKind::Record(id) if active.insert((*id, layout_required)) => {
                if let Some(fields) = &self.unit.records[*id].fields {
                    for field in fields {
                        self.validate_external_storage(
                            &field.ty,
                            active,
                            depth + 1,
                            layout_required,
                        )?;
                    }
                }
            }
            TypeKind::Function(function) => {
                self.check_function_at(function, depth + 1)?;
                self.validate_external_storage(&function.return_type, active, depth + 1, true)?;
                for parameter in &function.parameters {
                    self.validate_external_storage(&parameter.ty, active, depth + 1, true)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn blocked_enumerator(&self, name: &str) -> bool {
        self.external.enumerators.contains(name)
    }

    pub(super) fn emit_external_assertions(&self, source: &mut String) -> Result<(), Error> {
        for external in self
            .external
            .types
            .values()
            .filter(|external| external.layout_required)
        {
            let name = &external.rust_name;
            if external.size_bytes.is_none() && external.alignment_bytes.is_none() {
                continue;
            }
            source.push_str("// Caller-owned type: size/alignment checks do not prove its validity or call ABI.\nconst _: () = {\n");
            if let Some(size) = external.size_bytes {
                writeln!(
                    source,
                    "    assert!(::core::mem::size_of::<{name}>() == {size});"
                )
                .unwrap();
            }
            if let Some(alignment) = external.alignment_bytes {
                writeln!(
                    source,
                    "    assert!(::core::mem::align_of::<{name}>() == {alignment});"
                )
                .unwrap();
            }
            source.push_str("};\n\n");
        }
        Ok(())
    }

    pub(super) fn prepare_external_records(&mut self) -> Result<(), Error> {
        if self.options.blocklist_types.is_empty() {
            return Ok(());
        }
        for id in 0..self.unit.enums.len() {
            if self.unit.enums[id].scope == toucan_semantic::Scope::File
                && self.external_key(&Type::new(TypeKind::Enum(id)))?.is_some()
            {
                self.register_external(&Type::new(TypeKind::Enum(id)), false, false)?;
                self.external.enumerators.extend(
                    self.unit.enums[id]
                        .variants
                        .iter()
                        .map(|variant| variant.name.clone()),
                );
            }
        }
        let mut cache = vec![None; self.unit.records.len()];
        let mut active = vec![false; self.unit.records.len()];
        for &id in &self.records {
            self.external_containment(
                &Type::new(TypeKind::Record(id)),
                &mut cache,
                &mut active,
                0,
            )?;
        }
        self.external.records = cache;
        Ok(())
    }

    fn external_containment(
        &self,
        ty: &Type,
        cache: &mut [Option<bool>],
        active: &mut [bool],
        depth: usize,
    ) -> Result<bool, Error> {
        check_depth(depth)?;
        if self.external_key(ty)?.is_some() {
            return Ok(true);
        }
        Ok(match &ty.kind {
            TypeKind::Typedef(name) => self.external_containment(
                self.unit
                    .typedefs
                    .get(name)
                    .ok_or_else(|| Error(format!("unknown typedef `{name}`")))?,
                cache,
                active,
                depth + 1,
            )?,
            TypeKind::Array { element, .. } => {
                self.external_containment(element, cache, active, depth + 1)?
            }
            TypeKind::Record(id) => {
                if let Some(found) = cache[*id] {
                    return Ok(found);
                }
                if active[*id] {
                    return Err(Error("recursive record by value".into()));
                }
                active[*id] = true;
                let mut found = false;
                if let Some(fields) = &self.unit.records[*id].fields {
                    for field in fields {
                        found |= self.external_containment(&field.ty, cache, active, depth + 1)?;
                    }
                }
                active[*id] = false;
                cache[*id] = Some(found);
                found
            }
            _ => false,
        })
    }

    pub(super) fn contains_external_storage(&self, ty: &Type, depth: usize) -> Result<bool, Error> {
        if self.options.blocklist_types.is_empty() {
            return Ok(false);
        }
        check_depth(depth)?;
        if self.external_key(ty)?.is_some() {
            return Ok(true);
        }
        Ok(match &ty.kind {
            TypeKind::Typedef(name) => self.contains_external_storage(
                self.unit
                    .typedefs
                    .get(name)
                    .ok_or_else(|| Error(format!("unknown typedef `{name}`")))?,
                depth + 1,
            )?,
            TypeKind::Array { element, .. } => {
                self.contains_external_storage(element, depth + 1)?
            }
            TypeKind::Record(id) => self
                .external
                .records
                .get(*id)
                .copied()
                .flatten()
                .unwrap_or(false),
            _ => false,
        })
    }
}

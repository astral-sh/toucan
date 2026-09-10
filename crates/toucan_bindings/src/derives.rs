//! Derive eligibility follows emitted Rust storage, including zero validity.

use std::borrow::Cow;
use std::fmt::Write;

use toucan_semantic::{CallingConvention, RecordKind, Type, TypeKind};

use crate::{Emitter, Error, check_depth};

/// Requested traits for generated records and enum representations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeriveOptions {
    /// Derive `Copy` and `Clone` where all storage is copyable. Rust enums retain
    /// `Clone` when this is false, matching bindgen's enum compatibility policy.
    pub copy: bool,
    /// Request or suppress `Debug`. `None` keeps the default: enabled for Rust
    /// enums and disabled for records and storage helpers.
    pub debug: Option<bool>,
    /// Provide a zero-initializing `Default` when zero is a valid representation.
    /// No default is provided for Rust enums, incomplete records, atomic storage,
    /// caller-owned types, or structs containing a nonzero Rust enum value.
    pub default: bool,
    /// Derive `PartialEq` for comparable fields. Floating fields can support
    /// `PartialEq` without supporting `Eq`.
    pub partial_eq: bool,
    /// Derive `Eq` where supported. Also requests its required `PartialEq`.
    /// Rust enums retain both equality traits independently of this option.
    pub eq: bool,
}

impl Default for DeriveOptions {
    fn default() -> Self {
        Self {
            copy: true,
            debug: None,
            default: false,
            partial_eq: false,
            eq: false,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Traits {
    copy: bool,
    debug: bool,
    partial_eq: bool,
    eq: bool,
    zeroable: bool,
}

impl Traits {
    const ALL: Self = Self {
        copy: true,
        debug: true,
        partial_eq: true,
        eq: true,
        zeroable: true,
    };

    fn intersect(&mut self, other: Self) {
        self.copy &= other.copy;
        self.debug &= other.debug;
        self.partial_eq &= other.partial_eq;
        self.eq &= other.eq;
        self.zeroable &= other.zeroable;
    }

    fn attributes(self) -> String {
        let mut names = Vec::new();
        if self.debug {
            names.push("Debug");
        }
        if self.copy {
            names.extend(["Clone", "Copy"]);
        }
        if self.partial_eq {
            names.push("PartialEq");
        }
        if self.eq {
            names.push("Eq");
        }
        attributes(&names)
    }
}

#[derive(Default)]
pub(super) struct Records {
    values: Vec<Option<Traits>>,
    active: Vec<bool>,
}

impl Emitter<'_> {
    /// Compute each shared record shape once. Default output needs no new walk.
    pub(super) fn prepare_derive_records(&mut self) -> Result<(), Error> {
        if self.options.derives == DeriveOptions::default() {
            return Ok(());
        }
        let mut records = Records {
            values: vec![None; self.unit.records.len()],
            active: vec![false; self.unit.records.len()],
        };
        let mut remaining = 1_000_000;
        for &id in &self.records {
            self.derive_type(
                &Type::new(TypeKind::Record(id)),
                &mut records,
                &mut remaining,
                0,
            )?;
        }
        self.derive_records = records;
        Ok(())
    }

    fn derive_type(
        &self,
        ty: &Type,
        records: &mut Records,
        remaining: &mut usize,
        depth: usize,
    ) -> Result<Traits, Error> {
        check_depth(depth)?;
        *remaining = remaining.checked_sub(1).ok_or_else(|| {
            Error("derive eligibility exceeds the 1000000-node binding limit".into())
        })?;
        if self.external_key(ty)?.is_some() || self.callback_uses_external_storage(ty, depth)? {
            return Ok(Traits::default());
        }
        // Each alias may name a caller-owned Rust replacement. Resolving the
        // whole chain would discard that boundary and assume the C type's traits.
        if let TypeKind::Typedef(name) = &ty.kind {
            let inner = self
                .unit
                .typedefs
                .get(name)
                .ok_or_else(|| Error(format!("unknown typedef `{name}`")))?;
            return self.derive_type(inner, records, remaining, depth + 1);
        }
        let options = self.options.derives;
        let resolved = self.unit.resolve(ty)?;
        Ok(match &resolved.kind {
            TypeKind::Record(id) => {
                if let Some(value) = records.values[*id] {
                    return Ok(value);
                }
                if records.active[*id] {
                    return Err(Error("recursive record by value".into()));
                }
                records.active[*id] = true;
                let record = &self.unit.records[*id];
                let mut value = Traits::ALL;
                if let Some(fields) = &record.fields {
                    for field in fields {
                        value.intersect(self.derive_type(
                            &field.ty,
                            records,
                            remaining,
                            depth + 1,
                        )?);
                    }
                    if record.kind == RecordKind::Union {
                        value.debug = false;
                        value.partial_eq = false;
                        value.eq = false;
                        // A union has no active member invariant. Zeroing its
                        // storage does not make any particular member readable.
                        // Both containment queries use prepared record caches.
                        value.zeroable = !self.contains_atomic_storage(ty, 0)?
                            && !self.contains_external_storage(ty, 0)?;
                    } else if self.has_bitfield_padding(*id)? {
                        // MaybeUninit padding has no equality operation, and
                        // reading padding to implement one would be invalid.
                        value.partial_eq = false;
                        value.eq = false;
                    }
                } else {
                    value.zeroable = false;
                    value.partial_eq = false;
                    value.eq = false;
                }
                value.copy &= options.copy;
                value.debug &= options.debug.unwrap_or(false);
                value.partial_eq &= options.partial_eq || options.eq;
                value.eq &= options.eq;
                if (record.packed || record.pack.is_some()) && !value.copy {
                    value.debug = false;
                    value.partial_eq = false;
                    value.eq = false;
                }
                records.active[*id] = false;
                records.values[*id] = Some(value);
                value
            }
            TypeKind::Array { element, length } => {
                let mut value = self.derive_type(element, records, remaining, depth + 1)?;
                if length.unwrap_or(0) == 0 {
                    // Empty Rust arrays contain no potentially invalid value.
                    value.zeroable = true;
                }
                value
            }
            TypeKind::Enum(id) if self.is_rustified_enum(*id) => Traits {
                copy: options.copy,
                debug: options.debug.unwrap_or(true),
                zeroable: self.unit.enums[*id]
                    .variants
                    .iter()
                    .any(|variant| variant.value.value == 0),
                ..Traits::ALL
            },
            TypeKind::Pointer(pointee) => {
                let mut value = Traits::ALL;
                if let TypeKind::Function(function) = &self.unit.resolve(pointee)?.kind {
                    // Rust 1.64 and bindgen's supported derive subset share
                    // these C callback signature limits.
                    let comparable = function.calling_convention.for_target(self.unit.target)?
                        == CallingConvention::C
                        && function.parameters.len() <= 12;
                    value.debug = comparable;
                    value.partial_eq = comparable;
                    value.eq = comparable;
                }
                value
            }
            TypeKind::Float(_) => Traits {
                eq: false,
                ..Traits::ALL
            },
            TypeKind::Vector { .. } => Traits {
                copy: options.copy,
                debug: options.debug.unwrap_or(false),
                partial_eq: options.partial_eq || options.eq,
                eq: options.eq,
                zeroable: true,
            },
            TypeKind::Bool | TypeKind::Integer(_) | TypeKind::Enum(_) => Traits::ALL,
            // Opaque atomics have a separate initialization contract; external
            // types and unsupported storage cannot supply implicit guarantees.
            _ => Traits::default(),
        })
    }

    fn has_bitfield_padding(&self, id: usize) -> Result<bool, Error> {
        let fields = self.unit.records[id].fields.as_deref().unwrap_or_default();
        if !fields.iter().any(|field| field.bit_width.is_some()) {
            return Ok(false);
        }
        let layout = self.unit.layout(&Type::new(TypeKind::Record(id)))?;
        let mut end = 0;
        for (field, position) in fields.iter().zip(&layout.fields) {
            let Some(position) = position else { continue };
            let start = position.offset_bits / 8;
            if start > end {
                return Ok(true);
            }
            let next = if field.bit_width.is_some() {
                (position.offset_bits + position.size_bits).div_ceil(8)
            } else {
                start + self.unit.layout(&field.ty)?.size_bytes()
            };
            end = end.max(next);
        }
        Ok(end < layout.size_bytes())
    }

    pub(super) fn record_derives(&self, id: usize) -> Result<Cow<'static, str>, Error> {
        if let Some(Some(value)) = self.derive_records.values.get(id) {
            return Ok(Cow::Owned(value.attributes()));
        }
        let ty = Type::new(TypeKind::Record(id));
        Ok(
            if self.contains_atomic_storage(&ty, 0)? || self.contains_external_storage(&ty, 0)? {
                Cow::Borrowed("")
            } else {
                Cow::Borrowed("#[derive(Clone, Copy)]\n")
            },
        )
    }

    pub(super) fn record_has_default(&self, id: usize) -> bool {
        self.options.derives.default
            && self
                .derive_records
                .values
                .get(id)
                .is_some_and(|value| value.is_some_and(|value| value.zeroable))
    }

    pub(super) fn storage_is_copy(&self, ty: &Type, depth: usize) -> Result<bool, Error> {
        if self.options.derives == DeriveOptions::default() {
            return Ok(!self.contains_atomic_storage(ty, depth)?
                && !self.contains_external_storage(ty, depth)?);
        }
        check_depth(depth)?;
        if self.external_key(ty)?.is_some() || self.callback_uses_external_storage(ty, depth)? {
            return Ok(false);
        }
        if let TypeKind::Typedef(name) = &ty.kind {
            let inner = self
                .unit
                .typedefs
                .get(name)
                .ok_or_else(|| Error(format!("unknown typedef `{name}`")))?;
            return self.storage_is_copy(inner, depth + 1);
        }
        Ok(match &self.unit.resolve(ty)?.kind {
            TypeKind::Atomic(_) => false,
            TypeKind::Array { element, .. } => self.storage_is_copy(element, depth + 1)?,
            TypeKind::Record(id) => match self.derive_records.values.get(*id) {
                Some(Some(value)) => value.copy,
                _ => {
                    !self.contains_atomic_storage(ty, 0)?
                        && !self.contains_external_storage(ty, 0)?
                }
            },
            TypeKind::Enum(id) if self.is_rustified_enum(*id) => self.options.derives.copy,
            TypeKind::Vector { .. } => self.options.derives.copy,
            _ => true,
        })
    }

    pub(super) fn enum_derives(&self) -> Cow<'static, str> {
        if self.options.derives.debug.unwrap_or(true) && self.options.derives.copy {
            return Cow::Borrowed("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
        }
        let mut names = Vec::new();
        if self.options.derives.debug.unwrap_or(true) {
            names.push("Debug");
        }
        names.push("Clone");
        if self.options.derives.copy {
            names.push("Copy");
        }
        names.extend(["PartialEq", "Eq", "Hash"]);
        Cow::Owned(attributes(&names))
    }

    pub(super) fn vector_derives(&self) -> Cow<'static, str> {
        let traits = Traits {
            copy: self.options.derives.copy,
            debug: self.options.derives.debug.unwrap_or(false),
            partial_eq: self.options.derives.partial_eq || self.options.derives.eq,
            eq: self.options.derives.eq,
            zeroable: true,
        };
        if traits.copy && !traits.debug && !traits.partial_eq && !traits.eq {
            Cow::Borrowed("#[derive(Clone, Copy)]\n")
        } else {
            Cow::Owned(traits.attributes())
        }
    }
}

fn attributes(names: &[&str]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!("#[derive({})]\n", names.join(", "))
    }
}

/// Emit only after proving zero validity for the actual generated representation.
pub(super) fn zero_default(name: &str, source: &mut String) {
    writeln!(source, "impl ::core::default::Default for {name} {{\n    fn default() -> Self {{\n        let mut value = ::core::mem::MaybeUninit::<Self>::uninit();\n        // Zero is valid for this generated storage representation.\n        unsafe {{\n            ::core::ptr::write_bytes(value.as_mut_ptr(), 0, 1);\n            value.assume_init()\n        }}\n    }}\n}}\n").unwrap();
}

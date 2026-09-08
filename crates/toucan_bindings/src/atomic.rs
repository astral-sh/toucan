//! Atomic object storage and the separate scalar FFI value boundary.

use std::collections::HashMap;
use std::fmt::Write;

use toucan_semantic::{IntegerKind, Qualifiers, Type, TypeKind};
use toucan_target::Compiler;

use crate::{Emitter, Error, check_depth};

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct Key {
    value: Type,
    qualifiers: Qualifiers,
    size: u64,
    alignment: u64,
}

pub(super) enum Representation {
    Scalar(&'static str),
    Pointer(Type),
    Opaque,
}

#[derive(Default)]
pub(super) struct Atomics {
    indices: HashMap<Key, usize>,
    representations: Vec<(Key, Representation)>,
    /// Filled only when a selected type contains atomic storage.
    records: Vec<Option<bool>>,
}

impl Emitter<'_> {
    fn atomic_key(&self, ty: &Type) -> Result<Key, Error> {
        let value = self
            .unit
            .atomic_value(ty)?
            .ok_or_else(|| Error("expected atomic storage".into()))?;
        bounded_key(value, 0, &mut 65_536)?;
        let layout = self.unit.layout(ty)?;
        let size = layout.size_bytes();
        let alignment = layout.alignment_bytes();
        if !size.is_multiple_of(alignment)
            || layout.field_alignment_bits != layout.alignment_bits
            || size > i64::MAX as u64
        {
            return Err(Error(
                "atomic size or alignment cannot be represented by a Rust object type".into(),
            ));
        }
        Ok(Key {
            value: value.clone(),
            qualifiers: self.unit.qualifiers(ty)?,
            size,
            alignment,
        })
    }

    /// Register storage once; recursive pointer dependencies are collected afterward.
    pub(super) fn collect_atomic(&mut self, ty: &Type, depth: usize) -> Result<(), Error> {
        check_depth(depth)?;
        let unit = self.unit;
        let value = unit
            .atomic_value(ty)?
            .ok_or_else(|| Error("expected atomic storage".into()))?;
        self.reject_complex_storage(value)?;
        let key = self.atomic_key(ty)?;
        if self.atomics.indices.contains_key(&key) {
            return Ok(());
        }
        if self.atomics.representations.len() >= 65_536 {
            return Err(Error(
                "atomic storage count exceeds the 65536-entry binding limit".into(),
            ));
        }
        let mut representation = Representation::Opaque;
        let mut dependency = None;
        if key.qualifiers == Qualifiers::default() && key.alignment == key.size {
            let value = self.unit.resolve(&key.value)?;
            let scalar = match value.kind {
                TypeKind::Bool => Some("AtomicBool"),
                TypeKind::Integer(kind) => {
                    let signed = match kind {
                        IntegerKind::Char => self.unit.target.char_is_signed(),
                        IntegerKind::UnsignedChar
                        | IntegerKind::UnsignedShort
                        | IntegerKind::UnsignedInt
                        | IntegerKind::UnsignedLong
                        | IntegerKind::UnsignedLongLong
                        | IntegerKind::UnsignedInt128 => false,
                        _ => true,
                    };
                    atomic_integer(key.size, signed)
                }
                TypeKind::Enum(id) => {
                    let (_, signed) = self.enum_integer(id)?;
                    atomic_integer(key.size, signed)
                }
                _ => None,
            };
            if let Some(name) = scalar {
                representation = Representation::Scalar(name);
            } else if let TypeKind::Pointer(pointee) = &value.kind
                && self.unit.qualifiers(pointee)? == Qualifiers::default()
                && !matches!(
                    self.unit.resolve(pointee)?.kind,
                    TypeKind::Function(_)
                        | TypeKind::VariableArray { .. }
                        | TypeKind::Sve(_)
                        | TypeKind::Float(
                            crate::FloatKind::LongDouble
                                | crate::FloatKind::BFloat16
                                | crate::FloatKind::Extended { .. }
                        )
                )
            {
                representation = Representation::Pointer((**pointee).clone());
                dependency = Some((**pointee).clone());
            }
        }
        let id = self.atomics.representations.len();
        self.atomics.indices.insert(key.clone(), id);
        self.atomics.representations.push((key, representation));
        if let Some(dependency) = dependency {
            self.collect_at(&dependency, depth + 1)?;
        }
        Ok(())
    }

    fn atomic_representation(&self, id: usize) -> Result<String, Error> {
        Ok(match &self.atomics.representations[id].1 {
            Representation::Scalar(name) => format!("::core::sync::atomic::{name}"),
            Representation::Pointer(pointee) => {
                format!("::core::sync::atomic::AtomicPtr<{}>", self.ty(pointee)?)
            }
            Representation::Opaque => self.synthetic_name(&self.helper_name("atomic", id))?,
        })
    }

    pub(super) fn atomic_storage_type(&self, ty: &Type) -> Result<String, Error> {
        let key = self.atomic_key(ty)?;
        let id = self
            .atomics
            .indices
            .get(&key)
            .ok_or_else(|| Error("atomic storage was not collected".into()))?;
        self.atomic_representation(*id)
    }

    pub(super) fn emit_atomics(&self, source: &mut String) -> Result<(), Error> {
        for (id, (key, representation)) in self.atomics.representations.iter().enumerate() {
            let name = self.atomic_representation(id)?;
            let (size, alignment) = (key.size, key.alignment);
            if matches!(representation, Representation::Opaque) {
                writeln!(source,"/// Opaque C atomic storage. Read and write through C accessor functions.\n/// This type provides no Rust atomic operations or automatic thread sharing.\n#[repr(C, align({alignment}))]\npub struct {name} {{\n    __storage: ::core::cell::UnsafeCell<::core::mem::MaybeUninit<[::core::primitive::u8; {size}]>>,\n    __marker: ::core::marker::PhantomData<*mut ()>,\n}}\nimpl {name} {{\n    /// Allocates storage without initializing its C atomic value.\n    /// Call a C initializer before passing it to a function that reads the value.\n    pub const fn uninit() -> Self {{\n        Self {{ __storage: ::core::cell::UnsafeCell::new(::core::mem::MaybeUninit::uninit()), __marker: ::core::marker::PhantomData }}\n    }}\n}}\n").unwrap();
            }
            writeln!(source,"const _: [(); {size}] = [(); ::core::mem::size_of::<{name}>()];\nconst _: [(); {alignment}] = [(); ::core::mem::align_of::<{name}>()];\n").unwrap();
        }
        Ok(())
    }

    /// Memoize containment so shared record subgraphs do not multiply the work.
    /// No cache allocation or record walk is needed without selected atomics.
    pub(super) fn prepare_atomic_records(&mut self) -> Result<(), Error> {
        if self.atomics.representations.is_empty() {
            return Ok(());
        }
        let mut cache = vec![None; self.unit.records.len()];
        let mut active = vec![false; self.unit.records.len()];
        let mut remaining = 1_000_000;
        for &id in &self.records {
            self.atomic_containment(
                &Type::new(TypeKind::Record(id)),
                &mut cache,
                &mut active,
                &mut remaining,
                0,
            )?;
        }
        self.atomics.records = cache;
        Ok(())
    }

    fn atomic_containment(
        &self,
        ty: &Type,
        cache: &mut [Option<bool>],
        active: &mut [bool],
        remaining: &mut usize,
        depth: usize,
    ) -> Result<bool, Error> {
        check_depth(depth)?;
        *remaining = remaining.checked_sub(1).ok_or_else(|| {
            Error("atomic containment walk exceeds the binding node limit".into())
        })?;
        Ok(match &self.unit.resolve(ty)?.kind {
            TypeKind::Atomic(_) => true,
            TypeKind::Array { element, .. } => {
                self.atomic_containment(element, cache, active, remaining, depth + 1)?
            }
            TypeKind::Record(id) => {
                if let Some(value) = cache[*id] {
                    return Ok(value);
                }
                if active[*id] {
                    return Err(Error("recursive record by value".into()));
                }
                active[*id] = true;
                let mut value = false;
                if let Some(fields) = &self.unit.records[*id].fields {
                    for field in fields {
                        value |= self.atomic_containment(
                            &field.ty,
                            cache,
                            active,
                            remaining,
                            depth + 1,
                        )?;
                    }
                }
                active[*id] = false;
                cache[*id] = Some(value);
                value
            }
            _ => false,
        })
    }

    pub(super) fn contains_atomic_storage(&self, ty: &Type, depth: usize) -> Result<bool, Error> {
        if self.atomics.representations.is_empty() {
            return Ok(false);
        }
        check_depth(depth)?;
        Ok(match &self.unit.resolve(ty)?.kind {
            TypeKind::Atomic(_) => true,
            TypeKind::Array { element, .. } => self.contains_atomic_storage(element, depth + 1)?,
            TypeKind::Record(id) => self
                .atomics
                .records
                .get(*id)
                .copied()
                .flatten()
                .unwrap_or(false),
            _ => false,
        })
    }

    /// Collect pointer/function dependencies used only by the scalar call carrier.
    pub(super) fn collect_call_value(&mut self, ty: &Type, depth: usize) -> Result<(), Error> {
        self.reject_complex_call_value(ty, depth)?;
        self.collect_at(ty, depth)?;
        if let Some(value) = self.unit.atomic_value(ty)?
            && matches!(self.unit.resolve(value)?.kind, TypeKind::Pointer(_))
        {
            let value = value.clone();
            self.collect_at(&value, depth + 1)?;
        }
        Ok(())
    }

    /// Supported atomic scalar values cross calls independently of their Rust
    /// storage type; narrow Clang values need a separate ABI carrier. Atomic
    /// enums use all compatible integer values, including with rustified enums.
    pub(super) fn call_value_type(&self, ty: &Type, depth: usize) -> Result<String, Error> {
        check_depth(depth)?;
        let Some(value) = self.unit.atomic_value(ty)? else {
            return self.ty_at(ty, depth);
        };
        let resolved = self.unit.resolve(value)?;
        if matches!(
            resolved.kind,
            TypeKind::Float(
                crate::FloatKind::FLOAT32
                    | crate::FloatKind::FLOAT64
                    | crate::FloatKind::FLOAT32X
                    | crate::FloatKind::FLOAT64X
            )
        ) {
            return Err(Error("atomic GNU interchange/extended floating calls require a separate ABI proof; use C pointer accessors".into()));
        }
        if matches!(resolved.kind, TypeKind::Complex(_)) {
            return Err(crate::complex::call_abi_error());
        }
        if self.unit.compiler == Compiler::Clang
            && matches!(
                resolved.kind,
                TypeKind::Bool | TypeKind::Integer(_) | TypeKind::Enum(_)
            )
            && self.unit.layout(value)?.size_bits < 32
        {
            // Clang atomic values omit the primitive ABI's integer extension.
            // This breaks optimized callbacks on x86-64 Linux and Darwin ARM.
            // A repr(C) wrapper also changes ARM stack argument slots.
            return Err(Error(
                "narrow atomic scalar calls under Clang have no supported Rust ABI carrier; use C pointer accessors".into(),
            ));
        }
        if matches!(resolved.kind, TypeKind::Integer(_) | TypeKind::Enum(_))
            && self.unit.layout(value)?.size_bits == 128
        {
            self.check_128_bit_abi()?;
            return Err(Error(
                "128-bit atomic scalar calls require a separate ABI proof; use C pointer accessors"
                    .into(),
            ));
        }
        if matches!(
            resolved.kind,
            TypeKind::Bool
                | TypeKind::Integer(_)
                | TypeKind::Float(_)
                | TypeKind::Pointer(_)
                | TypeKind::Enum(_)
        ) {
            let plain = Type::new(resolved.kind.clone());
            let natural = self.unit.layout(&plain)?;
            let storage = self.unit.layout(ty)?;
            if self.unit.alignment(value)? != natural.alignment_bytes()
                || storage.size_bits != natural.size_bits
                || storage.alignment_bits != natural.alignment_bits
            {
                return Err(Error("altered atomic scalar alignment requires a separate call-ABI proof; use C pointer accessors".into()));
            }
        }
        let value = resolved;
        match value.kind {
            TypeKind::Bool | TypeKind::Integer(_) | TypeKind::Float(_) | TypeKind::Pointer(_)=>self.ty_at(value,depth+1),
            TypeKind::Enum(id)=>self.enum_type(id),
            _=>Err(Error("atomic aggregate values require an unsupported Rust call ABI; expose C pointer accessors".into())),
        }
    }
}

fn atomic_integer(bytes: u64, signed: bool) -> Option<&'static str> {
    Some(match (bytes, signed) {
        (1, true) => "AtomicI8",
        (1, false) => "AtomicU8",
        (2, true) => "AtomicI16",
        (2, false) => "AtomicU16",
        (4, true) => "AtomicI32",
        (4, false) => "AtomicU32",
        (8, true) => "AtomicI64",
        (8, false) => "AtomicU64",
        _ => return None,
    })
}

// Hashing/cloning a public caller-built Type must not precede its depth guard.
fn bounded_key(ty: &Type, depth: usize, remaining: &mut usize) -> Result<(), Error> {
    check_depth(depth)?;
    *remaining = remaining
        .checked_sub(1)
        .ok_or_else(|| Error("atomic type exceeds the 65536-node binding limit".into()))?;
    match &ty.kind {
        TypeKind::Pointer(inner)
        | TypeKind::Atomic(inner)
        | TypeKind::Array { element: inner, .. }
        | TypeKind::VariableArray { element: inner, .. }
        | TypeKind::Vector { element: inner, .. } => bounded_key(inner, depth + 1, remaining)?,
        TypeKind::Function(function) => {
            bounded_key(&function.return_type, depth + 1, remaining)?;
            for parameter in &function.parameters {
                bounded_key(&parameter.ty, depth + 1, remaining)?;
            }
        }
        _ => {}
    }
    Ok(())
}

//! Keep C complex storage and call ABIs behind their independent proof boundaries.

use toucan_semantic::{Type, TypeKind};

use crate::{Emitter, Error, check_depth};

pub(super) fn storage_error() -> Error {
    Error("C complex storage has no supported Rust representation yet".into())
}

pub(super) fn call_abi_error() -> Error {
    Error("C complex values and aggregates containing them require a verified Rust call ABI".into())
}

#[derive(Clone, Copy, Default)]
enum Containment {
    #[default]
    Unchecked,
    Visiting,
    Absent,
    Present,
}

#[derive(Default)]
pub(super) struct Records {
    states: Vec<Containment>,
}

impl Emitter<'_> {
    /// Checks embedded values without following pointers into another object's storage.
    /// Memoizing shared record shapes avoids expanding a type graph as a tree.
    fn contains_complex_value(
        &mut self,
        ty: &Type,
        depth: usize,
        remaining: &mut usize,
    ) -> Result<bool, Error> {
        check_depth(depth)?;
        *remaining = remaining.checked_sub(1).ok_or_else(|| {
            Error("complex type traversal exceeds the 65536-node binding limit".into())
        })?;
        let unit = self.unit;
        match &unit.resolve(ty)?.kind {
            TypeKind::Complex(_) => Ok(true),
            TypeKind::Atomic(inner)
            | TypeKind::Array { element: inner, .. }
            | TypeKind::VariableArray { element: inner, .. } => {
                self.contains_complex_value(inner, depth + 1, remaining)
            }
            TypeKind::Record(id) => {
                let record = unit
                    .records
                    .get(*id)
                    .ok_or_else(|| Error("invalid record identity".into()))?;
                if self.complex_records.states.is_empty() {
                    self.complex_records
                        .states
                        .resize(unit.records.len(), Containment::Unchecked);
                }
                match self.complex_records.states[*id] {
                    Containment::Present => return Ok(true),
                    Containment::Absent => return Ok(false),
                    Containment::Visiting => return Err(Error("recursive record by value".into())),
                    Containment::Unchecked => {}
                }
                self.complex_records.states[*id] = Containment::Visiting;
                let mut present = false;
                if let Some(fields) = &record.fields {
                    for field in fields {
                        if self.contains_complex_value(&field.ty, depth + 1, remaining)? {
                            present = true;
                            break;
                        }
                    }
                }
                self.complex_records.states[*id] = if present {
                    Containment::Present
                } else {
                    Containment::Absent
                };
                Ok(present)
            }
            _ => Ok(false),
        }
    }

    pub(super) fn reject_complex_call_value(
        &mut self,
        ty: &Type,
        depth: usize,
    ) -> Result<(), Error> {
        if self.contains_complex_value(ty, depth, &mut 65_536)? {
            return Err(call_abi_error());
        }
        Ok(())
    }

    pub(super) fn reject_complex_storage(&mut self, ty: &Type) -> Result<(), Error> {
        if self.contains_complex_value(ty, 0, &mut 65_536)? {
            return Err(storage_error());
        }
        Ok(())
    }
}

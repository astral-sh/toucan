//! Bounded comparison of object occurrence types after removing typedef spelling.

use std::collections::BTreeSet;

use crate::{Error, Qualifiers, TranslationUnit, Type, TypeKind};

pub(crate) const COMPARISON_LIMIT: usize = 1_000_000;
type Key = (*const Type, *const Type, u8, u8, bool, bool);

/// Compares object types without changing declarations or their owned type IDs.
///
/// Typedef spelling and alignment provenance are not C type identity. Qualifiers,
/// array bounds, prototypes, calling conventions, and nominal tag/vector identities
/// remain significant. This is narrower than general C compatibility.
///
/// Reuse one comparator across a binding pass to share its one-million-reference
/// budget and memoized type pairs. Type arguments borrow for the comparator's
/// lifetime, so cached identities cannot outlive or be reused by their owners.
pub struct ObjectTypeComparison<'unit> {
    unit: &'unit TranslationUnit,
    remaining: usize,
    matched: BTreeSet<Key>,
}

impl<'unit> ObjectTypeComparison<'unit> {
    /// Start one bounded comparison pass over an immutable translation unit.
    pub fn new(unit: &'unit TranslationUnit) -> Self {
        Self::with_remaining(unit, COMPARISON_LIMIT)
    }

    pub(crate) fn with_remaining(unit: &'unit TranslationUnit, remaining: usize) -> Self {
        Self {
            unit,
            remaining,
            matched: BTreeSet::new(),
        }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.remaining
    }

    /// Whether both types carry the same complete information after typedefs are removed.
    pub fn same_type(&mut self, first: &'unit Type, second: &'unit Type) -> Result<bool, Error> {
        self.compare(first, second, 0, 0, false, false, 0)
    }

    /// Match an earlier occurrence against a later declaration, permitting array completion.
    pub fn matches_declaration(
        &mut self,
        first: &'unit Type,
        second: &'unit Type,
    ) -> Result<bool, Error> {
        self.compare(first, second, 0, 0, false, true, 0)
    }

    fn same_parameter_contracts(
        &mut self,
        first: Option<crate::ParameterContractsId>,
        second: Option<crate::ParameterContractsId>,
    ) -> Result<bool, Error> {
        let first = self.unit.noescape_parameters(first)?;
        let second = self.unit.noescape_parameters(second)?;
        self.remaining = self
            .remaining
            .checked_sub(first.len().saturating_add(second.len()))
            .ok_or_else(|| Error::new(0, "object-type comparison reference limit exceeded"))?;
        Ok(first == second)
    }

    fn resolve(
        &mut self,
        mut ty: &'unit Type,
        mut qualifiers: u8,
    ) -> Result<(&'unit Type, u8), Error> {
        for _ in 0..128 {
            self.remaining = self
                .remaining
                .checked_sub(1)
                .ok_or_else(|| Error::new(0, "object-type comparison reference limit exceeded"))?;
            qualifiers |= qualifier_bits(ty.qualifiers);
            let TypeKind::Typedef(name) = &ty.kind else {
                return Ok((ty, qualifiers));
            };
            ty = self
                .unit
                .typedefs
                .get(name)
                .ok_or_else(|| Error::new(0, format!("unknown typedef `{name}`")))?;
        }
        Err(Error::new(
            0,
            "object-type comparison nesting limit exceeded",
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn compare(
        &mut self,
        first: &'unit Type,
        second: &'unit Type,
        first_qualifiers: u8,
        second_qualifiers: u8,
        parameter: bool,
        completion: bool,
        depth: usize,
    ) -> Result<bool, Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "object-type comparison nesting limit exceeded",
            ));
        }
        let (first, first_qualifiers) = self.resolve(first, first_qualifiers)?;
        let (second, second_qualifiers) = self.resolve(second, second_qualifiers)?;
        let arrays = matches!(
            (&first.kind, &second.kind),
            (TypeKind::Array { .. }, TypeKind::Array { .. })
                | (
                    TypeKind::VariableArray { .. },
                    TypeKind::VariableArray { .. }
                )
        );
        if !parameter && !arrays && first_qualifiers != second_qualifiers {
            return Ok(false);
        }
        let key = (
            std::ptr::from_ref(first),
            std::ptr::from_ref(second),
            first_qualifiers,
            second_qualifiers,
            parameter,
            completion,
        );
        if self.matched.contains(&key) {
            return Ok(true);
        }
        let matches = match (&first.kind, &second.kind) {
            (TypeKind::Void, TypeKind::Void) | (TypeKind::Bool, TypeKind::Bool) => true,
            (TypeKind::Integer(a), TypeKind::Integer(b)) => a == b,
            (TypeKind::Float(a), TypeKind::Float(b))
            | (TypeKind::Complex(a), TypeKind::Complex(b)) => a == b,
            (TypeKind::Record(a), TypeKind::Record(b)) | (TypeKind::Enum(a), TypeKind::Enum(b)) => {
                a == b
            }
            (TypeKind::Sve(a), TypeKind::Sve(b)) => a == b,
            (TypeKind::Pointer(a), TypeKind::Pointer(b))
            | (TypeKind::Atomic(a), TypeKind::Atomic(b)) => {
                self.compare(a, b, 0, 0, false, completion, depth + 1)?
            }
            (
                TypeKind::Array {
                    element: a,
                    length: al,
                },
                TypeKind::Array {
                    element: b,
                    length: bl,
                },
            ) => {
                (al == bl || (completion && al.is_none()))
                    && self.compare(
                        a,
                        b,
                        first_qualifiers,
                        second_qualifiers,
                        false,
                        completion,
                        depth + 1,
                    )?
            }
            (
                TypeKind::VariableArray {
                    element: a,
                    identity: ai,
                },
                TypeKind::VariableArray {
                    element: b,
                    identity: bi,
                },
            ) => {
                ai == bi
                    && self.compare(
                        a,
                        b,
                        first_qualifiers,
                        second_qualifiers,
                        false,
                        completion,
                        depth + 1,
                    )?
            }
            (
                TypeKind::Vector {
                    element: a,
                    lanes: al,
                    kind: ak,
                },
                TypeKind::Vector {
                    element: b,
                    lanes: bl,
                    kind: bk,
                },
            ) => al == bl && ak == bk && self.compare(a, b, 0, 0, false, completion, depth + 1)?,
            (TypeKind::Function(a), TypeKind::Function(b)) => {
                if a.prototype != b.prototype
                    || a.variadic != b.variadic
                    || a.calling_convention != b.calling_convention
                    || a.noreturn != b.noreturn
                    || a.parameters.len() != b.parameters.len()
                    || !self
                        .same_parameter_contracts(a.parameter_contracts, b.parameter_contracts)?
                {
                    false
                } else {
                    let mut same = self.compare(
                        &a.return_type,
                        &b.return_type,
                        0,
                        0,
                        false,
                        completion,
                        depth + 1,
                    )?;
                    for (a, b) in a.parameters.iter().zip(&b.parameters) {
                        if !same {
                            break;
                        }
                        // Parameter top-level qualifiers are absent from the C function type.
                        same = self.compare(&a.ty, &b.ty, 0, 0, true, completion, depth + 1)?;
                    }
                    same
                }
            }
            _ => false,
        };
        if matches {
            self.matched.insert(key);
        }
        Ok(matches)
    }
}

fn qualifier_bits(qualifiers: Qualifiers) -> u8 {
    u8::from(qualifiers.is_const)
        | (u8::from(qualifiers.is_volatile) << 1)
        | (u8::from(qualifiers.is_restrict) << 2)
        | (u8::from(qualifiers.is_unaligned()) << 3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze;
    use toucan_target::Target;

    #[test]
    fn shared_callback_pairs_are_visited_once_and_budget_survives_new_comparators() {
        let mut source = "typedef int (*A0)(void); typedef int (*B0)(void);".to_owned();
        for level in 1..12 {
            let previous = level - 1;
            source.push_str(&format!("typedef int (*A{level})(A{previous},A{previous}); typedef int (*B{level})(B{previous},B{previous});"));
        }
        let unit = analyze(&source, Target::X86_64UnknownLinuxGnu).unwrap();
        let a = &unit.typedefs["A11"];
        let b = &unit.typedefs["B11"];
        let mut comparison = ObjectTypeComparison::new(&unit);
        assert!(comparison.same_type(a, b).unwrap());
        assert!(COMPARISON_LIMIT - comparison.remaining() < 200);
        let before = comparison.remaining();
        for _ in 0..100 {
            assert!(comparison.same_type(a, b).unwrap());
        }
        assert!(before - comparison.remaining() <= 200);
        let mut remaining = 1;
        for _ in 0..2 {
            let mut comparison = ObjectTypeComparison::with_remaining(&unit, remaining);
            let error = comparison.same_type(a, b).unwrap_err();
            remaining = comparison.remaining();
            assert_eq!(
                error.message,
                "object-type comparison reference limit exceeded"
            );
        }
    }
}

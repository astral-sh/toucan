//! Opt-in source typedef dependencies of reachable C types.

use std::collections::{BTreeMap, BTreeSet};

use toucan_semantic::TranslationUnit;

use crate::Error;

/// Additional typedef dependencies erased from adjusted C parameter types.
///
/// These edges only retain named Rust declarations. They never replace a C
/// parameter type or change a call's ABI. Empty or absent maps preserve ordinary
/// binding generation.
#[derive(Clone, Debug, Default)]
pub struct TypeDependencies {
    /// Dependencies of callback fields, keyed by the semantic record identity.
    pub records: BTreeMap<usize, BTreeSet<String>>,
    /// Dependencies of function and callback typedefs, keyed by their C names.
    pub typedefs: BTreeMap<String, BTreeSet<String>>,
}

impl TypeDependencies {
    pub(crate) fn validate(&self, unit: &TranslationUnit) -> Result<(), Error> {
        if self.records.keys().any(|id| *id >= unit.records.len())
            || self
                .typedefs
                .keys()
                .any(|name| !unit.typedefs.contains_key(name))
        {
            return Err(Error("invalid parameter-type dependency owner".into()));
        }
        let mut remaining = 1_000_000usize;
        let mut bytes = 64 * 1024 * 1024usize;
        for name in self.typedefs.keys().chain(
            self.records
                .values()
                .chain(self.typedefs.values())
                .flatten(),
        ) {
            remaining = remaining.checked_sub(1).ok_or_else(|| {
                Error("parameter-type dependency reference limit exceeded".into())
            })?;
            bytes = bytes
                .checked_sub(name.len())
                .ok_or_else(|| Error("parameter-type dependency storage limit exceeded".into()))?;
            if !unit.typedefs.contains_key(name) {
                return Err(Error(format!("unknown parameter-type dependency `{name}`")));
            }
        }
        Ok(())
    }
}

//! Optional Rust documentation, keyed by original C identities.

use crate::{Emitter, Error};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use toucan_semantic::TranslationUnit;

/// Documentation for items in the supplied translation unit. Text is emitted as
/// escaped Rust string attributes after normal declaration selection and naming.
#[derive(Clone, Debug, Default)]
pub struct Documentation {
    /// Functions, objects, and typedefs, keyed by original C name.
    pub declarations: BTreeMap<String, String>,
    /// Record comments, keyed by canonical record index.
    pub records: BTreeMap<usize, String>,
    /// Enumeration comments, keyed by enum index.
    pub enums: BTreeMap<usize, String>,
    /// Named field comments, keyed by canonical record and field index.
    pub fields: BTreeMap<(usize, usize), String>,
    /// Enumerator documentation, keyed by its original C identifier.
    pub enumerators: BTreeMap<String, String>,
}
impl Documentation {
    pub(super) fn validate(&self, unit: &TranslationUnit) -> Result<(), Error> {
        let mut count = 0usize;
        let mut bytes = 0usize;
        for text in self
            .declarations
            .values()
            .chain(self.records.values())
            .chain(self.enums.values())
            .chain(self.fields.values())
            .chain(self.enumerators.values())
        {
            count += 1;
            bytes = bytes
                .checked_add(text.len())
                .ok_or_else(|| Error("documentation size overflow".into()))?;
            if count > 1_000_000 || bytes > 64 * 1024 * 1024 {
                return Err(Error("documentation exceeds its item or byte limit".into()));
            }
        }
        if !self.declarations.is_empty() {
            let names: BTreeSet<_> = unit
                .declarations
                .iter()
                .map(|declaration| declaration.name.as_str())
                .collect();
            if self
                .declarations
                .keys()
                .any(|name| !names.contains(name.as_str()))
            {
                return Err(Error(
                    "documentation references an unknown declaration".into(),
                ));
            }
        }
        if self.records.keys().any(|&id| id >= unit.records.len())
            || self.enums.keys().any(|&id| id >= unit.enums.len())
            || self.fields.keys().any(|&(id, field)| {
                unit.records
                    .get(id)
                    .and_then(|record| record.fields.as_ref())
                    .is_none_or(|fields| field >= fields.len())
            })
            || self
                .enumerators
                .keys()
                .any(|name| !unit.constants.contains_key(name))
        {
            return Err(Error(
                "documentation references an unknown type, field, or enumerator".into(),
            ));
        }
        Ok(())
    }
}

fn emit(source: &mut String, text: Option<&String>) {
    if let Some(text) = text
        && !text.is_empty()
    {
        writeln!(source, "#[doc = {text:?}]").unwrap();
    }
}
impl Emitter<'_> {
    pub(super) fn declaration_doc(&self, name: &str, source: &mut String) {
        emit(
            source,
            self.options
                .documentation
                .as_ref()
                .and_then(|docs| docs.declarations.get(name)),
        );
    }
    pub(super) fn record_doc(&self, id: usize, source: &mut String) -> Result<(), Error> {
        if let Some(docs) = &self.options.documentation {
            emit(source, docs.records.get(&self.unit.record_origin(id)?));
        }
        Ok(())
    }
    pub(super) fn enum_doc(&self, id: usize, source: &mut String) {
        emit(
            source,
            self.options
                .documentation
                .as_ref()
                .and_then(|docs| docs.enums.get(&id)),
        );
    }
    pub(super) fn field_doc(
        &self,
        id: usize,
        field: usize,
        source: &mut String,
    ) -> Result<(), Error> {
        if let Some(docs) = &self.options.documentation {
            emit(
                source,
                docs.fields.get(&(self.unit.record_origin(id)?, field)),
            );
        }
        Ok(())
    }
    pub(super) fn enumerator_doc(&self, name: &str, source: &mut String) {
        emit(
            source,
            self.options
                .documentation
                .as_ref()
                .and_then(|docs| docs.enumerators.get(name)),
        );
    }
}

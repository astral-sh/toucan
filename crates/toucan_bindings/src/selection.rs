//! Explicit binding roots keep C tag and ordinary identifier namespaces separate.

use std::collections::BTreeSet;

use toucan_semantic::{DeclarationKind, Scope, TranslationUnit};

use crate::{Error, Options};

/// Additional roots selected from the same translation unit used for generation.
///
/// Unlike name patterns, a record ID cannot select an unrelated function or object
/// with the same C spelling. Dependencies of selected roots are still collected.
/// `Options::selection = Some(default())` selects nothing unless a name allowlist
/// also matches. Every indexed root is validated before generation.
#[derive(Debug, Default, Clone)]
pub struct BindingSelection {
    /// Keep referenced types when a selected symbol is omitted or a reached
    /// type definition is blocklisted. Disabled by default; roots and ABI
    /// validation are otherwise unchanged.
    pub retain_type_dependencies: bool,
    /// Indices into `TranslationUnit::declarations`.
    pub declarations: BTreeSet<usize>,
    /// File-scope record IDs.
    pub records: BTreeSet<usize>,
    /// File-scope enum IDs.
    pub enums: BTreeSet<usize>,
    /// Exact typedef names, including caller-built aliases without declaration rows.
    pub typedefs: BTreeSet<String>,
    /// Exact names from `TranslationUnit::constants`.
    pub constants: BTreeSet<String>,
    /// Exact object macro names. Unsupported selected macros remain reportable.
    pub macros: BTreeSet<String>,
}

impl BindingSelection {
    /// Add file-scope tags matched by their emitted lexical names, before Rust
    /// keyword escaping. Anonymous enums without a typedef or enclosing record
    /// may also be selected by any of their original enumerator names.
    ///
    /// Dependencies are collected during generation. Typedefs, functions,
    /// objects, and macros are separate root categories.
    pub fn extend_tags(
        &mut self,
        unit: &TranslationUnit,
        options: &Options,
        mut matches_type: impl FnMut(&str) -> bool,
        mut matches_anonymous_variant: impl FnMut(&str) -> bool,
    ) -> Result<(), Error> {
        let names = crate::lexical_names::Names::new(unit, options)?;
        for (id, record) in unit.records.iter().enumerate() {
            if record.scope == Scope::File
                && !crate::tag_discovery::hidden_record(unit, options, id)
                && names
                    .record(id)
                    .or(record.name.as_deref())
                    .is_some_and(&mut matches_type)
            {
                self.records.insert(id);
            }
        }
        for (id, enumeration) in unit.enums.iter().enumerate() {
            if enumeration.scope != Scope::File
                || crate::tag_discovery::hidden_enum(unit, options, id)
            {
                continue;
            }
            let anonymous = !crate::lexical_names::Names::enum_is_named(unit, id)
                && names.enum_parent(unit, id).is_none();
            if names.enum_name(unit, id).is_some_and(&mut matches_type)
                || (anonymous
                    && enumeration
                        .variants
                        .iter()
                        .any(|variant| matches_anonymous_variant(&variant.name)))
            {
                self.enums.insert(id);
            }
        }
        Ok(())
    }

    pub(crate) fn validate(&self, unit: &TranslationUnit) -> Result<(), Error> {
        for &id in &self.declarations {
            if id >= unit.declarations.len() {
                return Err(Error(format!("invalid binding declaration root {id}")));
            }
        }
        for &id in &self.records {
            if !unit
                .records
                .get(id)
                .is_some_and(|record| record.scope == Scope::File)
            {
                return Err(Error(format!(
                    "invalid file-scope binding record root {id}"
                )));
            }
        }
        for &id in &self.enums {
            if !unit
                .enums
                .get(id)
                .is_some_and(|enumeration| enumeration.scope == Scope::File)
            {
                return Err(Error(format!("invalid file-scope binding enum root {id}")));
            }
        }
        for name in &self.typedefs {
            if !unit.typedefs.contains_key(name) {
                return Err(Error(format!("unknown binding typedef root `{name}`")));
            }
        }
        for name in &self.constants {
            if !unit.constants.contains_key(name) {
                return Err(Error(format!("unknown binding constant root `{name}`")));
            }
        }
        Ok(())
    }
}

impl Options {
    /// Whether the default unrestricted root selection applies.
    pub fn selects_all(&self) -> bool {
        self.allowlist.is_empty() && self.selection.is_none()
    }

    pub(crate) fn includes_declaration(
        &self,
        index: usize,
        name: &str,
        kind: DeclarationKind,
    ) -> bool {
        self.includes(name)
            || self.selection.as_ref().is_some_and(|roots| {
                roots.declarations.contains(&index)
                    || (kind == DeclarationKind::Typedef && roots.typedefs.contains(name))
            })
    }

    pub(crate) fn includes_record(&self, id: usize, name: Option<&str>) -> bool {
        name.is_some_and(|name| self.includes(name))
            || self
                .selection
                .as_ref()
                .is_some_and(|roots| roots.records.contains(&id))
    }

    pub(crate) fn includes_enum(&self, id: usize, name: Option<&str>) -> bool {
        name.is_some_and(|name| self.includes(name))
            || self
                .selection
                .as_ref()
                .is_some_and(|roots| roots.enums.contains(&id))
    }

    pub(crate) fn includes_typedef(&self, name: &str) -> bool {
        self.includes(name)
            || self
                .selection
                .as_ref()
                .is_some_and(|roots| roots.typedefs.contains(name))
    }

    pub(crate) fn includes_constant(&self, name: &str) -> bool {
        self.includes(name)
            || self
                .selection
                .as_ref()
                .is_some_and(|roots| roots.constants.contains(name))
    }

    /// Macro root selection, used before evaluating optional object constants.
    pub fn includes_macro(&self, name: &str) -> bool {
        self.includes(name)
            || self
                .selection
                .as_ref()
                .is_some_and(|roots| roots.macros.contains(name))
    }
}

//! Symbol renaming changes Rust declarations while retaining native linker names.

use std::collections::{BTreeMap, BTreeSet};

use toucan_semantic::{Declaration, DeclarationKind, TranslationUnit};

use crate::{Emitter, Error, MacroValue, Options};

impl Options {
    pub(crate) fn validate_generated_names(&self, unit: &TranslationUnit) -> Result<(), Error> {
        if self.generated_names.is_empty() {
            return Ok(());
        }
        let declared: BTreeSet<_> = unit
            .declarations
            .iter()
            .filter(|declaration| declaration.kind != DeclarationKind::Typedef)
            .map(|declaration| declaration.name.as_str())
            .collect();
        for (name, generated) in &self.generated_names {
            if !declared.contains(name.as_str()) {
                return Err(Error(format!(
                    "generated name override requires a declared function or object: `{name}`"
                )));
            }
            crate::identifier(name)?;
            crate::identifier(generated).map_err(|_| {
                Error(format!(
                    "generated name `{generated}` for `{name}` must be a nonempty ASCII identifier"
                ))
            })?;
        }
        Ok(())
    }
}

impl Emitter<'_> {
    pub(crate) fn generated_name(&self, name: &str) -> Result<String, Error> {
        self.names.identifier(
            self.options
                .generated_names
                .get(name)
                .map_or(name, String::as_str),
        )
    }

    pub(crate) fn validate_generated_collisions(
        &self,
        selected: &[&Declaration],
        macros: &BTreeMap<String, Option<MacroValue>>,
    ) -> Result<(), Error> {
        if self.options.generated_names.is_empty()
            && self.options.additional_objects.is_empty()
            && !self.options.prepend_enum_name
            && self.options.enum_constant_style == crate::EnumConstantStyle::Integer
        {
            return Ok(());
        }
        let mut values = BTreeMap::<String, String>::new();
        let mut insert = |rust: String, original: &str| -> Result<(), Error> {
            if let Some(previous) = values.insert(rust.clone(), original.into()) {
                return Err(Error(format!(
                    "generated Rust name `{rust}` conflicts between `{previous}` and `{original}`"
                )));
            }
            Ok(())
        };
        for declaration in selected {
            if declaration.kind != DeclarationKind::Typedef {
                insert(self.generated_name(&declaration.name)?, &declaration.name)?;
            }
        }
        for (name, object) in &self.options.additional_objects {
            insert(self.names.identifier(name)?, object.name())?;
        }
        for name in self.unit.constants.keys() {
            if let Some(generated) = self.emitted_constant_name(name, macros)? {
                insert(generated, name)?;
            }
        }
        for (name, value) in macros {
            if value.is_some() && self.options.includes_macro(name) {
                insert(self.names.identifier(name)?, name)?;
            }
        }
        Ok(())
    }
}

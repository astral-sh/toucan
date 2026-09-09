//! Select enum representations independently of declaration roots and Rust names.

use std::collections::{BTreeMap, BTreeSet};

use toucan_semantic::{DeclarationKind, Scope, TranslationUnit, TypeKind};

use crate::{Emitter, EnumConstantStyle, Error, Options, matches_name};

/// Resolve anonymous typedef names once, without changing the unit's C identity.
pub(super) fn select(
    unit: &TranslationUnit,
    options: &Options,
    lexical: &crate::lexical_names::Names,
) -> Result<BTreeSet<usize>, Error> {
    if options.rustified_enums || options.rustified_enum_patterns.is_empty() {
        return Ok(BTreeSet::new());
    }
    let mut typedef_names = BTreeMap::new();
    if options.enum_constant_style == EnumConstantStyle::Bindgen {
        return Ok(unit
            .enums
            .iter()
            .enumerate()
            .filter_map(|(id, enumeration)| {
                if enumeration.scope != Scope::File {
                    return None;
                }
                let named = enumeration.name.is_some()
                    || crate::lexical_names::Names::typedef_name(
                        unit,
                        unit.lexical_tags.enums.get(&id),
                    )
                    .is_some();
                let matches = |name| {
                    options
                        .rustified_enum_patterns
                        .iter()
                        .any(|pattern| matches_name(pattern, name))
                };
                let selected = lexical.enum_name(unit, id).is_some_and(matches)
                    || (!named
                        && enumeration
                            .variants
                            .iter()
                            .any(|variant| matches(&variant.name)));
                selected.then_some(id)
            })
            .collect());
    }
    for declaration in &unit.declarations {
        if declaration.kind == DeclarationKind::Typedef
            && let TypeKind::Enum(id) = unit.resolve(&declaration.ty)?.kind
        {
            typedef_names.entry(id).or_insert(declaration.name.as_str());
        }
    }
    let matches = |name: &str| {
        options
            .rustified_enum_patterns
            .iter()
            .any(|pattern| matches_name(pattern, name))
    };
    Ok(unit
        .enums
        .iter()
        .enumerate()
        .filter_map(|(id, enumeration)| {
            if enumeration.scope != Scope::File {
                return None;
            }
            let selected = match enumeration
                .name
                .as_deref()
                .or_else(|| typedef_names.get(&id).copied())
            {
                Some(name) => matches(name),
                None => enumeration
                    .variants
                    .iter()
                    .any(|variant| matches(&variant.name)),
            };
            selected.then_some(id)
        })
        .collect())
}

impl Emitter<'_> {
    pub(super) fn is_rustified_enum(&self, id: usize) -> bool {
        self.options.rustified_enums || self.rustified_enums.contains(&id)
    }
}

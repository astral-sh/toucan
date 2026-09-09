//! Apply optional header-cursor facts without changing default C emission.

use std::borrow::Cow;

use toucan_semantic::{Scope, TagDiscovery, TagLexicalOrigin, TagLexicalOrigins, TranslationUnit};

use crate::{EnumConstantStyle, Error, Options};

pub(super) fn hidden_record(unit: &TranslationUnit, options: &Options, id: usize) -> bool {
    options.enum_constant_style == EnumConstantStyle::Bindgen
        && unit
            .tag_discovery
            .as_ref()
            .is_some_and(|facts| matches!(facts.records.get(&id), Some(TagDiscovery::Hidden)))
}

pub(super) fn hidden_enum(unit: &TranslationUnit, options: &Options, id: usize) -> bool {
    options.enum_constant_style == EnumConstantStyle::Bindgen
        && unit
            .tag_discovery
            .as_ref()
            .is_some_and(|facts| matches!(facts.enums.get(&id), Some(TagDiscovery::Hidden)))
}

pub(super) fn enum_owner(unit: &TranslationUnit, id: usize) -> Option<Option<usize>> {
    match unit.tag_discovery.as_ref()?.enums.get(&id)? {
        TagDiscovery::Discovered { record, .. } => Some(*record),
        TagDiscovery::Hidden => None,
    }
}

/// Make a naming view; actual lexical origins stay in the semantic unit.
pub(super) fn origins(unit: &TranslationUnit) -> Result<Cow<'_, TagLexicalOrigins>, Error> {
    let Some(facts) = &unit.tag_discovery else {
        return Ok(Cow::Borrowed(&unit.lexical_tags));
    };
    if facts.records.len().saturating_add(facts.enums.len()) > 1_000_000 {
        return Err(Error(
            "tag discovery exceeds the 1000000-entry limit".into(),
        ));
    }
    let mut origins = unit.lexical_tags.clone();
    for (records, entries) in [(true, &facts.records), (false, &facts.enums)] {
        for (&id, discovery) in entries {
            let scope = if records {
                unit.records.get(id).map(|item| item.scope)
            } else {
                unit.enums.get(id).map(|item| item.scope)
            };
            if scope != Some(Scope::File) {
                return Err(Error(
                    "tag discovery references an invalid file-scope tag".into(),
                ));
            }
            let map = if records {
                &mut origins.records
            } else {
                &mut origins.enums
            };
            match discovery {
                TagDiscovery::Hidden => {
                    map.remove(&id);
                }
                TagDiscovery::Discovered { record, order, .. } => {
                    if let Some(owner) = record
                        && (unit
                            .records
                            .get(*owner)
                            .is_none_or(|item| item.scope != Scope::File)
                            || matches!(facts.records.get(owner), Some(TagDiscovery::Hidden)))
                    {
                        return Err(Error(
                            "tag discovery references an invalid or hidden record owner".into(),
                        ));
                    }
                    let origin = map.entry(id).or_insert(TagLexicalOrigin {
                        record: *record,
                        order: *order,
                        prior_file_declaration: false,
                        typedef_declaration: None,
                    });
                    origin.record = *record;
                    origin.order = *order;
                    origin.prior_file_declaration = false;
                }
            }
        }
    }
    Ok(Cow::Owned(origins))
}

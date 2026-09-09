//! Apply file and callback policies before invoking the binding emitter.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use regex::RegexSet;
use toucan::semantic::{DeclarationKind, DeclarationTarget};
use toucan::{BindingOptions, BindingSelection, Compilation};

use crate::callbacks::{ItemInfo, ItemKind, ParseCallbacks};
use crate::{BindgenError, configuration};

#[derive(Default)]
pub(crate) struct Patterns {
    pub(crate) files: Option<RegexSet>,
    pub(crate) types: Option<RegexSet>,
    pub(crate) functions: Option<RegexSet>,
    pub(crate) vars: Option<RegexSet>,
}

impl Patterns {
    pub(crate) fn new(
        files: &[String],
        types: &[String],
        functions: &[String],
        vars: &[String],
    ) -> Result<Self, BindgenError> {
        Ok(Self {
            files: patterns(files, "file")?,
            types: patterns(types, "type")?,
            functions: patterns(functions, "function")?,
            vars: patterns(vars, "var")?,
        })
    }

    pub(crate) fn is_restricted(&self) -> bool {
        self.files.is_some() || self.has_names()
    }

    pub(crate) fn has_names(&self) -> bool {
        self.types.is_some() || self.functions.is_some() || self.vars.is_some()
    }

    pub(crate) fn matches_file(&self, path: &std::path::Path) -> bool {
        self.files.as_ref().is_some_and(|patterns| {
            // The facade's compiler aliases are not written header declarations.
            path != std::path::Path::new("<builtin>/integer-types.h")
                && patterns.is_match(path.to_string_lossy().as_ref())
        })
    }

    pub(crate) fn matches_var(&self, name: &str) -> bool {
        self.vars
            .as_ref()
            .is_some_and(|patterns| patterns.is_match(name))
    }
}

fn patterns(patterns: &[String], kind: &str) -> Result<Option<RegexSet>, BindgenError> {
    if patterns.is_empty() {
        return Ok(None);
    }
    RegexSet::new(patterns.iter().map(|pattern| format!("^(?:{pattern})$")))
        .map(Some)
        .map_err(|error| configuration(format!("invalid allowlist_{kind} pattern: {error}")))
}

/// Written roots plus any extra names emitted for a single C object.
#[derive(Default)]
pub(crate) struct SelectedOccurrences {
    pub(crate) offsets: BTreeSet<usize>,
    pub(crate) additional_names: BTreeMap<usize, String>,
}

pub(crate) fn apply(
    compilation: &Compilation,
    patterns: &Patterns,
    callbacks: &[Rc<dyn ParseCallbacks>],
    options: &mut BindingOptions,
) -> Result<SelectedOccurrences, BindgenError> {
    let Some(origins) = compilation.declaration_origins() else {
        return Err(configuration(
            "declaration origins were not captured for selection",
        ));
    };
    let physical = compilation.preprocessed().file_origins();
    let matches_file = |offset| -> bool {
        !patterns.is_restricted()
            || physical
                .and_then(|catalog| catalog.source_name(offset))
                .is_some_and(|path| patterns.matches_file(path))
    };
    let mut selection = patterns
        .is_restricted()
        .then(Box::<BindingSelection>::default);
    if (patterns.types.is_some() || patterns.vars.is_some())
        && let Some(selection) = &mut selection
    {
        selection
            .extend_tags(
                compilation.unit(),
                options,
                |name| {
                    patterns
                        .types
                        .as_ref()
                        .is_some_and(|patterns| patterns.is_match(name))
                },
                |name| patterns.matches_var(name),
            )
            .map_err(|error| configuration(error.to_string()))?;
    }
    if let Some(selection) = &mut selection {
        selection.retain_type_dependencies = patterns.has_names();
    }
    let mut occurrences = SelectedOccurrences::default();
    let mut generated = BTreeSet::new();
    let mut records = BTreeSet::new();
    let mut enums = BTreeSet::new();
    for origin in origins.entries() {
        let discovery =
            compilation
                .unit()
                .tag_discovery
                .as_ref()
                .and_then(|facts| match origin.target() {
                    DeclarationTarget::Record(id) => facts.records.get(&id),
                    DeclarationTarget::Enum(id)
                    | DeclarationTarget::Enumerator {
                        enumeration: id, ..
                    } => facts.enums.get(&id),
                    _ => None,
                });
        let selected = match discovery {
            Some(toucan::semantic::TagDiscovery::Hidden) => continue,
            Some(toucan::semantic::TagDiscovery::Discovered { offset, .. }) => {
                matches_file(*offset)
            }
            None => matches_file(origin.source().range().start),
        };
        match origin.target() {
            DeclarationTarget::Declaration(index) => {
                let declaration = &compilation.unit().declarations[index];
                let kind = match declaration.kind {
                    DeclarationKind::Function
                        if !origin.is_inline()
                            && !declaration
                                .inline_facts
                                .is_some_and(|facts| facts.has_inline_definition) =>
                    {
                        Some(ItemKind::Function)
                    }
                    DeclarationKind::Variable if origin.is_external() => Some(ItemKind::Var),
                    _ => None,
                };
                // Invoke before filtering, but retain the first selected occurrence.
                let renamed = kind.and_then(|kind| {
                    callbacks.iter().rev().find_map(|callback| {
                        callback.generated_name_override(ItemInfo {
                            name: &declaration.name,
                            kind,
                        })
                    })
                });
                if renamed.as_ref().is_some_and(String::is_empty) {
                    return Err(configuration(format!(
                        "generated name for `{}` cannot be empty",
                        declaration.name
                    )));
                }
                let name = renamed.as_deref().unwrap_or(&declaration.name);
                let selected = selected
                    || match declaration.kind {
                        DeclarationKind::Function if kind.is_some() => patterns
                            .functions
                            .as_ref()
                            .is_some_and(|patterns| patterns.is_match(name)),
                        DeclarationKind::Variable => patterns.matches_var(name),
                        DeclarationKind::Typedef => {
                            patterns
                                .types
                                .as_ref()
                                .is_some_and(|patterns| patterns.is_match(name))
                                && physical
                                    .and_then(|catalog| {
                                        catalog.source_name(origin.source().range().start)
                                    })
                                    .is_none_or(|path| {
                                        path != std::path::Path::new("<builtin>/integer-types.h")
                                    })
                        }
                        _ => false,
                    };
                if selected && (kind.is_some() || declaration.kind != DeclarationKind::Function) {
                    let offset = origin.source().range().start;
                    if declaration.kind == DeclarationKind::Variable
                        && generated.contains(&index)
                        && options
                            .generated_names
                            .get(&declaration.name)
                            .unwrap_or(&declaration.name)
                            != name
                    {
                        occurrences.additional_names.insert(offset, name.to_owned());
                    }
                    occurrences.offsets.insert(offset);
                    if let Some(selection) = &mut selection {
                        selection.declarations.insert(index);
                    }
                    if generated.insert(index)
                        && let Some(renamed) = renamed
                    {
                        options
                            .generated_names
                            .insert(declaration.name.clone(), renamed);
                    }
                }
            }
            DeclarationTarget::Record(id) => {
                let first = !origin.is_reference() && records.insert(id);
                if selected
                    && (first || origin.is_definition())
                    && let Some(selection) = &mut selection
                {
                    selection.records.insert(id);
                }
            }
            DeclarationTarget::Enum(id) => {
                let first = !origin.is_reference() && enums.insert(id);
                if selected
                    && (first || origin.is_definition())
                    && let Some(selection) = &mut selection
                {
                    selection.enums.insert(id);
                }
            }
            DeclarationTarget::Enumerator {
                enumeration,
                variant,
            } => {
                if selected && let Some(selection) = &mut selection {
                    selection.constants.insert(
                        compilation.unit().enums[enumeration].variants[variant]
                            .name
                            .clone(),
                    );
                }
            }
            _ => return Err(configuration("unsupported declaration-origin category")),
        }
    }
    // Semantic compiler helper types have no written declaration cursor.
    if let Some(selection) = &mut selection {
        selection.records.retain(|id| records.contains(id));
        selection.enums.retain(|id| enums.contains(id));
    }
    // Bindgen applies function blocklists to the callback-adjusted item name.
    // Translate those policies back to C symbols for the core emitter.
    if !callbacks.is_empty() && !options.blocklist_functions.is_empty() {
        options.blocklist_functions = compilation
            .unit()
            .declarations
            .iter()
            .filter(|declaration| declaration.kind == DeclarationKind::Function)
            .filter(|declaration| {
                let name = options
                    .generated_names
                    .get(&declaration.name)
                    .unwrap_or(&declaration.name);
                options.blocklist_functions.iter().any(|pattern| {
                    pattern
                        .strip_suffix('*')
                        .map_or(pattern == name, |prefix| name.starts_with(prefix))
                })
            })
            .map(|declaration| declaration.name.clone())
            .collect();
    }
    options.selection = selection;
    Ok(occurrences)
}

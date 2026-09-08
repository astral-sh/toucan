//! Apply file and callback policies before invoking the binding emitter.

use std::collections::BTreeSet;
use std::rc::Rc;

use regex::RegexSet;
use toucan::semantic::{DeclarationKind, DeclarationTarget};
use toucan::{BindingOptions, BindingSelection, Compilation};

use crate::callbacks::{ItemInfo, ItemKind, ParseCallbacks};
use crate::{BindgenError, configuration};

pub(crate) fn file_patterns(patterns: &[String]) -> Result<Option<RegexSet>, BindgenError> {
    if patterns.is_empty() {
        return Ok(None);
    }
    RegexSet::new(patterns.iter().map(|pattern| format!("^(?:{pattern})$")))
        .map(Some)
        .map_err(|error| configuration(format!("invalid allowlist_file pattern: {error}")))
}

pub(crate) fn apply(
    compilation: &Compilation,
    files: Option<&RegexSet>,
    callbacks: &[Rc<dyn ParseCallbacks>],
    options: &mut BindingOptions,
) -> Result<(), BindgenError> {
    let Some(origins) = compilation.declaration_origins() else {
        return Err(configuration(
            "declaration origins were not captured for selection",
        ));
    };
    let physical = compilation.preprocessed().file_origins();
    let matches_file = |offset| -> bool {
        files.is_none_or(|patterns| {
            physical
                .and_then(|catalog| catalog.source_name(offset))
                .is_some_and(|path| {
                    // This facade prelude supplies compiler builtin aliases; it
                    // is not a header supplied by the build script or its includes.
                    path != std::path::Path::new("<builtin>/integer-types.h")
                        && patterns.is_match(path.to_string_lossy().as_ref())
                })
        })
    };
    let mut selection = files.map(|_| Box::<BindingSelection>::default());
    // An inline definition makes earlier declarations inline candidates too,
    // matching libclang's separate definition lookup before callback invocation.
    let inline_bodies: BTreeSet<_> = origins
        .entries()
        .iter()
        .filter_map(|origin| {
            if origin.is_definition()
                && origin.is_inline()
                && let DeclarationTarget::Declaration(index) = origin.target()
            {
                return Some(index);
            }
            None
        })
        .collect();
    let mut generated = BTreeSet::new();
    let mut functions = BTreeSet::new();
    let mut eligible_functions = BTreeSet::new();
    let mut records = BTreeSet::new();
    let mut enums = BTreeSet::new();
    for origin in origins.entries() {
        let selected = matches_file(origin.source().range().start);
        match origin.target() {
            DeclarationTarget::Declaration(index) => {
                let declaration = &compilation.unit().declarations[index];
                if declaration.kind == DeclarationKind::Function {
                    functions.insert(index);
                }
                let kind = match declaration.kind {
                    DeclarationKind::Function
                        if !origin.is_inline() && !inline_bodies.contains(&index) =>
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
                if selected && (kind.is_some() || declaration.kind != DeclarationKind::Function) {
                    if declaration.kind == DeclarationKind::Function {
                        eligible_functions.insert(index);
                    }
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
    if let (Some(selection), Some(files), Some(physical)) = (&mut selection, files, physical) {
        for name in compilation.preprocessed().macros.keys() {
            if physical
                .macro_definition_name(name)
                .is_some_and(|path| files.is_match(path.to_string_lossy().as_ref()))
            {
                selection.macros.insert(name.clone());
            }
        }
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
    if selection.is_none() {
        for index in functions.difference(&eligible_functions) {
            options
                .blocklist_functions
                .push(compilation.unit().declarations[*index].name.clone());
        }
    }
    options.selection = selection;
    Ok(())
}

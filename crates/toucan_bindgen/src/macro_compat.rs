//! Apply ordered macro evaluation before file selection and Rust projection.

use std::collections::BTreeMap;

use regex::RegexSet;
use toucan::{BindingOptions, Compilation, MacroValue, SkippedMacro};

use crate::macro_projection::{self, MacroTypeVariation};
use crate::macro_values::{Context, ErrorKind, Limits};
use crate::{BindgenError, configuration};

pub(crate) struct MacroBindings {
    pub(crate) values: BTreeMap<String, Option<MacroValue>>,
    pub(crate) skipped: Vec<SkippedMacro>,
}

/// Match the first successfully parsed definition's header after context updates.
pub(crate) fn evaluate(
    compilation: &Compilation,
    files: Option<&RegexSet>,
    callbacks_present: bool,
    options: &mut BindingOptions,
    variation: MacroTypeVariation,
    fit: bool,
) -> Result<MacroBindings, BindgenError> {
    let preprocessed = compilation.preprocessed();
    let definitions = preprocessed
        .macro_definitions()
        .ok_or_else(|| configuration("written macro definitions were not captured"))?;
    let profile = compilation
        .unit()
        .profile()
        .map_err(|error| configuration(error.to_string()))?;
    let mut context = Context::new(Limits::default(), profile);
    let mut values = BTreeMap::new();
    let mut skipped = BTreeMap::new();
    // File selection's final-macro origins do not describe historical output.
    if let Some(selection) = &mut options.selection {
        selection.macros.clear();
    }
    for definition in definitions {
        let name = definition.name();
        let selected = files.is_none_or(|files| {
            files.is_match(definition.accessed_path().to_string_lossy().as_ref())
        });
        let function_like = callbacks_present
            && preprocessed
                .macros
                .get(name)
                .is_some_and(|definition| definition.parameters.is_some());
        let result = context.define(name, definition.definition(), function_like);
        let omitted = match result {
            Ok(Some(parsed)) => {
                // Context updates happen even for duplicate or excluded definitions.
                if !parsed.first_definition || !selected {
                    continue;
                }
                match macro_projection::project(parsed.value, variation, fit) {
                    Ok(Some(value)) => {
                        skipped.remove(name);
                        values.insert(name.to_owned(), Some(value));
                        if let Some(selection) = &mut options.selection {
                            selection.macros.insert(name.to_owned());
                        }
                        continue;
                    }
                    Ok(None) => "literal evaluation produced no scalar value".to_owned(),
                    Err(reason) => reason.to_owned(),
                }
            }
            Ok(None) => {
                if !selected {
                    continue;
                }
                "function-like macro in the final environment".to_owned()
            }
            Err(error) => {
                let reason = match error.kind {
                    ErrorKind::Syntax => "unsupported literal expression syntax",
                    ErrorKind::InvalidLiteral => "unsupported literal token",
                    ErrorKind::UnknownIdentifier => {
                        "identifier has no previously parsed macro value"
                    }
                    ErrorKind::Keyword => "Clang classifies the token as a keyword",
                    ErrorKind::DivisionByZero => "integer division or remainder by zero",
                    ErrorKind::SourceLimit
                    | ErrorKind::TokenLimit
                    | ErrorKind::DepthLimit
                    | ErrorKind::StringLimit
                    | ErrorKind::ContextLimit
                    | ErrorKind::WorkLimit => {
                        return Err(configuration(format!(
                            "macro `{name}` at {} exceeds a macro evaluation resource limit ({:?})",
                            definition.location(),
                            error.kind
                        )));
                    }
                };
                if !selected {
                    continue;
                }
                match error.offset {
                    Some(offset) => format!("{reason} at replacement byte {offset}"),
                    None => reason.to_owned(),
                }
            }
        };
        if selected && !values.contains_key(name) {
            skipped
                .entry(name.to_owned())
                .or_insert_with(|| format!("{omitted} (definition at {})", definition.location()));
        }
    }
    Ok(MacroBindings {
        values,
        skipped: skipped
            .into_iter()
            .map(|(name, reason)| SkippedMacro { name, reason })
            .collect(),
    })
}

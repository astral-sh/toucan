//! Project optional source signature dependencies through Builder selection.

use std::collections::BTreeSet;

use toucan::{BindingOptions, Compilation};

use crate::selection::Patterns;
use crate::{BindgenError, configuration};

/// Retain signature typedefs selected by their owning declaration or physical header.
/// File matching shares the declaration policy, including the exclusion of builtin aliases.
pub(crate) fn apply(
    compilation: &Compilation,
    patterns: &Patterns,
    selected_occurrences: Option<&BTreeSet<usize>>,
    options: &mut BindingOptions,
) -> Result<(), BindgenError> {
    let dependencies = compilation
        .parameter_type_dependencies()
        .ok_or_else(|| configuration("parameter-type dependencies were not captured"))?;
    if let Some(selection) = &mut options.selection {
        let origins = compilation
            .preprocessed()
            .file_origins()
            .filter(|_| patterns.files.is_some());
        for occurrence in dependencies.occurrences() {
            let selected = selected_occurrences
                .is_some_and(|offsets| offsets.contains(&occurrence.owner_source().range().start))
                || origins
                    .and_then(|origins| origins.source_name(occurrence.source().range().start))
                    .is_some_and(|path| patterns.matches_file(path));
            if selected {
                selection
                    .typedefs
                    .extend(occurrence.typedefs().iter().cloned());
            }
        }
    }
    if !dependencies.records().is_empty() || !dependencies.typedefs().is_empty() {
        options.type_dependencies = Some(Box::new(toucan::TypeDependencies {
            records: dependencies.records().clone(),
            typedefs: dependencies.typedefs().clone(),
        }));
    }
    Ok(())
}

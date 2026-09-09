//! Project optional source signature dependencies through Builder selection.

use std::collections::BTreeSet;

use regex::RegexSet;
use toucan::{BindingOptions, Compilation};

use crate::{BindgenError, configuration};

pub(crate) fn apply(
    compilation: &Compilation,
    files: Option<&RegexSet>,
    selected_occurrences: Option<&BTreeSet<usize>>,
    options: &mut BindingOptions,
) -> Result<(), BindgenError> {
    let dependencies = compilation
        .parameter_type_dependencies()
        .ok_or_else(|| configuration("parameter-type dependencies were not captured"))?;
    if let Some(selection) = &mut options.selection {
        let origins = compilation.preprocessed().file_origins();
        for occurrence in dependencies.occurrences() {
            let selected = selected_occurrences
                .is_some_and(|offsets| offsets.contains(&occurrence.owner_source().range().start))
                || files.is_some_and(|files| {
                    origins
                        .and_then(|origins| origins.source_name(occurrence.source().range().start))
                        .is_some_and(|path| {
                            path != std::path::Path::new("<builtin>/integer-types.h")
                                && files.is_match(path.to_string_lossy().as_ref())
                        })
                });
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

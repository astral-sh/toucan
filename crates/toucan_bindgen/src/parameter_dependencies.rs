//! Project optional source signature dependencies through Builder selection.

use regex::RegexSet;
use toucan::{BindingOptions, Compilation};

use crate::{BindgenError, configuration};

pub(crate) fn apply(
    compilation: &Compilation,
    files: Option<&RegexSet>,
    options: &mut BindingOptions,
) -> Result<(), BindgenError> {
    let dependencies = compilation
        .parameter_type_dependencies()
        .ok_or_else(|| configuration("parameter-type dependencies were not captured"))?;
    if let Some(files) = files {
        let origins = compilation.preprocessed().file_origins().ok_or_else(|| {
            configuration("file origins were not captured for parameter dependencies")
        })?;
        let selection = options
            .selection
            .as_mut()
            .ok_or_else(|| configuration("parameter dependencies require file selection roots"))?;
        for occurrence in dependencies.occurrences() {
            if origins
                .source_name(occurrence.source().range().start)
                .is_some_and(|path| {
                    path != std::path::Path::new("<builtin>/integer-types.h")
                        && files.is_match(path.to_string_lossy().as_ref())
                })
            {
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

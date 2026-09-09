//! Choose the first written object in the selected headers.
use crate::{BindgenError, configuration};
use regex::RegexSet;
use toucan::{BindingOptions, Compilation};

pub(crate) fn select(
    compilation: &Compilation,
    files: Option<&RegexSet>,
    options: &mut BindingOptions,
) -> Result<(), BindgenError> {
    let objects = compilation
        .object_values()
        .ok_or_else(|| configuration("object values were not captured"))?;
    let physical = compilation.preprocessed().file_origins();
    for object in objects.entries() {
        let selected = files.is_none_or(|patterns| {
            physical
                .and_then(|catalog| catalog.source_name(object.offset()))
                .is_some_and(|path| patterns.is_match(path.to_string_lossy().as_ref()))
        });
        if selected && !options.object_bindings.contains_key(object.name()) {
            options
                .object_bindings
                .insert(object.name().into(), object.clone());
        }
    }
    Ok(())
}

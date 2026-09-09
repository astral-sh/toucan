//! Choose the first written object in the selected headers.
use crate::{BindgenError, configuration};
use std::collections::BTreeSet;
use toucan::{BindingOptions, Compilation};

pub(crate) fn select(
    compilation: &Compilation,
    selected_occurrences: Option<&BTreeSet<usize>>,
    options: &mut BindingOptions,
) -> Result<(), BindgenError> {
    let objects = compilation
        .object_values()
        .ok_or_else(|| configuration("object values were not captured"))?;
    for object in objects.entries() {
        let selected =
            selected_occurrences.is_none_or(|offsets| offsets.contains(&object.offset()));
        if selected && !options.object_bindings.contains_key(object.name()) {
            options
                .object_bindings
                .insert(object.name().into(), object.clone());
        }
    }
    Ok(())
}

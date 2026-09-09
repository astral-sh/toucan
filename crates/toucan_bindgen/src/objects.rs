//! Choose the first written object in the selected headers.
use crate::{BindgenError, configuration};
use std::collections::btree_map::Entry;
use toucan::{BindingOptions, Compilation};

pub(crate) fn select(
    compilation: &Compilation,
    selected_occurrences: Option<&crate::selection::SelectedOccurrences>,
    options: &mut BindingOptions,
) -> Result<(), BindgenError> {
    let objects = compilation
        .object_values()
        .ok_or_else(|| configuration("object values were not captured"))?;
    for object in objects.entries() {
        let selected =
            selected_occurrences.is_none_or(|selected| selected.offsets.contains(&object.offset()));
        if let Some(name) = selected_occurrences
            .and_then(|selected| selected.additional_names.get(&object.offset()))
        {
            match options.additional_objects.entry(name.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(object.clone());
                }
                Entry::Occupied(entry) if entry.get().declaration() == object.declaration() => {}
                Entry::Occupied(_) => {
                    return Err(configuration(format!(
                        "generated object name `{name}` conflicts between C declarations"
                    )));
                }
            }
            continue;
        }
        if selected && !options.object_bindings.contains_key(object.name()) {
            options
                .object_bindings
                .insert(object.name().into(), object.clone());
        }
    }
    Ok(())
}

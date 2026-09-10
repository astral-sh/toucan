//! Ordered written definitions, independently of the final macro environment.

use std::path::Path;
use std::sync::Arc;

use crate::file_origins::PathNames;
use crate::{InputFile, Macro, OriginKind, SourceLocation};

/// A successful, active `#define` encountered while reading source.
///
/// Repeated definitions are separate entries, and `#undef` does not remove them.
/// Replacement tokens are unexpanded; subsequent expansion still uses the normal
/// current macro environment. Configured predefined macros have no written entry.
#[derive(Clone, Debug)]
pub struct MacroDefinition {
    name: String,
    definition: Macro,
    location: SourceLocation,
    accessed: Arc<Path>,
}

impl MacroDefinition {
    /// The written macro identifier.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Parameters and unexpanded replacement, preserved at this definition.
    pub fn definition(&self) -> &Macro {
        &self.definition
    }

    /// Physical input location, before diagnostic `#line` remapping.
    pub fn location(&self) -> &SourceLocation {
        &self.location
    }

    /// Compiler-visible access spelling, before diagnostic `#line` remapping.
    pub fn accessed_path(&self) -> &Path {
        &self.accessed
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MacroDefinitions {
    entries: Vec<MacroDefinition>,
    paths: PathNames,
    retained_bytes: usize,
}

impl MacroDefinitions {
    pub(crate) fn entries(&self) -> &[MacroDefinition] {
        &self.entries
    }

    /// Check the retained-data budget before cloning a definition or its paths.
    pub(crate) fn record(
        &mut self,
        name: &str,
        definition: &Macro,
        input: InputFile<'_>,
        position: (usize, usize),
        byte_limit: usize,
    ) -> Result<(), String> {
        // Charge paths per occurrence even though the interner shares them.
        // This conservatively bounds definition data, not allocator overhead.
        let sizes = [
            size_of::<MacroDefinition>(),
            name.len(),
            input.physical.as_os_str().len(),
            input.accessed.as_os_str().len(),
        ];
        let bytes = sizes
            .into_iter()
            .chain(definition.retained_sizes())
            .try_fold(self.retained_bytes, usize::checked_add)
            .filter(|&bytes| bytes <= byte_limit)
            .ok_or("macro definition capture byte limit exceeded")?;
        self.entries.push(MacroDefinition {
            name: name.into(),
            definition: definition.clone(),
            location: SourceLocation {
                path: self.paths.intern(input.physical),
                line: position.0,
                column: position.1,
                kind: OriginKind::Directive,
            },
            accessed: self.paths.intern(input.accessed),
        });
        self.retained_bytes = bytes;
        Ok(())
    }
}

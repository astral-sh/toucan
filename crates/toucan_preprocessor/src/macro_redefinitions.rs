//! Explicit compatibility policy and retained incompatible redefinition records.

use std::path::Path;
use std::sync::Arc;

use crate::file_origins::PathNames;
use crate::{InputFile, OriginKind, SourceLocation};

/// How to handle a valid definition incompatible with the active macro.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MacroRedefinitionPolicy {
    /// Reject the redefinition with the existing preprocessing diagnostic.
    #[default]
    Strict,
    /// Use the replacement definition and retain a bounded diagnostic record.
    RecordAndReplace,
}

/// An incompatible definition accepted under an explicit compatibility policy.
///
/// Only the current location is recorded. There is no inferred previous location;
/// configured predefined definitions have no source location at all.
#[derive(Clone, Debug)]
pub struct MacroRedefinition {
    name: String,
    location: Option<SourceLocation>,
    accessed: Option<Arc<Path>>,
}

impl MacroRedefinition {
    /// The redefined macro's C identifier.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Current physical source location, before diagnostic `#line` remapping.
    /// None denotes a configured predefined definition without a source site.
    pub fn location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }

    /// Current accessed path spelling, when this definition came from source.
    pub fn accessed_path(&self) -> Option<&Path> {
        self.accessed.as_deref()
    }
}

pub(crate) struct DefinitionLocation<'a> {
    pub(crate) input: InputFile<'a>,
    pub(crate) line: usize,
    pub(crate) column: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MacroRedefinitions {
    entries: Vec<MacroRedefinition>,
    paths: PathNames,
    retained_bytes: usize,
}

impl MacroRedefinitions {
    pub(crate) fn entries(&self) -> &[MacroRedefinition] {
        &self.entries
    }

    /// Charge each event and its path payload before allocating retained data.
    pub(crate) fn record(
        &mut self,
        name: &str,
        location: Option<DefinitionLocation<'_>>,
        byte_limit: usize,
    ) -> Result<(), String> {
        let path_bytes = location.as_ref().map_or(0, |location| {
            location
                .input
                .physical
                .as_os_str()
                .len()
                .saturating_add(location.input.accessed.as_os_str().len())
        });
        let bytes = [size_of::<MacroRedefinition>(), name.len(), path_bytes]
            .into_iter()
            .try_fold(self.retained_bytes, usize::checked_add)
            .filter(|&bytes| bytes <= byte_limit)
            .ok_or("macro redefinition record byte limit exceeded")?;
        let (location, accessed) = location.map_or((None, None), |location| {
            (
                Some(SourceLocation {
                    path: self.paths.intern(location.input.physical),
                    line: location.line,
                    column: location.column,
                    kind: OriginKind::Directive,
                }),
                Some(self.paths.intern(location.input.accessed)),
            )
        });
        self.entries.push(MacroRedefinition {
            name: name.into(),
            location,
            accessed,
        });
        self.retained_bytes = bytes;
        Ok(())
    }
}

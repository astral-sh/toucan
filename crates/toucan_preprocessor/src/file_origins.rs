//! Opt-in physical file provenance for header-based binding selection.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use crate::{OriginKind, SourceLocation};

/// A contiguous output range produced while reading one physical or virtual header.
#[derive(Clone, Debug)]
pub struct FileMapping {
    generated: Range<usize>,
    path: Arc<Path>,
}

impl FileMapping {
    /// Byte range in [`crate::Preprocessed::source`].
    pub fn generated(&self) -> &Range<usize> {
        &self.generated
    }

    /// Input header path, unaffected by diagnostic `#line` names.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Physical input files and the origins of final active macro definitions.
///
/// Captured only when [`crate::Config::record_file_origins`] is enabled.
/// This preserves actual input paths; ordinary diagnostic mappings still honor
/// `#line` and GNU line-marker names. Command-line macros have no source origin.
#[derive(Clone, Debug, Default)]
pub struct FileOrigins {
    mappings: Vec<FileMapping>,
    macros: BTreeMap<String, SourceLocation>,
    paths: BTreeSet<Arc<Path>>,
}

impl FileOrigins {
    /// Ordered physical input ranges, coalesced when adjacent paths match.
    pub fn mappings(&self) -> &[FileMapping] {
        &self.mappings
    }

    /// Physical header containing the token at an output byte offset.
    pub fn source_file(&self, offset: usize) -> Option<&Path> {
        let index = self
            .mappings
            .partition_point(|entry| entry.generated.end <= offset);
        self.mappings
            .get(index)
            .filter(|entry| entry.generated.contains(&offset))
            .map(|entry| entry.path.as_ref())
    }

    /// Source location of the last active definition; absent after `#undef`.
    /// Physical line/column values precede diagnostic line remapping.
    pub fn macro_definition(&self, name: &str) -> Option<&SourceLocation> {
        self.macros.get(name)
    }

    fn intern_path(&mut self, path: &Path) -> Arc<Path> {
        if let Some(path) = self.paths.get(path) {
            return Arc::clone(path);
        }
        let path: Arc<Path> = Arc::from(path);
        self.paths.insert(Arc::clone(&path));
        path
    }

    pub(crate) fn append(&mut self, generated: Range<usize>, path: &Path) {
        if generated.is_empty() {
            return;
        }
        if let Some(last) = self.mappings.last_mut()
            && last.path.as_ref() == path
            && last.generated.end == generated.start
        {
            last.generated.end = generated.end;
            return;
        }
        let path = self.intern_path(path);
        self.mappings.push(FileMapping { generated, path });
    }

    pub(crate) fn define(&mut self, name: &str, path: &Path, line: usize, column: usize) {
        let path = self.intern_path(path);
        self.macros.insert(
            name.into(),
            SourceLocation {
                path,
                line,
                column,
                kind: OriginKind::Directive,
            },
        );
    }

    pub(crate) fn undefine(&mut self, name: &str) {
        self.macros.remove(name);
    }
}

//! Opt-in physical file provenance for header-based binding selection.

use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use crate::{OriginKind, SourceLocation};

/// A contiguous output range produced while reading one physical or virtual header.
#[derive(Clone, Debug)]
pub struct FileMapping {
    generated: Range<usize>,
    path: Arc<Path>,
    accessed: Arc<Path>,
}

impl FileMapping {
    /// Byte range in [`crate::Preprocessed::source`].
    pub fn generated(&self) -> &Range<usize> {
        &self.generated
    }

    /// Canonical identity for filesystem reads; supplied name for in-memory inputs.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Compiler-visible input spelling, before diagnostic `#line` remapping.
    /// Clang reuses the first name registered for a physical file; GNU uses each access.
    pub fn accessed_path(&self) -> &Path {
        &self.accessed
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
    macros: BTreeMap<String, (SourceLocation, Arc<Path>)>,
    paths: BTreeSet<PathName>,
}

impl FileOrigins {
    /// Ordered ranges, coalesced only when physical identity and accessed spelling match.
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

    /// Compiler-visible header name at an output byte offset, before `#line` remapping.
    pub fn source_name(&self, offset: usize) -> Option<&Path> {
        let index = self
            .mappings
            .partition_point(|entry| entry.generated.end <= offset);
        self.mappings
            .get(index)
            .filter(|entry| entry.generated.contains(&offset))
            .map(|entry| entry.accessed.as_ref())
    }

    /// Source location of the last active definition; absent after `#undef`.
    /// Physical line/column values precede diagnostic line remapping.
    pub fn macro_definition(&self, name: &str) -> Option<&SourceLocation> {
        self.macros.get(name).map(|entry| &entry.0)
    }

    /// Compiler-visible input name of the final active macro definition.
    pub fn macro_definition_name(&self, name: &str) -> Option<&Path> {
        self.macros.get(name).map(|entry| entry.1.as_ref())
    }

    fn intern_path(&mut self, path: &Path) -> Arc<Path> {
        if let Some(path) = self.paths.get(path.as_os_str()) {
            return Arc::clone(&path.0);
        }
        let path: Arc<Path> = Arc::from(path);
        self.paths.insert(PathName(Arc::clone(&path)));
        path
    }

    pub(crate) fn append(&mut self, generated: Range<usize>, path: &Path, accessed: &Path) {
        if generated.is_empty() {
            return;
        }
        if let Some(last) = self.mappings.last_mut()
            && last.path.as_ref() == path
            && last.accessed.as_os_str() == accessed.as_os_str()
            && last.generated.end == generated.start
        {
            last.generated.end = generated.end;
            return;
        }
        let path = self.intern_path(path);
        let accessed = self.intern_path(accessed);
        self.mappings.push(FileMapping {
            generated,
            path,
            accessed,
        });
    }

    pub(crate) fn define(
        &mut self,
        name: &str,
        path: &Path,
        accessed: &Path,
        line: usize,
        column: usize,
    ) {
        let path = self.intern_path(path);
        let accessed = self.intern_path(accessed);
        self.macros.insert(
            name.into(),
            (
                SourceLocation {
                    path,
                    line,
                    column,
                    kind: OriginKind::Directive,
                },
                accessed,
            ),
        );
    }

    pub(crate) fn undefine(&mut self, name: &str) {
        self.macros.remove(name);
    }
}

// Path's component-based comparison equates `a/./b` and `a/b`. Source names
// must retain their exact spelling, so interning compares the underlying OsStr.
#[derive(Clone, Debug)]
struct PathName(Arc<Path>);

impl Borrow<OsStr> for PathName {
    fn borrow(&self) -> &OsStr {
        self.0.as_os_str()
    }
}
impl PartialEq for PathName {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for PathName {}
impl PartialOrd for PathName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for PathName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.as_os_str().cmp(other.0.as_os_str())
    }
}

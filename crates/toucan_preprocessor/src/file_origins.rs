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
    system_include: bool,
}

impl FileMapping {
    /// Byte range in [`crate::Preprocessed::source`].
    pub fn generated(&self) -> &Range<usize> {
        &self.generated
    }

    /// Canonical read path, or the supplied name for in-memory inputs.
    /// Distinct hard links retain their distinct canonical paths here.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Compiler-visible input spelling, before diagnostic `#line` remapping.
    /// Clang reuses the first name registered for a physical file; GNU uses each access.
    pub fn accessed_path(&self) -> &Path {
        &self.accessed
    }

    /// Initial system status inherited from the includer or a system search root.
    /// Later `system_header` pragmas and line markers are separate source state.
    pub fn is_system_include(&self) -> bool {
        self.system_include
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
    paths: PathNames,
}

impl FileOrigins {
    /// Ordered ranges, coalesced when paths and initial include class match.
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

    /// Initial include class at a generated offset; see `FileMapping::is_system_include`.
    pub fn source_is_system_include(&self, offset: usize) -> Option<bool> {
        let index = self
            .mappings
            .partition_point(|entry| entry.generated.end <= offset);
        self.mappings
            .get(index)
            .filter(|entry| entry.generated.contains(&offset))
            .map(|entry| entry.system_include)
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
        self.paths.intern(path)
    }

    pub(crate) fn append(
        &mut self,
        generated: Range<usize>,
        path: &Path,
        accessed: &Path,
        system_include: bool,
    ) {
        if generated.is_empty() {
            return;
        }
        if let Some(last) = self.mappings.last_mut()
            && last.path.as_ref() == path
            && last.accessed.as_os_str() == accessed.as_os_str()
            && last.system_include == system_include
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
            system_include,
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

/// Shared path storage that preserves exact source-name spelling.
#[derive(Clone, Debug, Default)]
pub(crate) struct PathNames(BTreeSet<PathName>);

impl PathNames {
    pub(crate) fn intern(&mut self, path: &Path) -> Arc<Path> {
        if let Some(path) = self.0.get(path.as_os_str()) {
            return Arc::clone(&path.0);
        }
        let path: Arc<Path> = Arc::from(path);
        self.0.insert(PathName(Arc::clone(&path)));
        path
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

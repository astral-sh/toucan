//! Immutable source files and checked byte spans.

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// An index into a [`SourceMap`]. IDs are local to their map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(usize);

/// A half-open byte range in one source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub file: FileId,
    pub start: usize,
    pub end: usize,
}

/// One-based line and byte column, suitable for C compiler diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    path: PathBuf,
    text: Arc<str>,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(path: PathBuf, text: impl Into<Arc<str>>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        line_starts.extend(
            text.bytes()
                .enumerate()
                .filter_map(|(i, b)| (b == b'\n').then_some(i + 1)),
        );
        Self {
            path,
            text,
            line_starts,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Resolve a byte offset, including the end of the file. Non-boundary UTF-8
    /// offsets are rejected so the same offset can safely be used to slice text.
    pub fn location(&self, offset: usize) -> Option<Location> {
        if !self.text.is_char_boundary(offset) {
            return None;
        }
        let line = self.line_starts.partition_point(|start| *start <= offset);
        Some(Location {
            line,
            column: offset - self.line_starts[line - 1] + 1,
        })
    }
}

#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    /// Appends source without reading `path` or merging entries with the same path.
    pub fn add(&mut self, path: impl Into<PathBuf>, text: impl Into<Arc<str>>) -> FileId {
        let id = FileId(self.files.len());
        self.files.push(SourceFile::new(path.into(), text));
        id
    }

    pub fn file(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.0)
    }

    /// Validates a half-open byte range against the stored file and UTF-8 boundaries.
    /// Empty ranges, including one at the end of the file, are valid.
    pub fn span(&self, file: FileId, start: usize, end: usize) -> Option<Span> {
        self.file(file)?.text.get(start..end)?;
        Some(Span { file, start, end })
    }

    pub fn slice(&self, span: Span) -> Option<&str> {
        self.file(span.file)?.text.get(span.start..span.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_locations_use_bytes_and_handle_crlf_and_eof() {
        let source = SourceFile::new("header.h".into(), "α\r\nx\n");
        assert_eq!(source.location(2), Some(Location { line: 1, column: 3 }));
        assert_eq!(source.location(4), Some(Location { line: 2, column: 1 }));
        assert_eq!(source.location(6), Some(Location { line: 3, column: 1 }));
        assert_eq!(source.location(1), None);
        assert_eq!(source.location(7), None);
    }

    #[test]
    fn spans_reject_reversed_out_of_bounds_and_split_utf8_ranges() {
        let mut map = SourceMap::default();
        let file = map.add("header.h", "αbc");
        assert_eq!(map.span(file, 3, 2), None);
        assert_eq!(map.span(file, 0, 8), None);
        assert_eq!(map.span(file, 0, 1), None);
        assert_eq!(map.slice(map.span(file, 0, 2).unwrap()), Some("α"));
    }
}

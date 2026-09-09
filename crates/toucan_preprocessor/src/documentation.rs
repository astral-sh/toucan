//! Bounded physical comment and token provenance, independent of C declaration policy.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use crate::token::Token;

const MAX_ROWS: usize = 1_000_000;

// Account for Vec's four-row first allocation, then conservative doubling.
fn row_cost<T>(len: usize) -> usize {
    std::mem::size_of::<T>() * if len == 0 { 4 } else { 2 }
}

/// Optional comment retention. Ordinary preprocessing leaves this disabled.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DocumentationOptions {
    /// Retain ordinary comments in addition to documentation markers.
    pub parse_all_comments: bool,
}

/// A physical source read in one documentation catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentationSourceId(u32);

/// Physical coordinates, before diagnostic line remapping or macro expansion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentationLocation {
    source: DocumentationSourceId,
    offset: u32,
    line: u32,
}
impl DocumentationLocation {
    pub fn source(self) -> DocumentationSourceId {
        self.source
    }
    pub fn offset(self) -> usize {
        self.offset as usize
    }
    pub fn line(self) -> usize {
        self.line as usize
    }
}

pub(crate) struct CommentLocation {
    pub range: Range<usize>,
    pub line: usize,
    pub end_line: usize,
    pub column: usize,
}

/// One comment or adjacent comment group, retaining its physical spelling.
#[derive(Clone, Debug)]
pub struct RawComment {
    range: Range<usize>,
    line: usize,
    end_line: usize,
    column: usize,
    trailing: bool,
    text: String,
    barrier: usize,
}
impl RawComment {
    pub fn range(&self) -> &Range<usize> {
        &self.range
    }
    pub fn line(&self) -> usize {
        self.line
    }
    pub fn end_line(&self) -> usize {
        self.end_line
    }
    pub fn is_trailing(&self) -> bool {
        self.trailing
    }
    /// Includes delimiters, physical line splices, and separators inside a group.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// First raw `;`, `{`, `}`, `#`, or `@` after this comment, or source length.
    pub fn following_barrier(&self) -> usize {
        self.barrier
    }
}

/// Comments from one physical read; repeated inclusions remain separate.
#[derive(Clone, Debug)]
pub struct DocumentationSource {
    path: Arc<Path>,
    accessed: Arc<Path>,
    len: usize,
    comments: Vec<RawComment>,
    systems: Vec<(u32, bool)>,
    initial_system: bool,
}
impl DocumentationSource {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn accessed_path(&self) -> &Path {
        &self.accessed
    }
    pub fn source_len(&self) -> usize {
        self.len
    }
    pub fn comments(&self) -> &[RawComment] {
        &self.comments
    }
    pub fn is_system_at(&self, offset: usize) -> Option<bool> {
        if offset > self.len {
            return None;
        }
        let i = self
            .systems
            .partition_point(|(start, _)| (*start as usize) <= offset);
        Some(if i == 0 {
            self.initial_system
        } else {
            self.systems[i - 1].1
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OriginId(NonZeroU32);

/// The macro invocation and physical replacement spelling of an output token.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DocumentationOrigin {
    invocation: Option<DocumentationLocation>,
    spelling: Option<DocumentationLocation>,
}
impl DocumentationOrigin {
    pub fn invocation(self) -> Option<DocumentationLocation> {
        self.invocation
    }
    pub fn spelling(self) -> Option<DocumentationLocation> {
        self.spelling
    }
}

/// An output token with comment provenance. Whitespace is not mapped.
#[derive(Clone, Debug)]
pub struct DocumentationMapping {
    generated: Range<usize>,
    origin: OriginId,
}
impl DocumentationMapping {
    pub fn generated(&self) -> &Range<usize> {
        &self.generated
    }
}

/// Optional, owned documentation provenance. IDs belong to this catalog.
#[derive(Clone, Debug, Default)]
pub struct Documentation {
    sources: Vec<DocumentationSource>,
    origins: Vec<DocumentationOrigin>,
    mappings: Vec<DocumentationMapping>,
    macros: BTreeMap<String, Vec<Option<OriginId>>>,
    used: usize,
}
impl Documentation {
    pub(crate) fn start(limit: usize) -> Result<Box<Self>, String> {
        let used = std::mem::size_of::<Self>();
        if used > limit {
            return Err("documentation byte limit exceeded".into());
        }
        Ok(Box::new(Self {
            used,
            ..Self::default()
        }))
    }
    pub fn sources(
        &self,
    ) -> impl ExactSizeIterator<Item = (DocumentationSourceId, &DocumentationSource)> {
        self.sources
            .iter()
            .enumerate()
            .map(|(i, source)| (DocumentationSourceId(i as u32), source))
    }
    pub fn source(&self, id: DocumentationSourceId) -> Option<&DocumentationSource> {
        self.sources.get(id.0 as usize)
    }
    pub fn mappings(&self) -> &[DocumentationMapping] {
        &self.mappings
    }
    pub fn origin(&self, mapping: &DocumentationMapping) -> Option<DocumentationOrigin> {
        self.origins
            .get(mapping.origin.0.get() as usize - 1)
            .copied()
    }
    pub fn resolve(&self, offset: usize) -> Option<DocumentationOrigin> {
        let i = self
            .mappings
            .partition_point(|mapping| mapping.generated.end <= offset);
        let mapping = self.mappings.get(i)?;
        mapping
            .generated
            .contains(&offset)
            .then(|| self.origins[mapping.origin.0.get() as usize - 1])
    }

    fn charge(&mut self, bytes: usize, limit: usize) -> Result<(), String> {
        self.used = self
            .used
            .checked_add(bytes)
            .ok_or("documentation byte count overflow")?;
        if self.used > limit {
            return Err("documentation byte limit exceeded".into());
        }
        Ok(())
    }
    pub(crate) fn add_source(
        &mut self,
        path: &Path,
        accessed: &Path,
        len: usize,
        system: bool,
        limit: usize,
    ) -> Result<DocumentationSourceId, String> {
        if self.sources.len() >= MAX_ROWS || len > u32::MAX as usize {
            return Err("documentation source count or size limit exceeded".into());
        }
        self.charge(
            row_cost::<DocumentationSource>(self.sources.len())
                + path
                    .as_os_str()
                    .len()
                    .saturating_add(accessed.as_os_str().len())
                    .saturating_mul(2)
                + 64,
            limit,
        )?;
        let id = DocumentationSourceId(self.sources.len() as u32);
        self.sources.push(DocumentationSource {
            path: Arc::from(path),
            accessed: Arc::from(accessed),
            len,
            comments: Vec::new(),
            systems: Vec::new(),
            initial_system: system,
        });
        Ok(id)
    }
    pub(crate) fn comment(
        &mut self,
        id: DocumentationSourceId,
        raw: &str,
        location: CommentLocation,
        options: DocumentationOptions,
        limit: usize,
    ) -> Result<(), String> {
        let CommentLocation {
            range,
            line,
            end_line,
            column,
        } = location;
        let ordinary = !marked(&raw[range.clone()]);
        let inferred_trailing = options.parse_all_comments
            && ordinary
            && raw.as_bytes()[..range.start]
                .iter()
                .rev()
                .take_while(|b| !matches!(b, b'\r' | b'\n'))
                .any(|b| !b.is_ascii_whitespace());
        let trailing = inferred_trailing
            || raw[range.start..].starts_with("///<")
            || raw[range.start..].starts_with("//!<")
            || raw[range.start..].starts_with("/**<")
            || raw[range.start..].starts_with("/*!<");
        let previous = self.sources[id.0 as usize].comments.last();
        let merge = previous
            .filter(|c| {
                c.trailing == trailing || c.trailing && !trailing && ordinary && c.column == column
            })
            .filter(|c| adjacent(&raw[c.range.end..range.start]))
            .map(|c| c.range.end);
        if let Some(end) = merge {
            self.charge((range.end - end).saturating_mul(2), limit)?;
            let last = self.sources[id.0 as usize]
                .comments
                .last_mut()
                .expect("merge comment");
            last.text.push_str(&raw[end..range.end]);
            last.range.end = range.end;
            last.end_line = end_line;
        } else {
            if self.sources[id.0 as usize].comments.len() >= MAX_ROWS {
                return Err("documentation comment count limit exceeded".into());
            }
            self.charge(
                row_cost::<RawComment>(self.sources[id.0 as usize].comments.len())
                    + range.len() * 2,
                limit,
            )?;
            self.sources[id.0 as usize].comments.push(RawComment {
                text: raw[range.clone()].to_owned(),
                range,
                line,
                end_line,
                column,
                trailing,
                barrier: raw.len(),
            });
        }
        Ok(())
    }
    pub(crate) fn finish_source(&mut self, id: DocumentationSourceId, raw: &str) {
        let mut barrier = 0;
        for comment in &mut self.sources[id.0 as usize].comments {
            if barrier < comment.range.end {
                barrier = raw.as_bytes()[comment.range.end..]
                    .iter()
                    .position(|b| b";{}#@".contains(b))
                    .map_or(raw.len(), |i| comment.range.end + i);
            }
            comment.barrier = barrier;
        }
    }
    pub(crate) fn location(
        &mut self,
        source: DocumentationSourceId,
        offset: usize,
        line: usize,
        limit: usize,
    ) -> Result<OriginId, String> {
        let location = DocumentationLocation {
            source,
            offset: u32::try_from(offset).map_err(|_| "documentation offset limit exceeded")?,
            line: u32::try_from(line).map_err(|_| "documentation line limit exceeded")?,
        };
        self.push_origin(
            DocumentationOrigin {
                invocation: Some(location),
                spelling: None,
            },
            limit,
        )
    }
    fn push_origin(
        &mut self,
        origin: DocumentationOrigin,
        limit: usize,
    ) -> Result<OriginId, String> {
        if self.origins.len() >= MAX_ROWS {
            return Err("documentation origin count limit exceeded".into());
        }
        self.charge(row_cost::<DocumentationOrigin>(self.origins.len()), limit)?;
        self.origins.push(origin);
        Ok(OriginId(
            NonZeroU32::new(self.origins.len() as u32).expect("one-based origin"),
        ))
    }
    pub(crate) fn expanded(
        &mut self,
        invocation: Option<OriginId>,
        spelling: Option<OriginId>,
        limit: usize,
    ) -> Result<Option<OriginId>, String> {
        if invocation.is_none() && spelling.is_none() {
            return Ok(None);
        }
        let origin = |id: OriginId| self.origins[id.0.get() as usize - 1];
        let call = invocation.and_then(|id| origin(id).invocation);
        let spelling = spelling.and_then(|id| {
            let o = origin(id);
            o.spelling.or(o.invocation)
        });
        self.push_origin(
            DocumentationOrigin {
                invocation: call,
                spelling,
            },
            limit,
        )
        .map(Some)
    }
    pub(crate) fn map(
        &mut self,
        generated: Range<usize>,
        origin: Option<OriginId>,
        limit: usize,
    ) -> Result<(), String> {
        if let Some(origin) = origin {
            if self.mappings.len() >= MAX_ROWS {
                return Err("documentation mapping count limit exceeded".into());
            }
            self.charge(row_cost::<DocumentationMapping>(self.mappings.len()), limit)?;
            self.mappings
                .push(DocumentationMapping { generated, origin });
        }
        Ok(())
    }
    pub(crate) fn define(
        &mut self,
        name: &str,
        tokens: &[Token],
        limit: usize,
    ) -> Result<(), String> {
        self.macros.remove(name);
        if tokens.iter().any(|token| token.doc_origin.is_some()) {
            self.charge(
                name.len() * 2 + tokens.len() * std::mem::size_of::<Option<OriginId>>() * 2 + 1024,
                limit,
            )?;
            self.macros.insert(
                name.to_owned(),
                tokens.iter().map(|token| token.doc_origin).collect(),
            );
        }
        Ok(())
    }
    pub(crate) fn undefine(&mut self, name: &str) {
        self.macros.remove(name);
    }
    pub(crate) fn replacements(&self, name: &str, tokens: &mut [Token]) {
        if let Some(origins) = self.macros.get(name) {
            assert_eq!(origins.len(), tokens.len(), "macro spelling token count");
            for (token, origin) in tokens.iter_mut().zip(origins) {
                token.doc_origin = *origin;
            }
        }
    }
    pub(crate) fn system_origin(
        &mut self,
        id: OriginId,
        value: bool,
        limit: usize,
    ) -> Result<(), String> {
        if let Some(location) = self.origins[id.0.get() as usize - 1].invocation {
            self.system(location.source, location.offset(), value, limit)?;
        }
        Ok(())
    }
    fn system(
        &mut self,
        id: DocumentationSourceId,
        offset: usize,
        value: bool,
        limit: usize,
    ) -> Result<(), String> {
        let source = &self.sources[id.0 as usize];
        if source
            .systems
            .last()
            .map_or(source.initial_system, |entry| entry.1)
            == value
        {
            return Ok(());
        }
        if source.systems.len() >= MAX_ROWS {
            return Err("documentation region count limit exceeded".into());
        }
        if source
            .systems
            .last()
            .is_some_and(|entry| entry.0 as usize > offset)
        {
            return Err("documentation region order is not representable".into());
        }
        self.charge(row_cost::<(u32, bool)>(source.systems.len()), limit)?;
        self.sources[id.0 as usize].systems.push((
            u32::try_from(offset).map_err(|_| "documentation offset limit exceeded")?,
            value,
        ));
        Ok(())
    }
    pub(crate) fn finish(&mut self) {
        self.macros.clear();
    }
}

fn marked(raw: &str) -> bool {
    raw.starts_with("/**")
        || raw.starts_with("/*!")
        || raw.starts_with("///")
        || raw.starts_with("//!")
}

pub(crate) fn recognized(raw: &str, options: DocumentationOptions) -> bool {
    let valid = raw.starts_with("//") || raw.starts_with("/*") && raw.ends_with("*/");
    valid && (options.parse_all_comments || marked(raw))
}

fn adjacent(raw: &str) -> bool {
    let mut newlines = 0;
    let mut previous = None;
    for byte in raw.bytes() {
        if !byte.is_ascii_whitespace() {
            return false;
        }
        if matches!(byte, b'\n' | b'\r') {
            if previous.is_some_and(|before| matches!(before, b'\n' | b'\r') && before != byte) {
                previous = None;
                continue;
            }
            newlines += 1;
            if newlines > 1 {
                return false;
            }
        }
        previous = Some(byte);
    }
    true
}

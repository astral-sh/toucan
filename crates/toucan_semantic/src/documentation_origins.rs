//! Optional declaration locations needed to attach physical documentation comments.

use serde::Serialize;

use crate::{Error, parser_extensions::SourceMap};

/// A documentable item in the same [`crate::Analysis`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[non_exhaustive]
pub enum DocumentationTarget {
    /// Index in [`crate::TranslationUnit::declarations`].
    Declaration(usize),
    /// Index in [`crate::TranslationUnit::records`].
    Record(usize),
    /// Index in [`crate::TranslationUnit::enums`].
    Enum(usize),
    /// An enumeration and the index of one of its variants.
    Enumerator { enumeration: usize, variant: usize },
    /// A record and the index of one of its fields.
    Field { record: usize, field: usize },
}

/// One written declaration, retaining token starts independently of diagnostics.
#[derive(Debug, Serialize)]
pub struct DocumentationDeclaration {
    target: DocumentationTarget,
    begin: usize,
    name: usize,
    parent_name: Option<usize>,
    reference: bool,
}
impl DocumentationDeclaration {
    /// The declaration or member contributed to by this occurrence.
    pub fn target(&self) -> DocumentationTarget {
        self.target
    }
    /// First token of this declaration in the original preprocessed input.
    pub fn begin(&self) -> usize {
        self.begin
    }
    /// Identifier token, or the first specifier for an unnamed declaration.
    pub fn name(&self) -> usize {
        self.name
    }
    /// Identifier of the immediately containing record or enumeration.
    pub fn parent_name(&self) -> Option<usize> {
        self.parent_name
    }
    /// A tag reference that does not carry its own documentation comment.
    pub fn is_reference(&self) -> bool {
        self.reference
    }
}

/// Source-ordered declarations and members retained for documentation attachment.
/// Coordinates refer to token starts, not contiguous source fragments.
#[derive(Debug, Serialize)]
pub struct DocumentationDeclarations {
    entries: Vec<DocumentationDeclaration>,
}
impl DocumentationDeclarations {
    /// Written occurrences, ordered by identifier position in preprocessed input.
    pub fn entries(&self) -> &[DocumentationDeclaration] {
        &self.entries
    }
}

pub(crate) struct Builder {
    entries: Vec<DocumentationDeclaration>,
    pub(crate) parent_name: Option<usize>,
}
impl Builder {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            parent_name: None,
        }
    }
    pub(crate) fn push(
        &mut self,
        target: DocumentationTarget,
        begin: usize,
        name: usize,
        reference: bool,
    ) -> Result<(), Error> {
        if self.entries.len() == 1_000_000 {
            return Err(Error::new(name, "documentation declaration limit exceeded"));
        }
        self.entries.push(DocumentationDeclaration {
            target,
            begin,
            name,
            parent_name: self.parent_name,
            reference,
        });
        Ok(())
    }
    pub(crate) fn standalone_tag(&mut self, name: usize) {
        if let Some(entry) = self.entries.last_mut()
            && entry.name == name
        {
            entry.reference = false;
        }
    }
    pub(crate) fn finish(
        mut self,
        offsets: &SourceMap,
        source_len: usize,
    ) -> Result<DocumentationDeclarations, Error> {
        for entry in &mut self.entries {
            let parsed_name = entry.name;
            entry.begin = offsets.original_offset(entry.begin);
            entry.name = offsets.original_offset(entry.name);
            entry.parent_name = entry
                .parent_name
                .map(|offset| offsets.original_offset(offset));
            if entry.begin >= source_len
                || entry.name >= source_len
                || entry.parent_name.is_some_and(|offset| offset >= source_len)
            {
                return Err(Error::new(
                    parsed_name,
                    "documentation declaration offset is outside the source",
                ));
            }
        }
        self.entries.sort_by_key(|entry| (entry.name, entry.begin));
        Ok(DocumentationDeclarations {
            entries: self.entries,
        })
    }
}

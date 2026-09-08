//! Written declaration locations without retaining expression or statement arenas.

use lang_c::span::Span;
use serde::Serialize;

use crate::Error;
use crate::checked::SourceSpan;
use crate::parser_extensions::SourceMap;

const MAX_ORIGINS: usize = 1_000_000;
const MAX_FRAGMENT_BYTES: usize = 64 * 1024 * 1024;

/// A declaration or type owned by the same [`crate::Analysis`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum DeclarationTarget {
    /// Index in [`crate::TranslationUnit::declarations`].
    Declaration(usize),
    /// Index in [`crate::TranslationUnit::records`].
    Record(usize),
    /// Index in [`crate::TranslationUnit::enums`].
    Enum(usize),
    /// A variant of the indexed enumeration.
    Enumerator { enumeration: usize, variant: usize },
}

/// One written file-scope declaration; compatible redeclarations remain separate.
#[derive(Debug, Serialize)]
pub struct DeclarationOrigin {
    target: DeclarationTarget,
    source: SourceSpan,
    definition: bool,
    external: bool,
    reference: bool,
    inline: bool,
}

impl DeclarationOrigin {
    /// Canonical declaration or type contributed to by this occurrence.
    pub fn target(&self) -> DeclarationTarget {
        self.target
    }

    /// Identifier location in the original preprocessed input, or the unnamed tag.
    pub fn source(&self) -> &SourceSpan {
        &self.source
    }

    /// Whether this occurrence only names an existing tag, without declaring it.
    /// Such uses are not independent header allowlist roots.
    pub fn is_reference(&self) -> bool {
        self.reference
    }

    /// Inline fact at this occurrence: GNU records the written specifier, while
    /// Clang also inherits earlier inline declarations. A later inline definition
    /// remains a separate occurrence of the same target.
    pub fn is_inline(&self) -> bool {
        self.inline
    }

    /// Whether this occurrence supplies a definition.
    pub fn is_definition(&self) -> bool {
        self.definition
    }

    /// Whether this occurrence declares an externally linked function or object.
    pub fn is_external(&self) -> bool {
        self.external
    }
}

/// Source-ordered declarations captured independently of checked-code retention.
///
/// File-scope tag references are included alongside tag declarations. Their target
/// identifies the canonical type, including a definition supplied later in the input.
/// This catalog is capped at one million occurrences and 64 MiB of source fragments.
#[derive(Debug, Serialize)]
pub struct DeclarationOrigins {
    entries: Vec<DeclarationOrigin>,
}

impl DeclarationOrigins {
    /// Written occurrences in preprocessed source order.
    pub fn entries(&self) -> &[DeclarationOrigin] {
        &self.entries
    }
}

pub(crate) struct Builder {
    entries: Vec<(DeclarationTarget, Span, bool, bool, bool, bool)>,
}

impl Builder {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub(crate) fn push(
        &mut self,
        target: DeclarationTarget,
        source: Span,
        definition: bool,
        external: bool,
        reference: bool,
        inline: bool,
    ) -> Result<(), Error> {
        if self.entries.len() == MAX_ORIGINS {
            return Err(Error::new(
                source.start,
                "declaration-origin occurrence limit exceeded",
            ));
        }
        self.entries
            .push((target, source, definition, external, reference, inline));
        Ok(())
    }

    /// Standalone `struct S;`/`enum E;` declarations redeclare visible tags.
    pub(crate) fn standalone_tag(&mut self, source: Span) {
        if let Some((_, span, _, _, reference, _)) = self.entries.last_mut()
            && span.start == source.start
        {
            *reference = false;
        }
    }

    pub(crate) fn finish(self, offsets: &SourceMap) -> Result<DeclarationOrigins, Error> {
        let mut fragment_bytes = 0usize;
        let mut entries = self
            .entries
            .into_iter()
            .map(|(target, span, definition, external, reference, inline)| {
                let source = crate::checked::map_source_span(offsets, span, |_, bytes| {
                    fragment_bytes = fragment_bytes
                        .checked_add(bytes)
                        .filter(|total| *total <= MAX_FRAGMENT_BYTES)
                        .ok_or_else(|| {
                            Error::new(
                                span.start,
                                "declaration-origin source fragment limit exceeded",
                            )
                        })?;
                    Ok(())
                })?;
                Ok(DeclarationOrigin {
                    target,
                    source,
                    definition,
                    external,
                    reference,
                    inline,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        entries.sort_by_key(|entry| (entry.source.range().start, entry.source.range().end));
        Ok(DeclarationOrigins { entries })
    }
}

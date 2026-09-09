use std::fmt;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

/// The source anchor represented by a generated range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginKind {
    /// The start of an original preprocessing token.
    Token,
    /// The start of the outer macro invocation that produced the token.
    MacroInvocation,
    /// The start of a preserved preprocessing directive.
    Directive,
}

/// A source line and one-based byte column, including line directive remapping.
/// Lines are one-based except that GNU line markers can explicitly select zero.
///
/// Macro locations identify invocation sites, not definition sites or an
/// expansion stack. A location is an anchor; it does not describe every byte of
/// a token that spans escaped newlines.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLocation {
    /// Shared source name; filesystem headers use canonical paths.
    pub path: Arc<Path>,
    pub line: usize,
    pub column: usize,
    pub kind: OriginKind,
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}",
            self.path.display(),
            self.line,
            self.column
        )
    }
}

/// A half-open byte range in preprocessed source and its source anchor.
///
/// Ranges include the separator following a token. Preserved directives occupy
/// one range. Ranges are ordered and disjoint in an unmodified result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMapping {
    pub generated: Range<usize>,
    pub origin: SourceLocation,
}

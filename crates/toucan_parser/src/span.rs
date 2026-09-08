//! Source text location tracking
use std::{cmp, fmt};

/// Byte offset of a node start and end positions in the input stream
#[derive(Copy, Clone)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// Create a new span for a specific location
    // Preserve the upstream constructor used by the generated grammar.
    #[allow(clippy::self_named_constructors)]
    pub fn span(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    /// Create a new undefined span that is equal to any other span
    pub fn none() -> Span {
        Span {
            start: usize::MAX,
            end: usize::MAX,
        }
    }

    /// Test if span is undefined
    pub fn is_none(&self) -> bool {
        self.start == usize::MAX && self.end == usize::MAX
    }
}

impl cmp::PartialEq for Span {
    fn eq(&self, other: &Self) -> bool {
        (self.start == other.start && self.end == other.end) || self.is_none() || other.is_none()
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        if !self.is_none() {
            write!(fmt, "{}…{}", self.start, self.end)
        } else {
            write!(fmt, "…")
        }
    }
}

/// Associate a span with an arbitrary type
#[derive(Debug, PartialEq, Clone)]
pub struct Node<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Node<T> {
    /// Create new node
    pub fn new(node: T, span: Span) -> Node<T> {
        Node { node, span }
    }
}

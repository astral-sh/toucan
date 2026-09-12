//! Low-level syntax storage for compiler passes that consume or construct nodes.
//!
//! [`driver::Parse::into_raw`](::driver::Parse::into_raw) explicitly leaves the
//! owner-bound read API. Raw roots and arenas can be separated; the caller must
//! keep their association. IDs are local table indices, and raw lookup checks
//! bounds but cannot distinguish another arena's in-range index.
//!
//! Raw node cloning copies IDs rather than descendants. Clone a complete raw
//! parse to copy all storage, or use [`AstRef::to_owned`](::view::AstRef::to_owned)
//! before entering this API to copy a reachable subtree. There is deliberately
//! no public conversion from raw parts back to an owner-bound view.

use arena::Arena;
use ast::{Expression, TranslationUnit};
use limits::ParseStatistics;
use span::Node;

/// A translation unit whose root and arena are managed by the caller.
#[derive(Clone, Debug)]
pub struct Parse {
    /// Preprocessed source text.
    pub source: String,
    /// Low-level syntax root; its IDs refer to `arena`.
    pub unit: TranslationUnit,
    /// Resource counters for parsing.
    pub statistics: ParseStatistics,
    /// Storage associated with the root.
    pub arena: Arena,
}

/// An expression whose root and arena are managed by the caller.
#[derive(Clone, Debug)]
pub struct ExpressionParse {
    /// Preprocessed source text.
    pub source: String,
    /// Low-level expression root; its IDs refer to `arena`.
    pub expression: Node<Expression>,
    /// Resource counters for parsing.
    pub statistics: ParseStatistics,
    /// Storage associated with the root.
    pub arena: Arena,
}

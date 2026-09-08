//! C declarations, target-specific types, expressions, and function-body constraints.
//!
//! This crate checks preprocessed C without invoking an external compiler. Unsupported
//! constructs return diagnostics; function bodies are checked even when a binding
//! consumer omits their definitions from its generated API.

mod analyze;
mod expression;
mod initializer;
mod integer;
mod ir;
mod statement;

pub use analyze::{analyze, evaluate_integer};
pub use ir::*;

/// A source-positioned syntax, semantic, or unsupported-feature diagnostic.
#[derive(Clone, Debug, thiserror::Error, serde::Serialize)]
#[error("{message} at byte {offset}")]
pub struct Error {
    pub message: String,
    pub offset: usize,
}

impl Error {
    pub(crate) fn new(offset: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset,
        }
    }
}

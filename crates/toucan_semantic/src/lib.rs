//! C declarations, target-specific types, and integer constant expressions.
//!
//! This crate accepts preprocessed C without invoking an external compiler. Function
//! bodies are parsed, but are not type-checked; the semantic API describes header
//! declarations rather than claiming to validate executable C programs.

mod analyze;
mod expression;
mod initializer;
mod integer;
mod ir;

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

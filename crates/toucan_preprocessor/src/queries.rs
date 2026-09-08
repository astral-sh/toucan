//! Compiler queries are mutable predefined operators, not replacement macros.

use std::fmt::Debug;
use std::sync::Arc;

use crate::token::{Kind, Token};

/// The compiler's macro-argument rules for feature queries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryDialect {
    Gnu,
    Clang,
}

/// A predefined feature-query operator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeatureQuery {
    Builtin,
    Attribute,
}

impl FeatureQuery {
    pub(crate) const ALL: [Self; 2] = [Self::Builtin, Self::Attribute];

    pub fn name(self) -> &'static str {
        match self {
            Self::Builtin => "__has_builtin",
            Self::Attribute => "__has_attribute",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "__has_builtin" => Some(Self::Builtin),
            "__has_attribute" => Some(Self::Attribute),
            _ => None,
        }
    }

    pub(crate) fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// An immutable catalog supplied by the embedding frontend.
///
/// Return zero for unsupported names. Values can carry standard-version dates,
/// so the return type is numeric. The preprocessor validates the argument before
/// invoking the provider. Attribute names and namespaces retain their spelling;
/// the provider decides which aliases it supports. A provider must be bounded,
/// deterministic, and independent of mutable host state.
pub trait FeatureQueryProvider: Debug + Send + Sync {
    fn query(&self, kind: FeatureQuery, namespace: Option<&str>, name: &str) -> u64;
}

/// Enabled queries and their compiler-specific argument rules.
///
/// Source `#undef` and `#define` directives change the current translation unit's
/// active operators. Configuration changes apply to subsequent entry points.
#[derive(Clone, Debug)]
pub struct FeatureQueries {
    pub dialect: QueryDialect,
    pub provider: Arc<dyn FeatureQueryProvider>,
    pub(crate) enabled: u8,
}

impl FeatureQueries {
    /// Enable both supported operators with the supplied immutable catalog.
    pub fn new(dialect: QueryDialect, provider: Arc<dyn FeatureQueryProvider>) -> Self {
        Self {
            dialect,
            provider,
            enabled: FeatureQuery::ALL
                .iter()
                .fold(0, |bits, kind| bits | kind.bit()),
        }
    }

    /// Disable a predefined operator, as for a command-line `-U` option.
    pub fn disable(&mut self, kind: FeatureQuery) {
        self.enabled &= !kind.bit();
    }

    pub fn is_enabled(&self, kind: FeatureQuery) -> bool {
        self.enabled & kind.bit() != 0
    }

    pub(crate) fn expands_argument(&self, kind: FeatureQuery) -> bool {
        kind == FeatureQuery::Attribute || self.dialect == QueryDialect::Gnu
    }

    pub(crate) fn evaluate(&self, kind: FeatureQuery, tokens: &[Token]) -> Result<u64, String> {
        let (namespace, name) = match tokens {
            [name] if name.kind == Kind::Identifier => (None, name.text.as_str()),
            [namespace, first, second, name]
                if kind == FeatureQuery::Attribute
                    && self.dialect == QueryDialect::Gnu
                    && namespace.kind == Kind::Identifier
                    && first.text == ":"
                    && second.text == ":"
                    && name.kind == Kind::Identifier =>
            {
                (Some(namespace.text.as_str()), name.text.as_str())
            }
            _ => {
                return Err(format!(
                    "{} requires {}",
                    kind.name(),
                    if kind == FeatureQuery::Attribute && self.dialect == QueryDialect::Gnu {
                        "an identifier or namespace::identifier"
                    } else {
                        "exactly one identifier"
                    }
                ));
            }
        };
        Ok(self.provider.query(kind, namespace, name))
    }
}

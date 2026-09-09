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
    Feature,
    Extension,
    CAttribute,
    DeclspecAttribute,
    BuildingModule,
}

impl FeatureQuery {
    pub(crate) const ALL: [Self; 7] = [
        Self::Builtin,
        Self::Attribute,
        Self::Feature,
        Self::Extension,
        Self::CAttribute,
        Self::DeclspecAttribute,
        Self::BuildingModule,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Builtin => "__has_builtin",
            Self::Attribute => "__has_attribute",
            Self::Feature => "__has_feature",
            Self::Extension => "__has_extension",
            Self::CAttribute => "__has_c_attribute",
            Self::DeclspecAttribute => "__has_declspec_attribute",
            Self::BuildingModule => "__building_module",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "__has_builtin" => Some(Self::Builtin),
            "__has_attribute" => Some(Self::Attribute),
            "__has_feature" => Some(Self::Feature),
            "__has_extension" => Some(Self::Extension),
            "__has_c_attribute" => Some(Self::CAttribute),
            "__has_declspec_attribute" => Some(Self::DeclspecAttribute),
            "__building_module" => Some(Self::BuildingModule),
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
    /// Enables the dialect's predefined operators with an immutable catalog.
    /// GNU has builtin, GNU-attribute and C-attribute queries; Clang has all seven.
    pub fn new(dialect: QueryDialect, provider: Arc<dyn FeatureQueryProvider>) -> Self {
        Self {
            dialect,
            provider,
            enabled: FeatureQuery::ALL
                .iter()
                .filter(|kind| {
                    dialect == QueryDialect::Clang
                        || matches!(
                            kind,
                            FeatureQuery::Builtin
                                | FeatureQuery::Attribute
                                | FeatureQuery::CAttribute
                        )
                })
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
        matches!(
            kind,
            FeatureQuery::Attribute | FeatureQuery::CAttribute | FeatureQuery::DeclspecAttribute
        ) || kind == FeatureQuery::Builtin && self.dialect == QueryDialect::Gnu
    }

    /// Namespace separators are read without expanding that token.
    pub(crate) fn permits_namespace(&self, kind: FeatureQuery) -> bool {
        kind == FeatureQuery::CAttribute
            || kind == FeatureQuery::Attribute && self.dialect == QueryDialect::Gnu
    }

    /// Clang's shared feature-query evaluator uses a long literal for revision dates.
    pub(crate) fn spelling(&self, value: u64) -> String {
        if self.dialect == QueryDialect::Clang && value > 1 {
            format!("{value}L")
        } else {
            value.to_string()
        }
    }

    pub(crate) fn evaluate(&self, kind: FeatureQuery, tokens: &[Token]) -> Result<u64, String> {
        let (namespace, name) = match tokens {
            [name] if name.kind == Kind::Identifier => (None, name.text.as_str()),
            [namespace, scope, name]
                if self.permits_namespace(kind)
                    && namespace.kind == Kind::Identifier
                    && scope.text == "::"
                    && name.kind == Kind::Identifier =>
            {
                (Some(namespace.text.as_str()), name.text.as_str())
            }
            [namespace, colon, second, name]
                if self.permits_namespace(kind)
                    && namespace.kind == Kind::Identifier
                    && colon.colon_scope
                    && second.text == ":"
                    && name.kind == Kind::Identifier =>
            {
                (Some(namespace.text.as_str()), name.text.as_str())
            }
            _ => {
                return Err(format!(
                    "{} requires {}",
                    kind.name(),
                    if self.permits_namespace(kind) {
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

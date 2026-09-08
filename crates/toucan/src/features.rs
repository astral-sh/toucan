//! The facade connects preprocessing queries to semantic name classifiers.
use std::sync::Arc;

use toucan_preprocessor::{FeatureQueries, FeatureQuery, FeatureQueryProvider, QueryDialect};
use toucan_target::{Compiler, CompilerProfile};

#[derive(Debug)]
struct Catalog(CompilerProfile);

impl FeatureQueryProvider for Catalog {
    fn query(&self, kind: FeatureQuery, namespace: Option<&str>, name: &str) -> u64 {
        if namespace.is_some_and(|namespace| {
            self.0.compiler() != Compiler::Gnu
                || !self.0.language_mode().is_gnu()
                || !matches!(namespace, "gnu" | "__gnu__")
        }) {
            return 0;
        }
        match kind {
            FeatureQuery::Builtin => u64::from(toucan_semantic::has_builtin(self.0, name)),
            FeatureQuery::Attribute => toucan_semantic::has_attribute(self.0, name),
            FeatureQuery::Feature | FeatureQuery::Extension => {
                let name = name
                    .strip_prefix("__")
                    .and_then(|name| name.strip_suffix("__"))
                    .unwrap_or(name);
                u64::from(
                    (self.0.language_mode().is_c11() || kind == FeatureQuery::Extension)
                        && matches!(
                            name,
                            "c_alignas"
                                | "c_alignof"
                                | "c_atomic"
                                | "c_generic_selections"
                                | "c_static_assert"
                                | "c_thread_local"
                        ),
                )
            }
            FeatureQuery::DeclspecAttribute => {
                toucan_semantic::has_declspec_attribute(self.0, name)
            }
            // C23 attributes need their own syntax and declaration constraints;
            // module queries have no active module in this frontend.
            FeatureQuery::CAttribute | FeatureQuery::BuildingModule => 0,
        }
    }
}

pub(crate) fn queries(profile: CompilerProfile) -> FeatureQueries {
    FeatureQueries::new(
        match profile.compiler() {
            Compiler::Gnu => QueryDialect::Gnu,
            Compiler::Clang => QueryDialect::Clang,
        },
        Arc::new(Catalog(profile)),
    )
}

//! Scoped policies for speculative evaluation and frontend checking.
//!
//! These flags describe the current operation, not facts learned about the input.
//! Nested operations inherit the caller's policy and restore it on return,
//! including when a check reports an error. Caches and feature-use records live
//! outside this context so restoring a policy never rolls back semantic facts.

use crate::analyze::Analyzer;

#[derive(Clone, Copy, Default)]
pub(crate) struct Context {
    const_object_reads: bool,
    late_object_size_folds: bool,
    suppress_target_features: bool,
    enum_expression: bool,
}

impl Context {
    /// Public folding queries may use facts unavailable to frontend C checks.
    pub(crate) fn constant_query() -> Self {
        Self {
            late_object_size_folds: true,
            ..Self::default()
        }
    }

    pub(crate) fn allows_const_object_reads(self) -> bool {
        self.const_object_reads
    }

    pub(crate) fn allows_late_object_size_folds(self) -> bool {
        self.late_object_size_folds
    }

    pub(crate) fn suppresses_target_features(self) -> bool {
        self.suppress_target_features
    }

    pub(crate) fn is_enum_expression(self) -> bool {
        self.enum_expression
    }
}

impl Analyzer {
    fn with_evaluation<T>(
        &mut self,
        enter: impl FnOnce(&mut Context),
        evaluate: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous = self.evaluation;
        enter(&mut self.evaluation);
        let result = evaluate(self);
        self.evaluation = previous;
        result
    }

    /// Permit completed const scalar values without making them ordinary ICEs.
    pub(crate) fn with_const_object_reads<T>(
        &mut self,
        evaluate: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.with_evaluation(|context| context.const_object_reads = true, evaluate)
    }

    /// Type constraints must not depend on later code-generation object sizes.
    pub(crate) fn with_frontend_folding<T>(&mut self, evaluate: impl FnOnce(&mut Self) -> T) -> T {
        self.with_evaluation(|context| context.late_object_size_folds = false, evaluate)
    }

    /// Rechecking an expression must not introduce new target-feature obligations.
    pub(crate) fn without_target_feature_uses<T>(
        &mut self,
        evaluate: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.with_evaluation(|context| context.suppress_target_features = true, evaluate)
    }

    /// Tags defined in an enumerator expression have compiler-specific visibility.
    pub(crate) fn within_enum_expression<T>(&mut self, evaluate: impl FnOnce(&mut Self) -> T) -> T {
        self.with_evaluation(|context| context.enum_expression = true, evaluate)
    }
}

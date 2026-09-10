//! Completed file-scope scalars for Clang initializers and constant queries.
//!
//! These values are not C integer constant expressions and are never installed
//! in the translation unit's enumerator table or retained as expression trees.

use std::collections::BTreeMap;

use lang_c::{ast, span::Node};
use toucan_target::Compiler;

use crate::{Error, SymbolBinding, TypeKind, analyze::Analyzer, floating::ArithmeticValue};

const MAX_OBJECTS: usize = 100_000;
const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct Values {
    entries: BTreeMap<String, ArithmeticValue>,
    bytes: usize,
}

impl Values {
    /// Charge owned names and a conservative map-entry allowance before cloning.
    fn insert(&mut self, name: &str, value: ArithmeticValue, offset: usize) -> Result<(), Error> {
        let bytes = self
            .bytes
            .saturating_add(name.len())
            .saturating_add(2 * std::mem::size_of::<(String, ArithmeticValue)>());
        if self.entries.len() >= MAX_OBJECTS || bytes > MAX_BYTES {
            return Err(Error::new(
                offset,
                "const-object initializer value limit exceeded",
            ));
        }
        self.bytes = bytes;
        self.entries.insert(name.to_owned(), value);
        Ok(())
    }
}

impl Analyzer {
    /// Snapshot eligibility only after an initialized definition has completed.
    /// In Clang, weak attributes present at that definition prevent folding;
    /// a weak attribute on a later redeclaration does not invalidate the value.
    pub(crate) fn note_const_object(
        &mut self,
        declaration: usize,
        initializer: &Node<ast::Initializer>,
    ) -> Result<(), Error> {
        if self.unit.compiler != Compiler::Clang {
            return Ok(());
        }
        let object = &self.unit.declarations[declaration];
        let qualifiers = self.unit.qualifiers(&object.ty)?;
        if !qualifiers.is_const
            || qualifiers.is_volatile
            || object.symbol_binding == SymbolBinding::Weak
            || self.dll_imported_object(&object.name)
        {
            return Ok(());
        }
        let destination = self.unit.atomic_value(&object.ty)?.unwrap_or(&object.ty);
        if !matches!(
            self.unit.resolve(destination)?.kind,
            TypeKind::Integer(_) | TypeKind::Bool | TypeKind::Enum(_) | TypeKind::Float(_)
        ) {
            return Ok(());
        }
        let ty = object.ty.clone();
        // The declaration has already passed ordinary initializer checking. An
        // admitted initializer outside this scalar evaluator's coverage must
        // remain accepted; it simply cannot seed later const-object reads.
        let Ok(Some(value)) = self.scalar_initializer_value(&ty, initializer, true) else {
            return Ok(());
        };
        let name = &self.unit.declarations[declaration].name;
        self.const_objects.get_or_insert_with(Box::default).insert(
            name,
            value,
            initializer.span.start,
        )
    }

    /// Resolve a completed file value only in a static initializer or constant query.
    /// A local object, parameter, or enumerator can hide the same file spelling.
    pub(crate) fn const_object_value(&self, name: &str) -> Option<ArithmeticValue> {
        if !self.evaluation.allows_const_object_reads()
            || self.unit.constants.contains_key(name)
            || self
                .lexical_scopes
                .iter()
                .rev()
                .find(|scope| scope.names.contains_key(name))
                .is_some_and(|scope| !scope.linked.contains(name))
        {
            return None;
        }
        self.const_objects.as_ref()?.entries.get(name).copied()
    }

    /// Keep object reads local to arithmetic initializer evaluation, including
    /// nested constant queries, and restore the caller's context on errors.
    pub(crate) fn eval_initializer_arithmetic(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ArithmeticValue, Error> {
        self.with_const_object_reads(|analyzer| analyzer.eval_arithmetic(expression))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lang_c::span::Span;

    #[test]
    fn value_budget_is_checked_before_insert_and_context_restores_after_error() {
        let value = ArithmeticValue::Integer(crate::IntegerValue::int(7));
        let mut values = Values {
            bytes: MAX_BYTES,
            ..Default::default()
        };
        assert!(values.insert("object", value, 5).is_err());
        assert!(values.entries.is_empty());
        assert_eq!(values.bytes, MAX_BYTES);

        let analysis = crate::analyze_with_profile(
            "",
            toucan_target::CompilerProfile::new(
                toucan_target::Target::X86_64UnknownLinuxGnu,
                Compiler::Clang,
            )
            .unwrap(),
            &crate::AnalysisOptions::default(),
        )
        .unwrap();
        let mut analyzer = Analyzer::from_unit(analysis.into_unit());
        let span = Span { start: 0, end: 7 };
        let expression = Node::new(
            ast::Expression::Identifier(Box::new(Node::new(
                ast::Identifier {
                    name: "missing".into(),
                },
                span,
            ))),
            span,
        );
        assert!(analyzer.eval_initializer_arithmetic(&expression).is_err());
        assert!(!analyzer.evaluation.allows_const_object_reads());
        analyzer.with_const_object_reads(|analyzer| {
            assert!(analyzer.eval_initializer_arithmetic(&expression).is_err());
            assert!(analyzer.evaluation.allows_const_object_reads());
        });
        assert!(!analyzer.evaluation.allows_const_object_reads());
    }
}

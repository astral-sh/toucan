//! Declaration-level non-return promises without changing C function types.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{Error, Type, TypeKind, analyze::Analyzer};

const MAX_BINDINGS: usize = 65_536;

#[derive(Default)]
pub(crate) struct Registry {
    file: FxHashSet<String>,
    scopes: FxHashMap<usize, FxHashSet<String>>,
    bindings: usize,
}

impl<'ast> Analyzer<'ast> {
    /// A lexical promise, not a whole-program proof that a call cannot return.
    pub(crate) fn visible_noreturn(&self, name: &str) -> bool {
        self.noreturn_registry.as_ref().is_some_and(|registry| {
            registry.file.contains(name)
                || (1..=self.lexical_scopes.len()).rev().any(|depth| {
                    registry
                        .scopes
                        .get(&depth)
                        .is_some_and(|scope| scope.contains(name))
                })
        })
    }

    pub(crate) fn declaration_noreturn(
        &self,
        name: &str,
        ty: &Type,
        written: bool,
        previous_file: Option<usize>,
    ) -> Result<bool, Error> {
        let TypeKind::Function(function) = &self.unit.resolve(ty)?.kind else {
            return Ok(false);
        };
        Ok(written
            || function.noreturn
            || self.visible_noreturn(name)
            || previous_file.is_some_and(|index| self.unit.declarations[index].noreturn)
            || (previous_file.is_none()
                && self
                    .block_externs
                    .get(name)
                    .is_some_and(|previous| previous.noreturn)))
    }

    pub(crate) fn record_noreturn(
        &mut self,
        name: &str,
        value: bool,
        file: bool,
        offset: usize,
    ) -> Result<(), Error> {
        if !value {
            return Ok(());
        }
        let registry = self.noreturn_registry.get_or_insert_with(Default::default);
        if registry.file.contains(name) {
            return Ok(());
        }
        let depth = self.lexical_scopes.len();
        if !file
            && registry
                .scopes
                .get(&depth)
                .is_some_and(|scope| scope.contains(name))
        {
            return Ok(());
        }
        if registry.bindings >= MAX_BINDINGS {
            return Err(Error::new(
                offset,
                "noreturn declaration bindings exceed the 65536-entry limit",
            ));
        }
        registry.bindings += 1;
        if file {
            registry.file.insert(name.to_owned());
        } else {
            registry
                .scopes
                .entry(depth)
                .or_default()
                .insert(name.to_owned());
        }
        Ok(())
    }

    pub(crate) fn leave_noreturn_scope(&mut self) {
        if let Some(registry) = &mut self.noreturn_registry
            && let Some(scope) = registry.scopes.remove(&self.lexical_scopes.len())
        {
            registry.bindings -= scope.len();
        }
    }
}

//! Reject cycles in caller-built alias graphs before nullable alias emission.

use std::collections::BTreeSet;

use toucan_semantic::{Type, TypeKind};

use crate::{Emitter, Error, check_depth};

struct State<'a> {
    active: BTreeSet<&'a str>,
    complete: BTreeSet<&'a str>,
    remaining: usize,
}

impl Emitter<'_> {
    /// Named callbacks stop signature expansion, so validate their alias graph separately.
    pub(super) fn validate_alias_dependencies(&self) -> Result<(), Error> {
        if !self.options.nullable_function_typedefs {
            return Ok(());
        }
        let mut state = State {
            active: BTreeSet::new(),
            complete: BTreeSet::new(),
            remaining: 1_000_000,
        };
        for name in &self.aliases {
            self.validate_alias_name(name, &mut state, 0)?;
        }
        Ok(())
    }

    fn validate_alias_name<'a>(
        &'a self,
        name: &'a str,
        state: &mut State<'a>,
        depth: usize,
    ) -> Result<(), Error> {
        check_depth(depth)?;
        if self.options.blocks_type(name)
            || (self.options.size_t_is_usize && name == "size_t")
            || state.complete.contains(name)
        {
            return Ok(());
        }
        if !state.active.insert(name) {
            return Err(Error(format!("cyclic Rust type alias `{name}`")));
        }
        let ty = self
            .unit
            .typedefs
            .get(name)
            .ok_or_else(|| Error(format!("unknown typedef `{name}`")))?;
        self.validate_alias_type(ty, state, depth)?;
        state.active.remove(name);
        state.complete.insert(name);
        Ok(())
    }

    fn validate_alias_type<'a>(
        &'a self,
        ty: &'a Type,
        state: &mut State<'a>,
        depth: usize,
    ) -> Result<(), Error> {
        check_depth(depth)?;
        state.remaining = state.remaining.checked_sub(1).ok_or_else(|| {
            Error("alias dependency validation exceeds the 1000000-node binding limit".into())
        })?;
        match &ty.kind {
            TypeKind::Typedef(name) => self.validate_alias_name(name, state, depth + 1)?,
            TypeKind::Pointer(inner)
            | TypeKind::Atomic(inner)
            | TypeKind::Vector { element: inner, .. }
            | TypeKind::Array { element: inner, .. }
            | TypeKind::VariableArray { element: inner, .. } => {
                self.validate_alias_type(inner, state, depth + 1)?;
            }
            TypeKind::Function(function) => {
                self.validate_alias_type(&function.return_type, state, depth + 1)?;
                for parameter in &function.parameters {
                    self.validate_alias_type(&parameter.ty, state, depth + 1)?;
                }
            }
            // A named record provides indirection in Rust's type graph. Its fields
            // can refer back to this alias without forming a recursive type alias.
            _ => {}
        }
        Ok(())
    }
}

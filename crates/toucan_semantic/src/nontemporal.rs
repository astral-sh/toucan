//! Clang's typed memory accesses with a non-temporal cache hint.

use lang_c::{ast, span::Node};
use serde::Serialize;

use crate::{Error, Type, TypeKind, analyze::Analyzer};

/// An ordinary memory access with a cache hint that a backend may ignore.
///
/// These operations are neither atomic nor volatile, even if the address points
/// to a const- or volatile-qualified type. Qualifiers and effects of evaluating
/// the address and stored value remain on their expression uses. Storing through
/// a const-qualified pointer does not permit modifying an actually const object.
/// No synchronization or instruction-set requirement is implied by the hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum NontemporalOperation {
    Load,
    Store,
}

impl NontemporalOperation {
    /// Position of the pointer whose pointee determines the memory access type.
    pub fn address_argument(self) -> usize {
        match self {
            Self::Load => 0,
            Self::Store => 1,
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "__builtin_nontemporal_load" => Some(Self::Load),
            "__builtin_nontemporal_store" => Some(Self::Store),
            _ => None,
        }
    }
}

impl<'ast> Analyzer<'ast> {
    pub(crate) fn nontemporal_value_type(
        &mut self,
        operation: NontemporalOperation,
        call: &Node<ast::CallExpression>,
    ) -> Result<Type, Error> {
        let offset = call.span.start;
        if self.gnu_sync_profile() {
            return Err(Error::new(
                offset,
                "non-temporal builtins require a Clang compiler profile",
            ));
        }
        let address_index = operation.address_argument();
        if call.node.arguments.len() != address_index + 1 {
            return Err(Error::new(
                offset,
                "argument count does not match non-temporal intrinsic",
            ));
        }
        let address = self.value_expression_type(&call.node.arguments[address_index])?;
        let TypeKind::Pointer(pointee) = &address.kind else {
            return Err(Error::new(offset, "non-temporal address must be a pointer"));
        };
        let value = self.unqualified(pointee)?;
        if !matches!(
            value.kind,
            TypeKind::Bool
                | TypeKind::Integer(_)
                | TypeKind::Enum(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Pointer(_)
                | TypeKind::Vector { .. }
        ) {
            return Err(Error::new(
                offset,
                "non-temporal address must point to an integer, floating, pointer, or fixed-vector object",
            ));
        }
        if self.unit.target.is_windows()
            && let TypeKind::Enum(id) = value.kind
            && !self.unit.enums[id].complete
        {
            return Err(Error::new(
                offset,
                "Microsoft forward-enum non-temporal accesses require unsupported incomplete-enum layouts",
            ));
        }
        self.require_complete_object(&value, offset)?;
        Ok(value)
    }

    pub(crate) fn nontemporal_call_type(
        &mut self,
        operation: NontemporalOperation,
        call: &Node<ast::CallExpression>,
    ) -> Result<Type, Error> {
        let value = self.nontemporal_value_type(operation, call)?;
        if operation == NontemporalOperation::Load {
            return Ok(value);
        }
        let argument = &call.node.arguments[0];
        let source = self.value_expression_type(argument)?;
        // Clang's parameter initialization permits its default equal-size vector
        // reinterpretation. Scalars still use the ordinary assignment rules.
        if !(matches!(value.kind, TypeKind::Vector { .. })
            && matches!(source.kind, TypeKind::Vector { .. })
            && self.unit.layout(&value)?.size_bits == self.unit.layout(&source)?.size_bits)
        {
            self.check_assignment_type(&value, &source, argument)?;
        }
        Ok(Type::new(TypeKind::Void))
    }
}

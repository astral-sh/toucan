//! Clang 18 integer elementwise operations, with its scalar promotion rules.
use crate::{Error, Type, TypeKind, analyze::Analyzer};
use lang_c::{ast, span::Node};
use serde::Serialize;

/// Integer operations applied independently to scalar values or vector lanes.
/// Both converted operands are evaluated once; their relative order is unspecified.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum ElementwiseOperation {
    /// Addition clamped to the result element type's representable range.
    AddSaturating,
    /// Subtraction clamped to the result element type's representable range.
    SubtractSaturating,
    Minimum,
    Maximum,
}
impl ElementwiseOperation {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "__builtin_elementwise_add_sat" => Some(Self::AddSaturating),
            "__builtin_elementwise_sub_sat" => Some(Self::SubtractSaturating),
            "__builtin_elementwise_min" => Some(Self::Minimum),
            "__builtin_elementwise_max" => Some(Self::Maximum),
            _ => None,
        }
    }
}
impl Analyzer {
    /// Clang 18 applies scalar arithmetic conversions but does not splat scalar
    /// operands into vectors. Later Clang versions changed these promotion rules.
    pub(crate) fn elementwise_type(
        &mut self,
        operation: ElementwiseOperation,
        call: &Node<ast::CallExpression>,
    ) -> Result<Type, Error> {
        let offset = call.span.start;
        if self.gnu_vector_profile() {
            return Err(Error::new(
                offset,
                "elementwise intrinsics require a Clang compiler profile",
            ));
        }
        if call.node.arguments.len() != 2 {
            return Err(Error::new(
                offset,
                "elementwise intrinsics require two arguments",
            ));
        }
        let left = self.expression_info(&call.node.arguments[0])?;
        let right = self.expression_info(&call.node.arguments[1])?;
        let lhs = self.converted_type(&left, offset)?;
        let rhs = self.converted_type(&right, offset)?;
        if lhs.alignment.is_some() || rhs.alignment.is_some() {
            return Err(Error::new(
                offset,
                "elementwise operations on explicitly aligned typedef operands are unsupported",
            ));
        }
        let result = match (&lhs.kind, &rhs.kind) {
            (TypeKind::Vector { .. }, TypeKind::Vector { .. }) => {
                if !self.same_type(&lhs, &rhs, 0)? {
                    return Err(Error::new(
                        offset,
                        "elementwise vector operands must have the same type",
                    ));
                }
                lhs
            }
            (TypeKind::Vector { .. }, _) | (_, TypeKind::Vector { .. }) => {
                return Err(Error::new(
                    offset,
                    "elementwise operations cannot mix vector and scalar operands",
                ));
            }
            _ => self.arithmetic_type(&left, &right, offset)?,
        };
        let element = if let TypeKind::Vector { element, .. } = &result.kind {
            element.as_ref()
        } else {
            &result
        };
        match self.unit.resolve(element)?.kind {
            TypeKind::Integer(_) => Ok(result),
            TypeKind::Float(_)
                if matches!(
                    operation,
                    ElementwiseOperation::Minimum | ElementwiseOperation::Maximum
                ) =>
            {
                Err(Error::new(
                    offset,
                    "floating elementwise min/max operations are unsupported",
                ))
            }
            _ => Err(Error::new(
                offset,
                "elementwise integer operations require integer scalar or vector elements",
            )),
        }
    }
}

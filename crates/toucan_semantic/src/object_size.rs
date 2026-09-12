//! Object-size query constraints, without inferred allocation extents.

use lang_c::{ast, span::Node};

use crate::analyze::Analyzer;
use crate::floating::ArithmeticValue;
use crate::integer::integer_to_type;
use crate::{Error, IntegerKind, Type, TypeKind};

pub(crate) struct ObjectSizeSignature {
    pub(crate) result: Type,
    pub(crate) parameters: [Type; 2],
}

impl<'ast> Analyzer<'ast> {
    pub(crate) fn object_size_signature(&self, name: &str) -> Option<ObjectSizeSignature> {
        if !is_object_size_builtin(name) {
            return None;
        }
        let mut pointee = Type::new(TypeKind::Void);
        pointee.qualifiers.is_const = true;
        Some(ObjectSizeSignature {
            result: integer_to_type(self.size_value(0)),
            parameters: [
                pointee.pointer(),
                Type::new(TypeKind::Integer(IntegerKind::Int)),
            ],
        })
    }

    /// The mode is checked after the prototype's conversion to int. GNU also
    /// accepts foldable floating/comma expressions; Clang requires a C ICE.
    pub(crate) fn check_object_size_mode(
        &mut self,
        expression: &Node<ast::Expression>,
        destination: &Type,
    ) -> Result<(), Error> {
        let gnu = self.unit.compiler == toucan_target::Compiler::Gnu;
        let offset = expression.span.start;
        if !gnu && !self.is_integer_constant_expression(expression, 0)? {
            return Err(Error::new(
                offset,
                "object-size mode requires an integer constant expression",
            ));
        }
        let value = self.object_size_mode_value(expression).map_err(|_| {
            Error::new(
                offset,
                "object-size mode cannot be evaluated as a supported constant",
            )
        })?;
        let value = self
            .convert_arithmetic(value, destination, offset)?
            .integer(offset)?;
        if value.signed_value() < 0 || value.value > 3 {
            return Err(Error::new(
                offset,
                "object-size mode must be between zero and three after conversion to int",
            ));
        }
        Ok(())
    }

    pub(crate) fn object_size_mode_value(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ArithmeticValue, Error> {
        self.enter_expression(expression.span.start)?;
        let result = if let ast::Expression::Comma(operands) = &expression.node {
            let mut result = None;
            for operand in operands.get(self.arena).iter() {
                match self.object_size_mode_value(operand) {
                    Ok(value) => result = Some(Ok(value)),
                    Err(error) => {
                        result = Some(Err(error));
                        break;
                    }
                }
            }
            result
                .unwrap_or_else(|| Err(Error::new(expression.span.start, "empty comma expression")))
        } else {
            self.eval_arithmetic(expression)
        };
        self.leave_expression();
        result
    }
}

pub(crate) fn is_object_size_builtin(name: &str) -> bool {
    matches!(
        name,
        "__builtin_object_size" | "__builtin_dynamic_object_size"
    )
}

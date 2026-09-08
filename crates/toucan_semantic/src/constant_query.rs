//! Conservative constant knowledge, without optimizer-dependent propagation.

use lang_c::{ast, span::Node};

use crate::analyze::Analyzer;
use crate::{Error, IntegerValue, TypeKind};

impl Analyzer {
    pub(crate) fn eval_constant_query(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<IntegerValue, Error> {
        // Check every operand before attempting a fold. Invalid syntax/types must
        // not become a successful query returning zero. Retained type-name and
        // statement scopes are also established before speculative evaluation.
        self.builtin_call_type(call)?;
        let checkpoint = self.sve_feature_checkpoint();
        let known = self.known_constant_operand(&call.node.arguments[0])?;
        // This second pass determines constant knowledge, not execution.
        self.discard_sve_feature_uses(checkpoint);
        Ok(IntegerValue::int(i128::from(known)))
    }

    fn known_constant_operand(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<bool, Error> {
        self.enter_expression(expression.span.start)?;
        let result = self.known_constant_operand_inner(expression);
        self.leave_expression();
        result
    }

    fn known_constant_operand_inner(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<bool, Error> {
        use ast::{BinaryOperator as Binary, UnaryOperator as Unary};
        match &expression.node {
            ast::Expression::TypesCompatible(query) => {
                self.eval_types_compatible(query)?;
                return Ok(true);
            }
            ast::Expression::StringLiteral(_) => return Ok(true),
            ast::Expression::Constant(_)
            | ast::Expression::SizeOfTy(_)
            | ast::Expression::SizeOfVal(_)
            | ast::Expression::AlignOf(_)
            | ast::Expression::OffsetOf(_) => {}
            ast::Expression::Identifier(identifier) => {
                if !self.unit.constants.contains_key(&identifier.node.name) {
                    return Ok(false);
                }
            }
            ast::Expression::Cast(cast) => {
                if !self.known_constant_operand(&cast.node.expression)? {
                    return Ok(false);
                }
                let ty = self.type_name(&cast.node.type_name.node)?;
                if matches!(self.unit.resolve(&ty)?.kind, TypeKind::Pointer(_)) {
                    return Ok(!self.unit.is_variably_modified(&ty)?);
                }
            }
            ast::Expression::UnaryOperator(unary) => {
                if !matches!(
                    unary.node.operator.node,
                    Unary::Plus | Unary::Minus | Unary::Complement | Unary::Negate
                ) || !self.known_constant_operand(&unary.node.operand)?
                {
                    return Ok(false);
                }
            }
            ast::Expression::BinaryOperator(binary) => {
                if !self.known_constant_operand(&binary.node.lhs)? {
                    return Ok(false);
                }
                if matches!(
                    binary.node.operator.node,
                    Binary::LogicalAnd | Binary::LogicalOr
                ) && let Ok(left) = self.eval_arithmetic(&binary.node.lhs)
                    && ((binary.node.operator.node == Binary::LogicalAnd && !left.truth())
                        || (binary.node.operator.node == Binary::LogicalOr && left.truth()))
                {
                    return Ok(true);
                }
                if !self.known_constant_operand(&binary.node.rhs)? {
                    return Ok(false);
                }
            }
            ast::Expression::Conditional(conditional) => {
                if !self.known_constant_operand(&conditional.node.condition)? {
                    return Ok(false);
                }
                let Ok(condition) = self.eval_arithmetic(&conditional.node.condition) else {
                    return Ok(false);
                };
                let selected = if condition.truth() {
                    &conditional.node.then_expression
                } else {
                    &conditional.node.else_expression
                };
                if !self.known_constant_operand(selected)? {
                    return Ok(false);
                }
                let ty = self.expression_type(expression)?;
                if matches!(self.unit.resolve(&ty)?.kind, TypeKind::Pointer(_)) {
                    return Ok(true);
                }
            }
            ast::Expression::Choose(selection) => {
                let selected = self.choose_expression(selection)?;
                return self.known_constant_operand(selected);
            }
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                return self.known_constant_operand(selected);
            }
            ast::Expression::Call(call) => {
                let name = self.builtin_name(call);
                if name == Some("__c11_atomic_is_lock_free") {
                    return Ok(self.eval_c11_atomic_lock_free(call).is_ok());
                }
                if name
                    .and_then(crate::atomic::AtomicOperation::from_name)
                    .is_some_and(crate::atomic::AtomicOperation::is_lock_free_query)
                {
                    return Ok(self.eval_atomic_lock_free(call).is_ok());
                }
                if name
                    .and_then(crate::overflow::OverflowIntrinsic::from_name)
                    .is_some_and(crate::overflow::OverflowIntrinsic::is_predicate)
                {
                    return Ok(self.eval_overflow_predicate(call).is_ok());
                }
                if name == Some("__builtin_constant_p") {
                    return Ok(true);
                }
                if name
                    .and_then(|name| self.object_size_signature(name))
                    .is_some()
                {
                    // Later object-size facts do not prove that the compiler's
                    // frontend can fold this enclosing constant query.
                    return Ok(self.infer_object_size(call)?.frontend_fold());
                }
                if !name.is_some_and(|name| {
                    name == "__builtin_expect"
                        || self.byte_swap_type(name).is_some()
                        || self.bit_count_type(name).is_some()
                        || self.infinity_builtin_kind(name).is_some()
                        || self.nan_builtin(name).is_some()
                }) {
                    return Ok(false);
                }
                for argument in &call.node.arguments {
                    if !self.known_constant_operand(argument)? {
                        return Ok(false);
                    }
                }
            }
            // Reads, writes, compound literals and statement expressions require
            // knowledge beyond this frontend's supported constant folds.
            _ => return Ok(false),
        }
        // A valid expression need not have a valid constant value: overflow,
        // division by zero, or a VLA size are not proofs of constant knowledge.
        Ok(self.eval_arithmetic(expression).is_ok())
    }
}

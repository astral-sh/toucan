//! GNU argument packs are expanded while inlining, not passed as integer values.

use lang_c::{ast, span::Node};

use crate::analyze::Analyzer;
use crate::{Error, IntegerKind, TypeKind};

impl Analyzer {
    /// Recognizes a pack after its argument expression has been type-checked.
    /// C's int type describes unevaluated uses; it does not describe a runtime pack.
    pub(crate) fn argument_pack(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<bool, Error> {
        if !self.has_variadic_packs {
            return Ok(false);
        }
        self.enter_expression(expression.span.start)?;
        let result = self.argument_pack_inner(expression);
        self.leave_expression();
        result
    }

    fn argument_pack_inner(&mut self, expression: &Node<ast::Expression>) -> Result<bool, Error> {
        match &expression.node {
            ast::Expression::Call(call) => {
                Ok(self.builtin_name(call) == Some("__builtin_va_arg_pack"))
            }
            ast::Expression::Cast(cast) => {
                let ty = self.type_name(&cast.node.type_name.node)?;
                if matches!(
                    self.unit.resolve(&ty)?.kind,
                    TypeKind::Integer(IntegerKind::Int)
                ) {
                    self.argument_pack(&cast.node.expression)
                } else {
                    Ok(false)
                }
            }
            ast::Expression::UnaryOperator(unary)
                if unary.node.operator.node == ast::UnaryOperator::Plus =>
            {
                self.argument_pack(&unary.node.operand)
            }
            ast::Expression::Choose(selection) => {
                let selected = self.checked_choose_expression(selection)?;
                self.argument_pack(selected)
            }
            ast::Expression::GenericSelection(selection) => {
                let key = (selection.span.start, selection.span.end);
                let index = *self.generic_selections.get(&key).ok_or_else(|| {
                    Error::new(
                        selection.span.start,
                        "argument generic selection was not checked",
                    )
                })?;
                let selected = match &selection.node.associations[index].node {
                    ast::GenericAssociation::Type(association) => &association.node.expression,
                    ast::GenericAssociation::Default(expression) => expression,
                };
                self.argument_pack(selected)
            }
            _ => Ok(false),
        }
    }

    /// A recognized expansion follows all fixed and explicit variadic arguments.
    pub(crate) fn check_argument_pack(
        &mut self,
        argument: &Node<ast::Expression>,
        index: usize,
        argument_count: usize,
        fixed_count: usize,
        variadic: bool,
    ) -> Result<(), Error> {
        if self.argument_pack(argument)?
            && (!variadic || index < fixed_count || index + 1 != argument_count)
        {
            return Err(Error::new(
                argument.span.start,
                "variadic argument pack requires the final variadic argument position",
            ));
        }
        Ok(())
    }
}

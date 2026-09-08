//! Compiler intrinsics used by C standard headers and inline functions.

use lang_c::{ast, span::Node};

use crate::analyze::Analyzer;
use crate::{Error, IntegerKind, IntegerValue, Type, TypeKind};

impl Analyzer {
    /// Recognizes intrinsics only when an ordinary declaration has not shadowed
    /// their names. Intrinsics never become exported external declarations.
    pub(crate) fn builtin_name<'a>(&self, call: &'a Node<ast::CallExpression>) -> Option<&'a str> {
        let ast::Expression::Identifier(identifier) = &call.node.callee.node else {
            return None;
        };
        let name = identifier.node.name.as_str();
        if self
            .lexical_scopes
            .iter()
            .any(|scope| scope.names.contains_key(name))
            || self
                .unit
                .declarations
                .iter()
                .any(|declaration| declaration.name == name)
            || self.unit.constants.contains_key(name)
        {
            return None;
        }
        Some(name)
    }

    pub(crate) fn builtin_call_type(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<Option<Type>, Error> {
        let Some(name) = self.builtin_name(call) else {
            return Ok(None);
        };
        let arity = match name {
            "__builtin_va_start" | "__builtin_va_copy" | "__builtin_expect" => 2,
            "__builtin_va_end" => 1,
            "__builtin_unreachable" | "__builtin_trap" => 0,
            _ => return Ok(None),
        };
        let arguments = &call.node.arguments;
        let offset = call.span.start;
        if arguments.len() != arity {
            return Err(Error::new(
                offset,
                format!("{name} requires {arity} arguments"),
            ));
        }
        match name {
            "__builtin_expect" => {
                let ty = Type::new(TypeKind::Integer(IntegerKind::Long));
                for argument in arguments {
                    self.check_assignment(&ty, argument)?;
                }
                return Ok(Some(ty));
            }
            "__builtin_va_start" => {
                let Some(function) = self.current_function_signature() else {
                    return Err(Error::new(
                        offset,
                        "va_start requires a variadic function body",
                    ));
                };
                if !function.variadic {
                    return Err(Error::new(
                        offset,
                        "va_start requires a variadic function body",
                    ));
                }
                let ast::Expression::Identifier(identifier) = &arguments[1].node else {
                    return Err(Error::new(
                        arguments[1].span.start,
                        "va_start requires the last named parameter",
                    ));
                };
                let name = &identifier.node.name;
                if function
                    .parameters
                    .last()
                    .and_then(|parameter| parameter.name.as_ref())
                    != Some(name)
                    || self.current_function_parameter(name).is_none()
                {
                    return Err(Error::new(
                        arguments[1].span.start,
                        "va_start requires the last named parameter",
                    ));
                }
                self.check_va_list(&arguments[0], false)?;
            }
            "__builtin_va_copy" => {
                self.check_va_list(&arguments[0], false)?;
                self.check_va_list(&arguments[1], false)?;
            }
            "__builtin_va_end" => self.check_va_list(&arguments[0], false)?,
            _ => {}
        }
        Ok(Some(Type::new(TypeKind::Void)))
    }

    /// Array va_list ABIs pass a pointer to the first state record. Other ABIs
    /// mutate the list object itself, requiring a modifiable lvalue.
    fn check_va_list(
        &mut self,
        expression: &Node<ast::Expression>,
        exact: bool,
    ) -> Result<(), Error> {
        let offset = expression.span.start;
        let list = self
            .unit
            .typedefs
            .get("__builtin_va_list")
            .cloned()
            .ok_or_else(|| Error::new(offset, "target has no builtin va_list type"))?;
        if matches!(self.unit.resolve(&list)?.kind, TypeKind::Array { .. }) && !exact {
            return self.check_assignment(&self.value_type(&list)?, expression);
        }
        let info = self.expression_info(expression)?;
        let actual = self.converted_type(&info, offset)?;
        if !self.compatible(&actual, &self.value_type(&list)?)? {
            return Err(Error::new(
                offset,
                "argument must have the target's va_list type",
            ));
        }
        if !matches!(self.unit.resolve(&list)?.kind, TypeKind::Array { .. }) {
            self.require_modifiable(&info, offset)?;
        }
        Ok(())
    }

    pub(crate) fn va_arg_type(
        &mut self,
        argument: &Node<ast::VaArgExpression>,
    ) -> Result<Type, Error> {
        self.check_va_list(&argument.node.va_list, true)?;
        let ty = self.type_name(&argument.node.type_name.node)?;
        self.require_complete_object(&ty, argument.span.start)?;
        Ok(ty)
    }

    /// The prediction hint preserves the first argument after conversion to long.
    /// Constant evaluation requires both arguments to be arithmetic constants.
    pub(crate) fn eval_expect(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<IntegerValue, Error> {
        let ty = self
            .builtin_call_type(call)?
            .ok_or_else(|| Error::new(call.span.start, "expected builtin prediction hint"))?;
        let first = self.eval_arithmetic(&call.node.arguments[0])?;
        let second = self.eval_arithmetic(&call.node.arguments[1])?;
        self.convert_arithmetic(second, &ty, call.span.start)?;
        self.convert_arithmetic(first, &ty, call.span.start)?
            .integer(call.span.start)
    }
}

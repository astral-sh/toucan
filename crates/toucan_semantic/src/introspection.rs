//! GNU type compatibility and compile-time expression selection.

use lang_c::{ast, span::Node};

use crate::analyze::Analyzer;
use crate::{Error, IntegerValue, Type, TypeKind};

impl Analyzer {
    /// Resolves the condition and checks the discarded arm once per source site.
    /// The caller checks the selected arm, preserving its original value category.
    pub(crate) fn choose_expression<'a>(
        &mut self,
        selection: &'a Node<ast::ChooseExpression>,
    ) -> Result<&'a Node<ast::Expression>, Error> {
        let key = (selection.span.start, selection.span.end);
        if !self.choose_selections.contains_key(&key) {
            let checkpoint = self.sve_feature_checkpoint();
            let selected = (|| -> Result<bool, Error> {
                self.expression_type(&selection.node.condition)?;
                if !self.is_integer_constant_expression(&selection.node.condition, 0)? {
                    return Err(Error::new(
                        selection.node.condition.span.start,
                        "__builtin_choose_expr condition requires an integer constant expression",
                    ));
                }
                let selected = self.eval(&selection.node.condition)?.truth();
                self.expression_type(if selected {
                    &selection.node.else_expression
                } else {
                    &selection.node.then_expression
                })?;
                Ok(selected)
            })();
            self.discard_sve_feature_uses(checkpoint);
            let selected = selected?;
            if self.choose_selections.len() >= 65_536 {
                return Err(Error::new(
                    selection.span.start,
                    "compile-time selection count exceeds the 65536-entry limit",
                ));
            }
            self.choose_selections.insert(key, selected);
        }
        self.checked_choose_expression(selection)
    }

    /// Reads an established decision without rechecking discarded subtrees.
    pub(crate) fn checked_choose_expression<'a>(
        &self,
        selection: &'a Node<ast::ChooseExpression>,
    ) -> Result<&'a Node<ast::Expression>, Error> {
        let selected = self
            .choose_selections
            .get(&(selection.span.start, selection.span.end))
            .ok_or_else(|| {
                Error::new(
                    selection.span.start,
                    "compile-time selection was not checked",
                )
            })?;
        Ok(if *selected {
            &selection.node.then_expression
        } else {
            &selection.node.else_expression
        })
    }

    /// Checks both written types without evaluating their runtime type operands.
    pub(crate) fn eval_types_compatible(
        &mut self,
        query: &Node<ast::TypesCompatibleExpression>,
    ) -> Result<IntegerValue, Error> {
        let key = (query.span.start, query.span.end);
        let value = if let Some(value) = self.type_compatibility_results.get(&key) {
            *value
        } else {
            let checkpoint = self.sve_feature_checkpoint();
            let value = (|| -> Result<bool, Error> {
                let left = self.type_name(&query.node.left.node)?;
                let right = self.type_name(&query.node.right.node)?;
                let left = self.compatibility_operand(&left, 0)?;
                let right = self.compatibility_operand(&right, 0)?;
                self.compatible(&left, &right)
            })();
            self.discard_sve_feature_uses(checkpoint);
            let value = value?;
            if self.type_compatibility_results.len() >= 65_536 {
                return Err(Error::new(
                    query.span.start,
                    "type compatibility query count exceeds the 65536-entry limit",
                ));
            }
            self.type_compatibility_results.insert(key, value);
            value
        };
        Ok(IntegerValue::int(i128::from(value)))
    }

    /// Array qualification belongs to its element type. Strip only that outer
    /// chain, keeping qualifications beneath pointers and function types intact.
    fn compatibility_operand(&self, ty: &Type, depth: usize) -> Result<Type, Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "type compatibility nesting exceeds the 128-level limit",
            ));
        }
        let mut ty = self.unqualified(ty)?;
        if self.gnu_sync_profile()
            && let TypeKind::Atomic(value) = &ty.kind
        {
            ty = self.unqualified(value)?;
        }
        match &mut ty.kind {
            TypeKind::Array { element, .. } | TypeKind::VariableArray { element } => {
                **element = self.compatibility_operand(element, depth + 1)?;
            }
            _ => {}
        }
        Ok(ty)
    }

    /// GCC ignores ordinary return qualifiers in function compatibility; Clang
    /// retains them. Atomic return types retain their separate type identity.
    pub(crate) fn compatible_return_type(
        &self,
        left: &Type,
        right: &Type,
        depth: usize,
    ) -> Result<bool, Error> {
        if self.unit.compiler == toucan_target::Compiler::Gnu {
            self.compatible_at(&self.unqualified(left)?, &self.unqualified(right)?, depth)
        } else {
            self.compatible_at(left, right, depth)
        }
    }
}

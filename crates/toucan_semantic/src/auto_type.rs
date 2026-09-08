//! Initializer-based GNU declaration types, without an IR placeholder type.

use crate::checked::{TypeStep, TypeUseId, UseContext};
use crate::{DeclarationKind, Error, Type, TypeKind, analyze::Analyzer};
use lang_c::{
    ast,
    span::{Node, Span},
};

pub(crate) struct AutoInference<'a> {
    pub(crate) keyword: Span,
    pub(crate) expression: &'a Node<ast::Expression>,
    pub(crate) ty: Type,
    pub(crate) type_use: Option<TypeUseId>,
    pub(crate) reuses_prior_type: bool,
}

impl Analyzer {
    /// Infers before reserving the variable: GCC still sees outer bindings here.
    pub(crate) fn infer_auto_declaration<'a>(
        &mut self,
        declaration: &'a Node<ast::Declaration>,
    ) -> Result<Option<AutoInference<'a>>, Error> {
        let Some(keyword) = declaration
            .node
            .specifiers
            .iter()
            .find_map(|s| match &s.node {
                ast::DeclarationSpecifier::TypeSpecifier(Node {
                    node: ast::TypeSpecifier::AutoType,
                    span,
                }) => Some(*span),
                _ => None,
            })
        else {
            return Ok(None);
        };
        if declaration
            .node
            .specifiers
            .iter()
            .filter(|s| matches!(s.node, ast::DeclarationSpecifier::TypeSpecifier(_)))
            .count()
            != 1
        {
            return Err(Error::new(
                keyword.start,
                "__auto_type cannot combine with another type specifier",
            ));
        }
        if declaration.node.specifiers.iter().any(|s| {
            matches!(
                s.node,
                ast::DeclarationSpecifier::StorageClass(Node {
                    node: ast::StorageClassSpecifier::Typedef,
                    ..
                })
            )
        }) {
            return Err(Error::new(
                keyword.start,
                "__auto_type requires an object declaration",
            ));
        }
        let [item] = declaration.node.declarators.as_slice() else {
            return Err(Error::new(
                keyword.start,
                if self.gnu_sync_profile() {
                    "__auto_type requires exactly one variable"
                } else {
                    "multiple Clang __auto_type declarators are unsupported"
                },
            ));
        };
        let mut declarator = &item.node.declarator;
        let name = loop {
            if !declarator.node.derived.is_empty() {
                return Err(Error::new(
                    declarator.span.start,
                    if self.gnu_sync_profile() {
                        "__auto_type requires an identifier without derived declarators"
                    } else {
                        "derived Clang __auto_type declarators are unsupported"
                    },
                ));
            }
            match &declarator.node.kind.node {
                ast::DeclaratorKind::Identifier(name) => break name.node.name.as_str(),
                ast::DeclaratorKind::Declarator(inner) => declarator = inner,
                ast::DeclaratorKind::Abstract => {
                    return Err(Error::new(
                        declarator.span.start,
                        "__auto_type requires a named variable",
                    ));
                }
            }
        };
        let Some(Node {
            node: ast::Initializer::Expression(expression),
            ..
        }) = &item.node.initializer
        else {
            return Err(Error::new(
                item.span.start,
                "__auto_type requires an expression initializer",
            ));
        };
        if !self.gnu_sync_profile()
            && declaration.node.specifiers.iter().any(|s| {
                matches!(
                    s.node,
                    ast::DeclarationSpecifier::TypeQualifier(Node {
                        node: ast::TypeQualifier::Atomic,
                        ..
                    })
                )
            })
        {
            return Err(Error::new(
                keyword.start,
                "Clang _Atomic __auto_type deduction does not produce a concrete type",
            ));
        }
        if !self.gnu_sync_profile() {
            self.pending_auto_types
                .push((name.to_owned(), self.lexical_scopes.len()));
        }
        let result = self.expression_info(expression);
        if !self.gnu_sync_profile() {
            self.pending_auto_types.pop();
        }
        let info = result?;
        if info.bitfield.is_some() {
            return Err(Error::new(
                expression.span.start,
                "__auto_type cannot infer from a bitfield",
            ));
        }
        let converted = self.converted_type(&info, expression.span.start)?;
        let preserves_atomic =
            !self.gnu_sync_profile() && self.unit.atomic_value(&info.ty)?.is_some();
        let mut ty = if preserves_atomic {
            self.unqualified(&info.ty)?
        } else {
            converted
        };
        let mut reuses_prior_type = false;
        // Clang completes an existing file declaration with its established type.
        if !self.gnu_sync_profile()
            && !self.in_function_body()
            && let Some(previous) = self
                .unit
                .declarations
                .iter()
                .find(|d| d.name == name && d.kind == DeclarationKind::Variable)
        {
            ty = previous.ty.clone();
            reuses_prior_type = true;
        }
        if matches!(
            self.unit.resolve(&ty)?.kind,
            TypeKind::Void | TypeKind::Function(_)
        ) {
            return Err(Error::new(
                expression.span.start,
                "__auto_type initializer must supply an object value",
            ));
        }
        if self.unit.is_sizeless(&ty)? {
            return Err(Error::new(
                expression.span.start,
                "automatic SVE objects require unsupported target-feature configuration",
            ));
        }
        self.require_complete_object(&ty, expression.span.start)?;
        let type_use = if self.checked.is_some() {
            let value = self.retained_use(expression, UseContext::Value, None)?;
            Some(if preserves_atomic && info.lvalue && !reuses_prior_type {
                // Deduction preserves Clang's atomic wrapper, although reading the
                // initializer uses the ordinary atomic-to-value conversion.
                self.code_builder().wrap_type_use(
                    value.type_use(),
                    &ty,
                    TypeStep::AtomicValue,
                    None,
                    expression.span.start,
                )?
            } else {
                self.code_builder()
                    .retype_use(value.type_use(), &ty, expression.span.start)?
            })
        } else {
            None
        };
        Ok(Some(AutoInference {
            keyword,
            expression,
            ty,
            type_use,
            reuses_prior_type,
        }))
    }

    /// Clang's undeduced name hides outer values and typedefs in its initializer.
    pub(crate) fn check_auto_reference(&self, name: &str, offset: usize) -> Result<(), Error> {
        if let Some((_, depth)) = self
            .pending_auto_types
            .iter()
            .rev()
            .find(|(pending, _)| pending == name)
            && !self
                .lexical_scopes
                .iter()
                .skip(*depth)
                .any(|scope| scope.names.contains_key(name))
        {
            return Err(Error::new(
                offset,
                format!("variable `{name}` with deduced type cannot appear in its own initializer"),
            ));
        }
        Ok(())
    }
}

//! Initializer-based GNU declaration types, without an IR placeholder type.

use crate::checked::{TypeStep, TypeUseId, UseContext};
use crate::{
    DeclarationKind, Error, Qualifiers, Type, TypeKind,
    analyze::{Analyzer, Attributes, PreparedSpecifiers},
};
use lang_c::{
    ast,
    span::{Node, Span},
};

pub(crate) struct AutoDeclaration {
    keyword: Span,
    multiple: bool,
    deduced: Option<Type>,
    prepared: Option<PreparedSpecifiers>,
}

pub(crate) struct AutoInference<'a> {
    pub(crate) keyword: Span,
    pub(crate) expression: &'a Node<ast::Expression>,
    pub(crate) ty: Type,
    pub(crate) type_use: Option<TypeUseId>,
    pub(crate) reuses_prior_type: bool,
    source_type: Option<Type>,
    layers: Vec<&'a Node<ast::DerivedDeclarator>>,
}

impl Analyzer {
    /// Validates the common declaration before checking each initializer in order.
    pub(crate) fn auto_declaration(
        &self,
        declaration: &Node<ast::Declaration>,
    ) -> Result<Option<AutoDeclaration>, Error> {
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
        let count = declaration.node.declarators.len();
        if count == 0 || (self.gnu_sync_profile() && count != 1) {
            return Err(Error::new(
                keyword.start,
                "__auto_type requires exactly one variable",
            ));
        }
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
        Ok(Some(AutoDeclaration {
            keyword,
            multiple: count > 1,
            deduced: None,
            prepared: None,
        }))
    }

    /// Common specifier attributes run once; each item supplies its own inferred base.
    pub(crate) fn auto_item<'a>(
        &mut self,
        declaration: &'a Node<ast::Declaration>,
        item: &'a Node<ast::InitDeclarator>,
        group: &mut AutoDeclaration,
    ) -> Result<(Type, Attributes, AutoInference<'a>), Error> {
        let inference = self.infer_auto_item(declaration, item, group.keyword)?;
        if group.multiple && !inference.reuses_prior_type {
            if let Some(previous) = &group.deduced {
                if !self.same_deduced_type(previous, &inference.ty)? {
                    return Err(Error::new(
                        item.span.start,
                        "Clang __auto_type declarators must deduce the same base type",
                    ));
                }
            } else {
                group.deduced = Some(inference.ty.clone());
            }
        }
        let prepared = if let Some(prepared) = &group.prepared {
            prepared.clone()
        } else {
            let prepared = self.prepare_specifiers(&declaration.node.specifiers)?;
            if group.multiple {
                group.prepared = Some(prepared.clone());
            }
            prepared
        };
        let (base, attributes) =
            self.complete_specifiers(&declaration.node.specifiers, prepared, Some(&inference))?;
        Ok((base, attributes, inference))
    }

    /// Infers before reserving the variable: GCC still sees outer bindings here.
    fn infer_auto_item<'a>(
        &mut self,
        declaration: &'a Node<ast::Declaration>,
        item: &'a Node<ast::InitDeclarator>,
        keyword: Span,
    ) -> Result<AutoInference<'a>, Error> {
        let mut declarator = &item.node.declarator;
        let mut layers = Vec::new();
        let mut depth = 0;
        let name = loop {
            depth += 1;
            if depth > 128 || layers.len() + declarator.node.derived.len() > 128 {
                return Err(Error::new(
                    declarator.span.start,
                    "__auto_type declarator nesting exceeds 128 levels",
                ));
            }
            if self.gnu_sync_profile() && !declarator.node.derived.is_empty() {
                return Err(Error::new(
                    declarator.span.start,
                    "__auto_type requires an identifier without derived declarators",
                ));
            }
            let split = declarator
                .node
                .derived
                .iter()
                .take_while(|d| matches!(d.node, ast::DerivedDeclarator::Pointer(_)))
                .count();
            layers.extend(declarator.node.derived[..split].iter());
            layers.extend(declarator.node.derived[split..].iter().rev());
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
        for layer in &layers {
            if let ast::DerivedDeclarator::Pointer(qualifiers) = &layer.node
                && qualifiers.iter().any(|q| {
                    matches!(
                        q.node,
                        ast::PointerQualifier::TypeQualifier(Node {
                            node: ast::TypeQualifier::Atomic,
                            ..
                        })
                    )
                })
            {
                return Err(Error::new(
                    layer.span.start,
                    "Clang atomic __auto_type pointer deduction does not produce a concrete type",
                ));
            }
        }
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
        let mut type_use = if self.checked.is_some() {
            let value = self.retained_use(expression, UseContext::Value, None)?;
            Some(if preserves_atomic && info.lvalue && !reuses_prior_type {
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
        let source_type = (!layers.is_empty()).then(|| ty.clone());
        for layer in layers.iter().rev() {
            let (inner, step) = match (&layer.node, &self.unit.resolve(&ty)?.kind) {
                (ast::DerivedDeclarator::Pointer(_), TypeKind::Pointer(inner)) => {
                    ((**inner).clone(), TypeStep::Pointer)
                }
                (ast::DerivedDeclarator::Array(_), TypeKind::Array { element, .. }) => {
                    let mut element = (**element).clone();
                    let outer = self.unit.qualifiers(&ty)?;
                    element.qualifiers = self.unit.qualifiers(&element)?;
                    element.qualifiers.is_const |= outer.is_const;
                    element.qualifiers.is_volatile |= outer.is_volatile;
                    element.qualifiers.is_restrict |= outer.is_restrict;
                    (element, TypeStep::Element)
                }
                (ast::DerivedDeclarator::Function(_), TypeKind::Function(function))
                    if function.prototype =>
                {
                    (function.return_type.clone(), TypeStep::Return)
                }
                _ => {
                    return Err(Error::new(
                        layer.span.start,
                        "Clang __auto_type declarator does not match its initializer type",
                    ));
                }
            };
            ty = inner;
            if let Some(id) = type_use {
                type_use =
                    Some(
                        self.code_builder()
                            .project_type_use(id, &ty, step, layer.span.start)?,
                    );
            }
        }
        let mut written = Qualifiers::default();
        for specifier in &declaration.node.specifiers {
            if let ast::DeclarationSpecifier::TypeQualifier(q) = &specifier.node {
                match q.node {
                    ast::TypeQualifier::Const => written.is_const = true,
                    ast::TypeQualifier::Volatile => written.is_volatile = true,
                    ast::TypeQualifier::Restrict => written.is_restrict = true,
                    _ => {}
                }
            }
        }
        if written != Qualifiers::default()
            && matches!(self.unit.resolve(&ty)?.kind, TypeKind::Function(_))
        {
            return Err(Error::new(
                keyword.start,
                "Clang __auto_type cannot deduce a qualified function base",
            ));
        }
        // Written base qualifiers constrain the pattern; they are applied again
        // after deduction and therefore do not belong to the inferred base.
        if self.remove_auto_qualifiers(&mut ty, written, 0)?
            && let Some(id) = type_use
        {
            type_use = Some(
                self.code_builder()
                    .retype_use(id, &ty, expression.span.start)?,
            );
        }
        Ok(AutoInference {
            keyword,
            expression,
            ty,
            type_use,
            reuses_prior_type,
            source_type,
            layers,
        })
    }

    fn remove_auto_qualifiers(
        &self,
        ty: &mut Type,
        written: Qualifiers,
        depth: usize,
    ) -> Result<bool, Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "__auto_type qualifier nesting exceeds 128 levels",
            ));
        }
        let mut changed = false;
        if matches!(ty.kind, TypeKind::Typedef(_)) {
            let qualifiers = self.unit.qualifiers(ty)?;
            let alignment = self.unit.typedef_alignment(ty)?;
            *ty = self.unit.resolve(ty)?.clone();
            ty.qualifiers = qualifiers;
            ty.alignment = alignment;
            changed = true;
        }
        let previous = ty.qualifiers;
        ty.qualifiers.is_const &= !written.is_const;
        ty.qualifiers.is_volatile &= !written.is_volatile;
        ty.qualifiers.is_restrict &= !written.is_restrict;
        changed |= previous != ty.qualifiers;
        if let TypeKind::Array { element, .. } | TypeKind::VariableArray { element, .. } =
            &mut ty.kind
        {
            changed |= self.remove_auto_qualifiers(element, written, depth + 1)?;
        }
        Ok(changed)
    }

    /// Parameter types and written array bounds are non-deduced parts of the pattern.
    pub(crate) fn check_auto_declarator(
        &self,
        inference: &AutoInference<'_>,
        declaration: &Node<ast::Declarator>,
        ty: &Type,
    ) -> Result<(), Error> {
        let Some(source) = &inference.source_type else {
            return Ok(());
        };
        let mut left = ty;
        let mut right = source;
        for layer in inference.layers.iter().rev() {
            let pair = match (
                &layer.node,
                &self.unit.resolve(left)?.kind,
                &self.unit.resolve(right)?.kind,
            ) {
                (
                    ast::DerivedDeclarator::Pointer(_),
                    TypeKind::Pointer(a),
                    TypeKind::Pointer(b),
                ) => Some((&**a, &**b)),
                (
                    ast::DerivedDeclarator::Array(_),
                    TypeKind::Array {
                        element: a,
                        length: al,
                    },
                    TypeKind::Array {
                        element: b,
                        length: bl,
                    },
                ) if al == bl => Some((&**a, &**b)),
                (
                    ast::DerivedDeclarator::Function(_),
                    TypeKind::Function(a),
                    TypeKind::Function(b),
                ) if a.prototype
                    && b.prototype
                    && a.variadic == b.variadic
                    && a.parameters.len() == b.parameters.len()
                    && a.calling_convention.for_target(self.unit.target)?
                        == b.calling_convention.for_target(self.unit.target)? =>
                {
                    for (a, b) in a.parameters.iter().zip(&b.parameters) {
                        if !self.same_deduced_type(
                            &self.unqualified(&a.ty)?,
                            &self.unqualified(&b.ty)?,
                        )? {
                            return Err(Error::new(
                                layer.span.start,
                                "Clang __auto_type function parameter does not match its initializer",
                            ));
                        }
                    }
                    Some((&a.return_type, &b.return_type))
                }
                _ => None,
            };
            let Some((a, b)) = pair else {
                return Err(Error::new(
                    declaration.span.start,
                    "Clang __auto_type declarator does not match its initializer type",
                ));
            };
            left = a;
            right = b;
        }
        Ok(())
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

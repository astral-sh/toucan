//! Identifier-list definitions keep entry types separate from canonical prototypes.

use crate::analyze::{Analyzer, LexicalScope};
use crate::checked::{
    DeclarationGroupId, OccurrenceId, OccurrenceKind, ScopeId, ScopeKind, SiteId,
};
use crate::integer::{integer_to_type, promote};
use crate::parameters::ParameterSyntax;
use crate::{Error, FloatKind, Qualifiers, Type, TypeKind};
use lang_c::{
    ast,
    span::{Node, Span},
};
use std::collections::{BTreeMap, HashMap};

const MAX_PARAMETERS: usize = 65_536;
const MAX_METADATA_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct Definitions {
    incoming: BTreeMap<usize, Vec<Type>>,
    bytes: usize,
}

pub(crate) struct Signature {
    pub(crate) incoming: Vec<Type>,
    pub(crate) retained: Option<Retained>,
}

pub(crate) struct Retained {
    pub(crate) declarations: Vec<DeclarationGroupId>,
    pub(crate) parameters: Vec<(OccurrenceId, SiteId)>,
}

pub(crate) fn validate_definition_shape(
    definition: &Node<ast::FunctionDefinition>,
) -> Result<(), Error> {
    if !crate::analyze::outermost_derived(&definition.node.declarator).is_some_and(|derived| {
        matches!(
            derived.node,
            ast::DerivedDeclarator::Function(_) | ast::DerivedDeclarator::KRFunction(_)
        )
    }) {
        return Err(Error::new(
            definition.node.declarator.span.start,
            "function definition requires an explicit function declarator",
        ));
    }
    if !definition.node.declarations.is_empty()
        && !crate::analyze::outermost_derived(&definition.node.declarator)
            .is_some_and(|d| matches!(d.node, ast::DerivedDeclarator::KRFunction(_)))
    {
        return Err(Error::new(
            definition.node.declarations[0].span.start,
            "a prototype definition cannot have a parameter declaration list",
        ));
    }
    Ok(())
}

impl Analyzer {
    pub(crate) fn check_old_style_parameters(
        &mut self,
        definition: &Node<ast::FunctionDefinition>,
        identifiers: &[Node<ast::Identifier>],
        span: Span,
    ) -> Result<Option<ScopeId>, Error> {
        if identifiers.len() > MAX_PARAMETERS {
            return Err(Error::new(
                span.start,
                "old-style definition exceeds the 65536-parameter limit",
            ));
        }
        let mut indices = HashMap::with_capacity(identifiers.len());
        for (index, identifier) in identifiers.iter().enumerate() {
            if self.unit.typedefs.contains_key(&identifier.node.name) {
                return Err(Error::new(
                    identifier.span.start,
                    "a typedef name cannot appear in an identifier-list definition",
                ));
            }
            if indices
                .insert(identifier.node.name.as_str(), index)
                .is_some()
            {
                return Err(Error::new(
                    identifier.span.start,
                    "duplicate identifier in function parameter list",
                ));
            }
        }
        if let Some(checked) = &mut self.checked {
            checked.reserve_old_style(
                identifiers.len(),
                definition.node.declarations.len(),
                span.start,
            )?;
        }
        let checked_scope = self
            .checked
            .as_mut()
            .map(|checked| checked.enter_scope(ScopeKind::Prototype, span, None))
            .transpose()?;
        self.lexical_scopes.push(LexicalScope {
            is_definition_parameters: true,
            parameters: Vec::with_capacity(identifiers.len()),
            ..LexicalScope::default()
        });
        let result = (|| {
            let mut order = vec![None; identifiers.len()];
            let mut retained = self.checked.as_ref().map(|_| Retained {
                declarations: Vec::new(),
                parameters: Vec::new(),
            });
            let mut sites = self.checked.as_ref().map(|_| vec![None; identifiers.len()]);
            for declaration in &definition.node.declarations {
                if self.gnu_sync_profile()
                    && declaration
                        .node
                        .specifiers
                        .first()
                        .is_some_and(|specifier| {
                            matches!(specifier.node, ast::DeclarationSpecifier::Extension(_))
                        })
                {
                    return Err(Error::new(
                        declaration.span.start,
                        "GNU identifier-list parameter declarations cannot start with attributes",
                    ));
                }
                if declaration.node.declarators.is_empty() {
                    return Err(Error::new(
                        declaration.span.start,
                        "an old-style parameter declaration must declare a parameter",
                    ));
                }
                let checkpoint = self
                    .checked
                    .as_ref()
                    .map(|checked| checked.declaration_checkpoint());
                let prepared = self.specifiers(&declaration.node.specifiers)?;
                for item in &declaration.node.declarators {
                    let name = declarator_name(&item.node.declarator).ok_or_else(|| {
                        Error::new(
                            item.span.start,
                            "old-style parameter requires an identifier",
                        )
                    })?;
                    let index = *indices.get(name).ok_or_else(|| {
                        Error::new(
                            item.span.start,
                            "declaration names a parameter absent from the identifier list",
                        )
                    })?;
                    if item.node.initializer.is_some() {
                        return Err(Error::new(
                            item.span.start,
                            "function parameters cannot have initializers",
                        ));
                    }
                    if order[index].is_some() {
                        return Err(Error::new(
                            item.span.start,
                            "duplicate parameter declaration",
                        ));
                    }
                    let declaration_index = self
                        .lexical_scopes
                        .last()
                        .expect("parameter scope")
                        .parameters
                        .len();
                    let site = self.check_parameter(
                        ParameterSyntax::OldStyle { declaration, item },
                        Some(&prepared),
                    )?;
                    let parameter = &self
                        .lexical_scopes
                        .last()
                        .expect("parameter scope")
                        .parameters[declaration_index];
                    if self.unit.is_sizeless(&parameter.ty)? {
                        return Err(Error::new(
                            item.span.start,
                            "SVE value definitions require unsupported target-feature configuration",
                        ));
                    }
                    if !self.is_complete_object(&parameter.ty, 0)? {
                        return Err(Error::new(
                            item.span.start,
                            "function definition parameter requires a complete object type",
                        ));
                    }
                    order[index] = Some(declaration_index);
                    if let Some(sites) = &mut sites {
                        sites[index] = site;
                    }
                }
                if let (Some(checked), Some(start), Some(retained)) =
                    (&mut self.checked, checkpoint, &mut retained)
                {
                    checked.complete_declaration_group(declaration, start)?;
                    retained
                        .declarations
                        .push(checked.declaration_group(declaration)?);
                }
            }
            for (identifier, index) in identifiers.iter().zip(&order) {
                if index.is_none() {
                    return Err(Error::new(
                        identifier.span.start,
                        "C11 requires a declaration for every identifier-list parameter",
                    ));
                }
            }
            let scope = self.lexical_scopes.last_mut().expect("parameter scope");
            let mut source_order: Vec<_> = std::mem::take(&mut scope.parameters)
                .into_iter()
                .map(Some)
                .collect();
            for (identifier, index) in identifiers.iter().zip(order) {
                let parameter = source_order[index.expect("all parameters declared")]
                    .take()
                    .expect("unique parameter");
                scope
                    .names
                    .insert(identifier.node.name.clone(), Some(scope.parameters.len()));
                scope.parameters.push(parameter);
            }
            let mut incoming = Vec::with_capacity(identifiers.len());
            let mut bytes = self.old_style_definitions.bytes;
            for parameter in &self
                .lexical_scopes
                .last()
                .expect("parameter scope")
                .parameters
            {
                charge_type(self.unit.resolve(&parameter.ty)?, &mut bytes, span.start, 0)?;
                incoming.push(self.old_style_argument_type(&parameter.ty, span.start)?);
            }
            if let (Some(checked), Some(sites), Some(retained)) =
                (&mut self.checked, sites, &mut retained)
            {
                for (identifier, site) in identifiers.iter().zip(sites) {
                    let occurrence = checked
                        .find(OccurrenceKind::OldStyleParameter, identifier)?
                        .ok_or_else(|| {
                            Error::new(
                                identifier.span.start,
                                "old-style parameter has no retained occurrence",
                            )
                        })?;
                    retained.parameters.push((
                        occurrence,
                        site.ok_or_else(|| {
                            Error::new(
                                identifier.span.start,
                                "old-style parameter has no retained declaration",
                            )
                        })?,
                    ));
                }
            }
            Ok(Signature { incoming, retained })
        })();
        self.leave_prototype();
        let signature = result?;
        self.function_scope
            .as_mut()
            .ok_or_else(|| Error::new(span.start, "old-style parameter scope was not captured"))?
            .old_style = Some(signature);
        Ok(checked_scope)
    }

    fn old_style_argument_type(&self, ty: &Type, offset: usize) -> Result<Type, Error> {
        let resolved = self.unit.resolve(ty)?;
        let kind = match &resolved.kind {
            TypeKind::Atomic(value) if self.gnu_sync_profile() => {
                TypeKind::Atomic(Box::new(self.old_style_argument_type(value, offset)?))
            }
            TypeKind::Bool | TypeKind::Integer(_) | TypeKind::Enum(_) => {
                return Ok(integer_to_type(promote(self.integer_type(ty, offset)?)));
            }
            TypeKind::Float(FloatKind::Float) => TypeKind::Float(FloatKind::Double),
            kind => kind.clone(),
        };
        Ok(Type {
            kind,
            qualifiers: Qualifiers::default(),
            alignment: resolved.alignment,
        })
    }
}

fn declarator_name(mut declarator: &Node<ast::Declarator>) -> Option<&str> {
    loop {
        match &declarator.node.kind.node {
            ast::DeclaratorKind::Identifier(identifier) => return Some(&identifier.node.name),
            ast::DeclaratorKind::Declarator(inner) => declarator = inner,
            ast::DeclaratorKind::Abstract => return None,
        }
    }
}

impl Analyzer {
    /// Reconcile an identifier-list definition with an earlier visible prototype.
    pub(crate) fn prepare_old_style_definition(
        &mut self,
        name: &str,
        ty: &mut Type,
        prior: Option<usize>,
        offset: usize,
    ) -> Result<(), Error> {
        let Some(scope) = &self.function_scope else {
            return Ok(());
        };
        let Some(old_style) = &scope.old_style else {
            return Ok(());
        };
        let previous = prior
            .map(|index| &self.unit.declarations[index].ty)
            .or_else(|| {
                (self.unit.compiler == toucan_target::Compiler::Clang)
                    .then(|| {
                        self.block_externs
                            .get(name)
                            .map(|declaration| &declaration.ty)
                    })
                    .flatten()
            });
        let Some(previous) = previous else {
            return Ok(());
        };
        let TypeKind::Function(previous) = &self.unit.resolve(previous)?.kind else {
            return Ok(());
        };
        if !previous.prototype {
            return Ok(());
        }
        if previous.parameters.len() != old_style.incoming.len() {
            return Err(Error::new(
                offset,
                "identifier-list definition parameter count conflicts with its prototype",
            ));
        }
        for ((parameter, incoming), local) in previous
            .parameters
            .iter()
            .zip(&old_style.incoming)
            .zip(&scope.parameters)
        {
            if !self.old_style_parameter_compatible(&parameter.ty, incoming)?
                && !self.old_style_parameter_compatible(&parameter.ty, &local.ty)?
            {
                return Err(Error::new(
                    offset,
                    "identifier-list definition parameter type conflicts with its prototype",
                ));
            }
        }
        // The established prototype controls the calling interface, including
        // GNU's raw char/float and variadic-definition extensions.
        let mut bytes = self.old_style_definitions.bytes;
        charge_type(&previous.return_type, &mut bytes, offset, 0)?;
        for parameter in &previous.parameters {
            charge_type(self.unit.resolve(&parameter.ty)?, &mut bytes, offset, 0)?;
        }
        let prototype = (**previous).clone();
        let TypeKind::Function(current) = &mut ty.kind else {
            return Ok(());
        };
        current.parameters = prototype.parameters;
        current.prototype = true;
        current.variadic = prototype.variadic;
        let incoming = current
            .parameters
            .iter()
            .map(|parameter| {
                let mut ty = self.unit.resolve(&parameter.ty)?.clone();
                ty.qualifiers = Qualifiers::default();
                Ok(ty)
            })
            .collect::<Result<Vec<_>, Error>>()?;
        self.function_scope
            .as_mut()
            .expect("definition scope")
            .old_style
            .as_mut()
            .expect("old-style scope")
            .incoming = incoming;
        Ok(())
    }

    fn old_style_parameter_compatible(&self, left: &Type, right: &Type) -> Result<bool, Error> {
        let mut left = self.unit.resolve(left)?.clone();
        let mut right = self.unit.resolve(right)?.clone();
        left.qualifiers = Qualifiers::default();
        right.qualifiers = Qualifiers::default();
        self.compatible(&left, &right)
    }

    pub(crate) fn has_old_style_definition(&self, index: usize) -> bool {
        self.old_style_definitions.incoming.contains_key(&index)
    }

    pub(crate) fn check_old_style_redeclaration(
        &self,
        index: usize,
        ty: &Type,
        offset: usize,
    ) -> Result<(), Error> {
        let Some(incoming) = self.old_style_definitions.incoming.get(&index) else {
            return Ok(());
        };
        let TypeKind::Function(function) = &self.unit.resolve(ty)?.kind else {
            return Ok(());
        };
        if !function.prototype {
            return Ok(());
        }
        if function.parameters.len() != incoming.len() {
            return Err(Error::new(
                offset,
                "prototype parameter count conflicts with an earlier identifier-list definition",
            ));
        }
        for (parameter, incoming) in function.parameters.iter().zip(incoming) {
            if !self.old_style_parameter_compatible(&parameter.ty, incoming)? {
                return Err(Error::new(
                    offset,
                    "prototype parameter type conflicts with an earlier identifier-list definition",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn save_old_style_definition(
        &mut self,
        index: usize,
        offset: usize,
    ) -> Result<(), Error> {
        let Some(signature) = self
            .function_scope
            .as_ref()
            .and_then(|scope| scope.old_style.as_ref())
        else {
            return Ok(());
        };
        let mut bytes = self.old_style_definitions.bytes;
        for ty in &signature.incoming {
            charge_type(ty, &mut bytes, offset, 0)?;
        }
        self.old_style_definitions.bytes = bytes;
        self.old_style_definitions
            .incoming
            .insert(index, signature.incoming.clone());
        Ok(())
    }
}

fn charge_type(ty: &Type, bytes: &mut usize, offset: usize, depth: usize) -> Result<(), Error> {
    if depth >= 128 {
        return Err(Error::new(
            offset,
            "old-style parameter type exceeds the 128-level limit",
        ));
    }
    *bytes = bytes.saturating_add(std::mem::size_of::<Type>());
    if let TypeKind::Typedef(name) = &ty.kind {
        *bytes = bytes.saturating_add(name.len());
    }
    if *bytes > MAX_METADATA_BYTES {
        return Err(Error::new(
            offset,
            "old-style definition metadata exceeds the 64 MiB limit",
        ));
    }
    match &ty.kind {
        TypeKind::Atomic(inner)
        | TypeKind::Pointer(inner)
        | TypeKind::Vector { element: inner, .. }
        | TypeKind::Array { element: inner, .. }
        | TypeKind::VariableArray { element: inner, .. } => {
            charge_type(inner, bytes, offset, depth + 1)?
        }
        TypeKind::Function(function) => {
            *bytes = bytes.saturating_add(std::mem::size_of::<crate::FunctionType>());
            charge_type(&function.return_type, bytes, offset, depth + 1)?;
            for parameter in &function.parameters {
                *bytes = bytes.saturating_add(
                    std::mem::size_of::<crate::Parameter>()
                        + parameter.name.as_ref().map_or(0, String::len),
                );
                charge_type(&parameter.ty, bytes, offset, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

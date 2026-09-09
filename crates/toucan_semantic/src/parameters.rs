//! Borrowed parameter syntax shared by prototypes and identifier-list definitions.

use crate::checked::{Builder, LocalDeclaration, OccurrenceKind, SiteId, TypeUseId};
use crate::{Error, TypeKind};
use lang_c::{
    ast,
    span::{Node, Span},
};

#[derive(Clone, Copy)]
pub(crate) enum ParameterSyntax<'a> {
    Prototype(&'a Node<ast::ParameterDeclaration>),
    OldStyle {
        declaration: &'a Node<ast::Declaration>,
        item: &'a Node<ast::InitDeclarator>,
    },
}

impl<'a> ParameterSyntax<'a> {
    pub(crate) fn span(self) -> Span {
        match self {
            Self::Prototype(parameter) => parameter.span,
            Self::OldStyle { item, .. } => item.span,
        }
    }
    pub(crate) fn specifiers(self) -> &'a [Node<ast::DeclarationSpecifier>] {
        match self {
            Self::Prototype(parameter) => &parameter.node.specifiers,
            Self::OldStyle { declaration, .. } => &declaration.node.specifiers,
        }
    }
    pub(crate) fn declarator(self) -> Option<&'a Node<ast::Declarator>> {
        match self {
            Self::Prototype(parameter) => parameter.node.declarator.as_ref(),
            Self::OldStyle { item, .. } => Some(&item.node.declarator),
        }
    }
    pub(crate) fn extensions(self) -> &'a [Node<ast::Extension>] {
        match self {
            Self::Prototype(parameter) => &parameter.node.extensions,
            Self::OldStyle { .. } => &[],
        }
    }
    pub(crate) fn retain_written_type(
        self,
        checked: &mut Builder,
        id: TypeUseId,
        resolved: &TypeKind,
    ) -> Result<(), Error> {
        match self {
            Self::Prototype(parameter) => {
                checked.parameter_type_use(parameter, OccurrenceKind::Parameter, id, resolved)
            }
            Self::OldStyle { item, .. } => {
                checked.parameter_type_use(item, OccurrenceKind::InitDeclarator, id, resolved)
            }
        }
    }
    pub(crate) fn retain_declaration(
        self,
        checked: &mut Builder,
        declaration: LocalDeclaration<'_>,
    ) -> Result<Option<SiteId>, Error> {
        match self {
            Self::Prototype(parameter) => {
                checked.local_declaration(parameter, OccurrenceKind::Parameter, declaration)
            }
            Self::OldStyle { item, .. } => {
                checked.local_declaration(item, OccurrenceKind::InitDeclarator, declaration)
            }
        }
    }
}

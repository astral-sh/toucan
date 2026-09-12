//! Lexical record containment without introducing a C tag namespace.

use std::collections::BTreeMap;

use lang_c::ast;
use serde::Serialize;

use crate::analyze::{Analyzer, Tag};
use crate::{Error, Scope, TypeKind};

/// A tag's definition location, or its first declaration while incomplete.
#[derive(Clone, Debug, Serialize)]
pub struct TagLexicalOrigin {
    /// Nearest lexically enclosing record definition, in the same unit.
    /// This does not change the tag's C scope or canonical identity.
    pub record: Option<usize>,
    /// Relative order in the parser input. This is not a diagnostic source offset.
    pub order: usize,
    /// A file-scope declaration outside a record preceded this definition.
    /// Later references to a completed tag do not change this fact.
    pub prior_file_declaration: bool,
    /// First direct typedef naming this anonymous definition. A later typeof
    /// alias is a separate declaration and does not name the tag definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typedef_declaration: Option<usize>,
}

/// Sparse origins for nested and anonymous tags. Ordinary named file tags need
/// no entry. Record and enum origins share the same source-order coordinates.
#[derive(Clone, Debug, Default, Serialize)]
pub struct TagLexicalOrigins {
    pub records: BTreeMap<usize, TagLexicalOrigin>,
    pub enums: BTreeMap<usize, TagLexicalOrigin>,
}

impl TagLexicalOrigins {
    pub fn is_empty(&self) -> bool {
        self.records.is_empty() && self.enums.is_empty()
    }
}

impl<'ast> Analyzer<'ast> {
    /// Retain lexical nesting separately from prior standalone declarations.
    pub(crate) fn note_lexical_tag(
        &mut self,
        tag: Tag,
        introduced: bool,
        definition: bool,
        complete: bool,
        anonymous: bool,
        order: usize,
    ) -> Result<(), Error> {
        if definition && self.evaluation.is_enum_expression() && self.scope() == Scope::File {
            self.needs_tag_discovery = true;
        }
        if complete {
            return Ok(());
        }
        let file_scope = self.scope() == Scope::File;
        let count = self.unit.lexical_tags.records.len() + self.unit.lexical_tags.enums.len();
        let entries = match tag {
            Tag::Record(_) => &mut self.unit.lexical_tags.records,
            Tag::Enum(_) => &mut self.unit.lexical_tags.enums,
        };
        let id = match tag {
            Tag::Record(id) | Tag::Enum(id) => id,
        };
        if !definition && !introduced {
            return Ok(());
        }
        if self.lexical_record.is_none() && !anonymous {
            entries.remove(&id);
            return Ok(());
        }
        let prior_file_declaration = file_scope
            && !introduced
            && entries
                .get(&id)
                .is_none_or(|origin| origin.prior_file_declaration);
        if !entries.contains_key(&id) && count >= 1_000_000 {
            return Err(Error::new(
                order,
                "lexical tag origins exceed the 1000000-entry limit",
            ));
        }
        entries.insert(
            id,
            TagLexicalOrigin {
                record: self.lexical_record,
                order,
                prior_file_declaration,
                typedef_declaration: None,
            },
        );
        Ok(())
    }

    /// A standalone redeclaration can expose an incomplete nested tag at file
    /// scope. Ordinary uses in objects, typedefs, or prototypes do not do this.
    pub(crate) fn note_standalone_lexical_tag(&mut self, kind: &TypeKind) {
        if self.lexical_record.is_some() || self.scope() != Scope::File {
            return;
        }
        let origin = match kind {
            TypeKind::Record(id) if self.unit.records[*id].fields.is_none() => {
                self.unit.lexical_tags.records.get_mut(id)
            }
            TypeKind::Enum(id) if !self.unit.enums[*id].complete => {
                self.unit.lexical_tags.enums.get_mut(id)
            }
            _ => None,
        };
        if let Some(origin) = origin {
            origin.prior_file_declaration = true;
        }
    }

    pub(crate) fn note_typedef_lexical_tag(
        &mut self,
        declaration: usize,
        specifiers: &[lang_c::span::Node<ast::DeclarationSpecifier>],
    ) -> Result<(), Error> {
        let order = specifiers.iter().find_map(|specifier| {
            let ast::DeclarationSpecifier::TypeSpecifier(ty) = &specifier.node else {
                return None;
            };
            match &ty.node {
                ast::TypeSpecifier::Struct(tag)
                    if tag.get(self.arena).node.identifier.is_none()
                        && tag.get(self.arena).node.declarations.is_some() =>
                {
                    let tag = tag.get(self.arena);
                    Some(tag.span.start)
                }
                ast::TypeSpecifier::Enum(tag)
                    if tag.get(self.arena).node.identifier.is_none()
                        && !tag.get(self.arena).node.enumerators.is_empty() =>
                {
                    let tag = tag.get(self.arena);
                    Some(tag.span.start)
                }
                _ => None,
            }
        });
        let Some(order) = order else { return Ok(()) };
        let ty = self.unit.resolve(&self.unit.declarations[declaration].ty)?;
        let origin = match ty.kind {
            TypeKind::Record(id) => {
                let id = self.unit.record_origin(id)?;
                self.unit.lexical_tags.records.get_mut(&id)
            }
            TypeKind::Enum(id) => self.unit.lexical_tags.enums.get_mut(&id),
            _ => None,
        };
        if let Some(origin) = origin
            && origin.order == order
        {
            origin.typedef_declaration.get_or_insert(declaration);
        }
        Ok(())
    }
}

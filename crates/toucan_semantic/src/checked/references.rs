//! Written type references and record-member declaration identities.

use std::collections::HashSet;

use lang_c::{
    ast,
    span::{Node, Span},
};
use serde::Serialize;

use super::{
    Builder, EntityId, EntityKey, EntityKind, Linkage, OccurrenceKind, ScopeId, SiteProperties,
    SourceSpan, Storage, declarator_name_span, map_span, unmapped_span,
};
use crate::analyze::Analyzer;
use crate::{Error, Field, Type, TypeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum ReferenceKind {
    Typedef,
    Tag,
    Field,
}

#[derive(Debug, Serialize)]
pub struct Reference {
    pub(crate) target: EntityId,
    pub(crate) scope: ScopeId,
    pub(crate) kind: ReferenceKind,
    pub(crate) source: SourceSpan,
}

#[derive(Default)]
pub(super) struct ReferenceBuilder {
    pub(super) standalone_tags: HashSet<(OccurrenceKind, usize, usize)>,
    spans: Vec<Span>,
    seen: HashSet<(EntityId, ScopeId, usize, usize)>,
}

impl Builder {
    pub(super) fn standalone_tag(
        &mut self,
        specifier: &Node<ast::TypeSpecifier>,
    ) -> Result<(), Error> {
        let (kind, span) = match &specifier.node {
            ast::TypeSpecifier::Struct(node) => (OccurrenceKind::Record, node.span),
            ast::TypeSpecifier::Enum(node) => (OccurrenceKind::Enum, node.span),
            _ => return Ok(()),
        };
        let key = (kind, span.start, span.end);
        if !self.reference_builder.standalone_tags.contains(&key) {
            self.budget.charge(0, 1, 0, span.start)?;
            self.reference_builder.standalone_tags.insert(key);
        }
        Ok(())
    }

    pub(crate) fn typedef_reference(&mut self, name: &Node<ast::Identifier>) -> Result<(), Error> {
        let entity = if let Some(entity) = self.entity_for_name(&name.node.name) {
            if self.code.entities[entity.index()].kind != EntityKind::Typedef {
                return Err(Error::new(
                    name.span.start,
                    "checked typedef resolves to a non-typedef entity",
                ));
            }
            entity
        } else {
            // Compiler-provided aliases have identity but no written declaration.
            self.entity(
                EntityKey::BuiltinTypedef(name.node.name.clone()),
                Some(&name.node.name),
                EntityKind::Typedef,
                name.span.start,
            )?
        };
        self.reference(entity, ReferenceKind::Typedef, name.span)
    }

    pub(super) fn reference(
        &mut self,
        entity: EntityId,
        kind: ReferenceKind,
        span: Span,
    ) -> Result<(), Error> {
        let key = (entity, self.current, span.start, span.end);
        if self.reference_builder.seen.contains(&key) {
            return Ok(());
        }
        self.budget.charge(1, 3, 0, span.start)?;
        self.reference_builder.seen.insert(key);
        self.reference_builder.spans.push(span);
        self.code.references.push(Reference {
            target: entity,
            scope: self.current,
            kind,
            source: unmapped_span(span),
        });
        Ok(())
    }

    pub(crate) fn member_declaration<T>(
        &mut self,
        node: &Node<T>,
        occurrence_kind: OccurrenceKind,
        record: usize,
        index: usize,
        field: &Field,
        name_span: Option<Span>,
    ) -> Result<Option<super::SiteId>, Error> {
        let Some(occurrence) = self.find(occurrence_kind, node)? else {
            return Ok(None);
        };
        let entity = self.entity(
            EntityKey::Field(record, index),
            field.name.as_deref(),
            EntityKind::Field { record, index },
            node.span.start,
        )?;
        let site = self.site(
            entity,
            occurrence,
            &field.ty,
            name_span,
            SiteProperties {
                storage: Storage::None,
                linkage: Linkage::None,
                register: false,
                definition: true,
            },
        )?;
        Ok(Some(site))
    }

    pub(super) fn finish_references(&mut self) -> Result<(), Error> {
        for (reference, span) in self
            .code
            .references
            .iter_mut()
            .zip(&self.reference_builder.spans)
        {
            reference.source = map_span(*span, &mut self.budget)?;
        }
        Ok(())
    }
}

pub(crate) fn member_name_span(declarator: &Node<ast::StructDeclarator>) -> Option<Span> {
    declarator
        .node
        .declarator
        .as_ref()
        .and_then(declarator_name_span)
}

impl Analyzer {
    /// Resolve the named end of an anonymous-member path while record identities are live.
    pub(crate) fn retain_member_reference(
        &mut self,
        ty: &Type,
        path: &[usize],
        name: &Node<ast::Identifier>,
    ) -> Result<Option<super::bounds::TypeUseId>, Error> {
        if self.checked.is_none() {
            return Ok(None);
        }
        let mut ty = ty;
        let mut target = None;
        for &index in path {
            let TypeKind::Record(record) = self.unit.resolve(ty)?.kind else {
                return Err(Error::new(
                    name.span.start,
                    "retained member path requires a record",
                ));
            };
            let member = self.unit.records[record]
                .fields
                .as_ref()
                .and_then(|fields| fields.get(index))
                .ok_or_else(|| {
                    Error::new(
                        name.span.start,
                        "retained member index is outside its record",
                    )
                })?;
            target = Some((self.unit.record_origin(record)?, index));
            ty = &member.ty;
        }
        let (record, index) =
            target.ok_or_else(|| Error::new(name.span.start, "retained member path is empty"))?;
        let checked = self.checked.as_mut().unwrap();
        let entity = checked.entity(
            EntityKey::Field(record, index),
            Some(&name.node.name),
            EntityKind::Field { record, index },
            name.span.start,
        )?;
        checked.reference(entity, ReferenceKind::Field, name.span)?;
        Ok(checked.entity_type_use(entity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checked::{CheckedCode, Limits, ScopeKind};
    use toucan_target::Target;

    fn retained(source: &str, target: Target) -> CheckedCode {
        let (unit, code) =
            crate::analyze::analyze_inner(source, target, Some(Limits::default())).unwrap();
        assert_eq!(
            format!("{unit:?}"),
            format!("{:?}", crate::analyze(source, target).unwrap())
        );
        let code = code.unwrap();
        assert!(code.ambiguous_aliases.is_empty());
        code
    }

    #[test]
    fn typedef_references_follow_scope_and_members_use_a_separate_namespace() {
        let source = r#"
            typedef int T;
            struct S { int T; T value; };
            T outside;
            int f(T parameter) {
                typedef long T;
                T local = 1;
                { typedef short T; T nested = 2; }
                return (T)parameter + local;
            }
        "#;
        for target in Target::ALL {
            let code = retained(source, target);
            let aliases: Vec<_> = code
                .declarations
                .iter()
                .filter(|site| code.entities[site.entity.index()].kind == EntityKind::Typedef)
                .collect();
            assert_eq!(aliases.len(), 3);
            let references: Vec<_> = code
                .references
                .iter()
                .filter(|reference| reference.kind == ReferenceKind::Typedef)
                .collect();
            assert_eq!(references.len(), 6);
            for (reference, expected) in references.iter().zip([0, 0, 0, 1, 2, 1]) {
                assert_eq!(reference.target, aliases[expected].entity);
                assert_eq!(&source[reference.source.range.clone()], "T");
            }
            assert_eq!(
                code.scopes[references[2].scope.index()].kind,
                ScopeKind::Function
            );
            assert_ne!(references[3].scope, references[4].scope);
        }
    }

    #[test]
    fn tags_distinguish_declarations_from_uses_and_preserve_shadowing() {
        let source = r#"
            struct S;
            struct S;
            struct S { int x; };
            struct S *first;
            enum E { VALUE };
            enum E second;
            int f(struct S *p) {
                struct S;
                struct S *local;
                return p->x;
            }
        "#;
        for target in Target::ALL {
            let code = retained(source, target);
            let tags: Vec<_> = code
                .declarations
                .iter()
                .filter(|site| {
                    matches!(
                        code.entities[site.entity.index()].kind,
                        EntityKind::Record(_)
                    )
                })
                .collect();
            assert_eq!(tags.len(), 4);
            assert_eq!(tags[0].entity, tags[1].entity);
            assert_eq!(tags[1].entity, tags[2].entity);
            assert_ne!(tags[2].entity, tags[3].entity);
            let refs: Vec<_> = code
                .references
                .iter()
                .filter(|reference| reference.kind == ReferenceKind::Tag)
                .collect();
            assert_eq!(refs.len(), 4);
            assert_eq!(refs[0].target, tags[0].entity);
            assert!(matches!(
                code.entities[refs[1].target.index()].kind,
                EntityKind::Enum(_)
            ));
            assert_eq!(refs[2].target, tags[0].entity);
            assert_eq!(refs[3].target, tags[3].entity);
            for reference in refs {
                assert_eq!(
                    source[reference.source.range.clone()],
                    *code.entities[reference.target.index()]
                        .name
                        .as_ref()
                        .unwrap()
                );
            }
        }
    }

    #[test]
    fn member_declarations_and_references_resolve_anonymous_and_bitfield_paths() {
        let source = r#"
            struct S {
                struct { int value; };
                unsigned bits:3;
                unsigned :0;
            };
            int f(struct S *p) { return p->value + p->bits + __builtin_offsetof(struct S, value); }
        "#;
        for target in Target::ALL {
            let code = retained(source, target);
            let members: Vec<_> = code
                .declarations
                .iter()
                .filter(|site| {
                    matches!(
                        code.entities[site.entity.index()].kind,
                        EntityKind::Field { .. }
                    )
                })
                .collect();
            assert_eq!(members.len(), 4);
            assert_eq!(
                members
                    .iter()
                    .filter(|site| site.name_source.is_none())
                    .count(),
                2
            );
            let refs: Vec<_> = code
                .references
                .iter()
                .filter(|reference| reference.kind == ReferenceKind::Field)
                .collect();
            assert_eq!(refs.len(), 3);
            assert_eq!(refs[0].target, refs[2].target);
            assert_ne!(refs[0].target, refs[1].target);
            for reference in refs {
                let declaration = members
                    .iter()
                    .find(|site| site.entity == reference.target)
                    .unwrap();
                assert_eq!(
                    source[reference.source.range.clone()],
                    source[declaration.name_source.as_ref().unwrap().range.clone()]
                );
            }
        }
    }

    #[test]
    fn initializer_designators_reference_members_through_array_and_anonymous_paths() {
        let source = "struct S { struct { int value; }; }; struct S a[4] = { [2].value = 3, [0] = { .value = 4 } };";
        for target in Target::ALL {
            let code = retained(source, target);
            let references: Vec<_> = code
                .references
                .iter()
                .filter(|reference| reference.kind == ReferenceKind::Field)
                .collect();
            assert_eq!(references.len(), 2);
            assert_eq!(references[0].target, references[1].target);
            let site = code
                .declarations
                .iter()
                .find(|site| site.entity == references[0].target)
                .unwrap();
            for reference in references {
                assert_eq!(&source[reference.source.range.clone()], "value");
                assert_eq!(
                    source[reference.source.range.clone()],
                    source[site.name_source.as_ref().unwrap().range.clone()]
                );
            }
        }
    }

    #[test]
    fn builtin_aliases_have_identity_without_a_written_declaration() {
        let source = "__builtin_va_list first; __builtin_va_list second;";
        for target in Target::ALL {
            let code = retained(source, target);
            let refs: Vec<_> = code
                .references
                .iter()
                .filter(|reference| reference.kind == ReferenceKind::Typedef)
                .collect();
            assert_eq!(refs.len(), 2);
            assert_eq!(refs[0].target, refs[1].target);
            assert!(
                code.declarations
                    .iter()
                    .all(|site| site.entity != refs[0].target)
            );
            assert_eq!(
                code.entities[refs[0].target.index()].kind,
                EntityKind::Typedef
            );
        }
    }

    #[test]
    fn references_keep_original_tokens_after_attribute_reordering_and_obey_quotas() {
        let source = "typedef int T; struct __attribute__((packed)) S { T field; }; T f(struct S *p) { return p->field; }";
        let code = retained(source, Target::X86_64UnknownLinuxGnu);
        for reference in &code.references {
            assert!(!reference.source.synthetic);
            assert_eq!(
                source[reference.source.range.clone()],
                *code.entities[reference.target.index()]
                    .name
                    .as_ref()
                    .unwrap()
            );
        }
        for limits in [
            Limits {
                nodes: 0,
                ..Limits::default()
            },
            Limits {
                edges: 0,
                ..Limits::default()
            },
            Limits {
                payload_bytes: 0,
                ..Limits::default()
            },
        ] {
            let error =
                crate::analyze::analyze_inner(source, Target::X86_64UnknownLinuxGnu, Some(limits))
                    .unwrap_err();
            assert!(error.to_string().contains("retention"), "{error}");
        }
    }
}

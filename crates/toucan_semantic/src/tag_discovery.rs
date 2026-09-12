//! Header cursor discovery, independent of C scope and lexical containment.

use std::collections::{BTreeMap, BTreeSet};

use lang_c::{ast, span::Span, visit::Visit};
use serde::Serialize;

use crate::{DeclarationKind, Error, Scope, TranslationUnit, Type, TypeKind};

/// First header-cursor discovery when it differs from C lexical containment.
#[derive(Clone, Debug, Serialize)]
pub enum TagDiscovery {
    /// Enum values and enum attributes do not expose nested declaration cursors.
    Hidden,
    /// A declaration type or a visited record exposes this tag.
    Discovered {
        /// Record that supplies the generated naming context, if any.
        record: Option<usize>,
        /// Relative parser-input order of the discovering declaration.
        order: usize,
        /// Source offset of the discovering declaration, before parser rewriting.
        offset: usize,
    },
}

/// Sparse facts for enum-cursor descendants and trailing record attributes.
/// These do not change C lookup, type identity, layout, or constant evaluation.
#[derive(Clone, Debug, Default, Serialize)]
pub struct TagDiscoveries {
    pub records: BTreeMap<usize, TagDiscovery>,
    pub enums: BTreeMap<usize, TagDiscovery>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Tag {
    Record(usize),
    Enum(usize),
}

#[derive(Clone, Copy)]
struct Event {
    tag: Tag,
    parent: Option<Tag>,
    naming_record: Option<usize>,
    offset: usize,
}

struct Scanner<'a> {
    unit: &'a TranslationUnit,
    parent: Option<Tag>,
    events: Vec<Event>,
    definitions: BTreeMap<Tag, Option<Tag>>,
    in_record_attribute: bool,
    attribute_tags: BTreeSet<Tag>,
    declarations: BTreeMap<&'a str, &'a Type>,
    named_records: BTreeMap<&'a str, usize>,
    named_enums: BTreeMap<&'a str, usize>,
    anonymous_records: BTreeMap<usize, usize>,
    anonymous_enums: BTreeMap<usize, usize>,
    remaining_type_work: usize,
    error: Option<Error>,
}

impl Scanner<'_> {
    fn push(&mut self, tag: Tag, offset: usize) {
        self.push_named(
            tag,
            offset,
            match self.parent {
                Some(Tag::Record(id)) => Some(id),
                _ => None,
            },
        );
    }

    fn push_named(&mut self, tag: Tag, offset: usize, naming_record: Option<usize>) {
        if self.events.len() >= 1_000_000 {
            self.error.get_or_insert_with(|| {
                Error::new(offset, "tag discovery exceeds the 1000000-event limit")
            });
            return;
        }
        self.events.push(Event {
            tag,
            parent: self.parent,
            naming_record,
            offset,
        });
    }

    fn record_id(&self, declaration: &ast::StructType, span: &Span) -> Option<usize> {
        match &declaration.identifier {
            Some(name) => self.named_records.get(name.node.name.as_str()).copied(),
            None => self.anonymous_records.get(&span.start).copied(),
        }
    }

    fn enum_id(&self, declaration: &ast::EnumType, span: &Span) -> Option<usize> {
        match &declaration.identifier {
            Some(name) => self.named_enums.get(name.node.name.as_str()).copied(),
            None => self.anonymous_enums.get(&span.start).copied(),
        }
    }

    fn type_edges(&mut self, ty: &Type, offset: usize, depth: usize) {
        self.type_edges_at(
            ty,
            offset,
            depth,
            match self.parent {
                Some(Tag::Record(id)) => Some(id),
                _ => None,
            },
        );
    }

    fn type_edges_at(
        &mut self,
        ty: &Type,
        offset: usize,
        depth: usize,
        naming_record: Option<usize>,
    ) {
        if self.error.is_some() {
            return;
        }
        if self.remaining_type_work == 0 {
            self.error = Some(Error::new(
                offset,
                "tag discovery exceeds the 1000000-step type traversal limit",
            ));
            return;
        }
        self.remaining_type_work -= 1;
        if depth >= 128 {
            self.error = Some(Error::new(
                offset,
                "tag discovery type nesting exceeds the 128-level limit",
            ));
            return;
        }
        let ty = match self.unit.resolve(ty) {
            Ok(ty) => ty,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        match &ty.kind {
            TypeKind::Record(id) if self.unit.records[*id].scope == Scope::File => {
                match self.unit.record_origin(*id) {
                    Ok(id) => self.push_named(Tag::Record(id), offset, naming_record),
                    Err(error) => self.error = Some(error),
                }
            }
            TypeKind::Enum(id) if self.unit.enums[*id].scope == Scope::File => {
                self.push_named(Tag::Enum(*id), offset, naming_record)
            }
            TypeKind::Pointer(ty)
            | TypeKind::Array { element: ty, .. }
            | TypeKind::VariableArray { element: ty, .. } => {
                self.type_edges_at(ty, offset, depth + 1, None)
            }
            TypeKind::Atomic(ty) => self.type_edges_at(ty, offset, depth + 1, naming_record),
            TypeKind::Function(function) => {
                self.type_edges_at(&function.return_type, offset, depth + 1, None);
                for parameter in &function.parameters {
                    self.type_edges_at(&parameter.ty, offset, depth + 1, None);
                }
            }
            _ => {}
        }
    }

    fn definition_tag(&self, ty: &ast::TypeSpecifier, arena: &lang_c::arena::Arena) -> Option<Tag> {
        match ty {
            ast::TypeSpecifier::Struct(tag) if tag.get(arena).node.declarations.is_some() => {
                let tag = tag.get(arena);
                self.record_id(&tag.node, &tag.span).map(Tag::Record)
            }
            ast::TypeSpecifier::Enum(tag) if !tag.get(arena).node.enumerators.is_empty() => {
                let tag = tag.get(arena);
                self.enum_id(&tag.node, &tag.span).map(Tag::Enum)
            }
            _ => None,
        }
    }

    fn declaration_specifiers(
        &mut self,
        specifiers: &[lang_c::span::Node<ast::DeclarationSpecifier>],
        arena: &lang_c::arena::Arena,
    ) {
        let outer = self.parent;
        let outer_attribute = self.in_record_attribute;
        let mut trailing_owner = None;
        for specifier in specifiers {
            self.in_record_attribute = outer_attribute
                || (matches!(specifier.node, ast::DeclarationSpecifier::Extension(_))
                    && matches!(trailing_owner, Some(Tag::Record(_))));
            self.parent = if matches!(specifier.node, ast::DeclarationSpecifier::Extension(_)) {
                trailing_owner.or(outer)
            } else {
                outer
            };
            self.visit_declaration_specifier(&specifier.node, &specifier.span, arena);
            if let ast::DeclarationSpecifier::TypeSpecifier(ty) = &specifier.node {
                trailing_owner = self.definition_tag(&ty.node, arena);
            }
        }
        self.parent = outer;
        self.in_record_attribute = outer_attribute;
    }

    fn specifier_qualifiers(
        &mut self,
        specifiers: &[lang_c::span::Node<ast::SpecifierQualifier>],
        arena: &lang_c::arena::Arena,
    ) {
        let outer = self.parent;
        let outer_attribute = self.in_record_attribute;
        let mut trailing_owner = None;
        for specifier in specifiers {
            self.in_record_attribute = outer_attribute
                || (matches!(specifier.node, ast::SpecifierQualifier::Extension(_))
                    && matches!(trailing_owner, Some(Tag::Record(_))));
            self.parent = if matches!(specifier.node, ast::SpecifierQualifier::Extension(_)) {
                trailing_owner.or(outer)
            } else {
                outer
            };
            self.visit_specifier_qualifier(&specifier.node, &specifier.span, arena);
            if let ast::SpecifierQualifier::TypeSpecifier(ty) = &specifier.node {
                trailing_owner = self.definition_tag(&ty.node, arena);
            }
        }
        self.parent = outer;
        self.in_record_attribute = outer_attribute;
    }

    fn declaration_type(
        &mut self,
        declarator: &ast::Declarator,
        offset: usize,
        arena: &lang_c::arena::Arena,
    ) {
        fn name<'a>(
            declarator: &'a ast::Declarator,
            arena: &'a lang_c::arena::Arena,
        ) -> Option<&'a str> {
            match &declarator.kind.node {
                ast::DeclaratorKind::Identifier(name) => Some(&name.node.name),
                ast::DeclaratorKind::Declarator(inner) => {
                    let inner = inner.get(arena);
                    name(&inner.node, arena)
                }
                ast::DeclaratorKind::Abstract => None,
            }
        }
        if let Some(ty) = name(declarator, arena)
            .and_then(|name| self.declarations.get(name))
            .copied()
        {
            self.type_edges(ty, offset, 0);
        }
    }
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_declaration(
        &mut self,
        declaration: &'ast ast::Declaration,
        span: &'ast Span,
        arena: &'ast lang_c::arena::Arena,
    ) {
        self.declaration_specifiers(&declaration.specifiers, arena);
        for declarator in &declaration.declarators {
            self.visit_init_declarator(&declarator.node, &declarator.span, arena);
        }
        if declaration.declarators.is_empty() && self.parent.is_none() {
            for specifier in &declaration.specifiers {
                let ast::DeclarationSpecifier::TypeSpecifier(ty) = &specifier.node else {
                    continue;
                };
                match &ty.node {
                    ast::TypeSpecifier::Struct(tag)
                        if tag.get(arena).node.declarations.is_none() =>
                    {
                        let tag = tag.get(arena);
                        if let Some(id) = self.record_id(&tag.node, &tag.span) {
                            self.push(Tag::Record(id), span.start);
                        }
                    }
                    ast::TypeSpecifier::Enum(tag) if tag.get(arena).node.enumerators.is_empty() => {
                        let tag = tag.get(arena);
                        if let Some(id) = self.enum_id(&tag.node, &tag.span) {
                            self.push(Tag::Enum(id), span.start);
                        }
                    }
                    _ => {}
                }
            }
        }
        for declarator in &declaration.declarators {
            self.declaration_type(
                &declarator.node.declarator.node,
                declarator.span.start,
                arena,
            );
        }
    }

    fn visit_struct_type(
        &mut self,
        declaration: &'ast ast::StructType,
        span: &'ast Span,
        arena: &'ast lang_c::arena::Arena,
    ) {
        // Attributes between `struct` and the tag are visited before the
        // record cursor. Trailing attributes are handled by the specifier list.
        for extension in &declaration.extensions {
            self.visit_extension(&extension.node, &extension.span, arena);
        }
        let Some(declarations) = &declaration.declarations else {
            return;
        };
        let Some(id) = self.record_id(declaration, span) else {
            return;
        };
        let tag = Tag::Record(id);
        self.push(tag, span.start);
        self.definitions.insert(tag, self.parent);
        if self.in_record_attribute && self.error.is_none() {
            self.attribute_tags.insert(tag);
        }
        let previous = self.parent.replace(tag);
        let mut field_index = 0;
        for declaration in declarations {
            self.visit_struct_declaration(&declaration.node, &declaration.span, arena);
            let ast::StructDeclaration::Field(field) = &declaration.node else {
                continue;
            };
            let fields = self.unit.records[id].fields.as_deref().unwrap_or_default();
            if field.node.declarators.is_empty() {
                use crate::analyze::AnonymousRecordSpecifier;
                let anonymous = match crate::analyze::anonymous_record_specifier(
                    &field.node.specifiers,
                    self.unit.target,
                    arena,
                ) {
                    Some(
                        AnonymousRecordSpecifier::Direct | AnonymousRecordSpecifier::MicrosoftTag,
                    ) => true,
                    Some(AnonymousRecordSpecifier::MicrosoftTypedef(name)) => {
                        // Discovery visits file-scope records. Resolve the
                        // original alias, before written qualifiers: an atomic
                        // alias is ignored, while `_Atomic RecordAlias` is not.
                        match self.unit.typedefs.get(name).map(|ty| self.unit.resolve(ty)) {
                            Some(Ok(ty)) => matches!(ty.kind, TypeKind::Record(_)),
                            Some(Err(error)) => {
                                self.error = Some(error);
                                false
                            }
                            None => false,
                        }
                    }
                    None => false,
                };
                if anonymous {
                    if let Some(member) = fields.get(field_index) {
                        self.type_edges(&member.ty, field.span.start, 0);
                    }
                    field_index += 1;
                }
            } else {
                for declarator in &field.node.declarators {
                    if let Some(member) = fields.get(field_index) {
                        self.type_edges(&member.ty, declarator.span.start, 0);
                    }
                    field_index += 1;
                }
            }
        }
        self.parent = previous;
    }

    fn visit_enum_type(
        &mut self,
        declaration: &'ast ast::EnumType,
        span: &'ast Span,
        arena: &'ast lang_c::arena::Arena,
    ) {
        // Interior attributes precede the cursor; trailing attributes belong
        // to it and are visited by the surrounding specifier list.
        for extension in &declaration.extensions {
            self.visit_extension(&extension.node, &extension.span, arena);
        }
        if declaration.enumerators.is_empty() {
            return;
        }
        let Some(id) = self.enum_id(declaration, span) else {
            return;
        };
        let tag = Tag::Enum(id);
        self.push(tag, span.start);
        self.definitions.insert(tag, self.parent);
        if self.in_record_attribute && self.error.is_none() {
            self.attribute_tags.insert(tag);
        }
        let previous = self.parent.replace(tag);
        for enumerator in &declaration.enumerators {
            self.visit_enumerator(&enumerator.node, &enumerator.span, arena);
        }
        self.parent = previous;
    }

    fn visit_type_name(
        &mut self,
        name: &'ast ast::TypeName,
        _: &'ast Span,
        arena: &'ast lang_c::arena::Arena,
    ) {
        self.specifier_qualifiers(&name.specifiers, arena);
        if let Some(declarator) = &name.declarator {
            self.visit_declarator(&declarator.node, &declarator.span, arena);
        }
    }

    fn visit_struct_field(
        &mut self,
        field: &'ast ast::StructField,
        _: &'ast Span,
        arena: &'ast lang_c::arena::Arena,
    ) {
        self.specifier_qualifiers(&field.specifiers, arena);
        for declarator in &field.declarators {
            self.visit_struct_declarator(&declarator.node, &declarator.span, arena);
        }
    }

    fn visit_parameter_declaration(
        &mut self,
        _: &'ast ast::ParameterDeclaration,
        _: &'ast Span,
        _arena: &'ast lang_c::arena::Arena,
    ) {
    }
    fn visit_statement(
        &mut self,
        _: &'ast ast::Statement,
        _: &'ast Span,
        _arena: &'ast lang_c::arena::Arena,
    ) {
    }
    fn visit_function_definition(
        &mut self,
        definition: &'ast ast::FunctionDefinition,
        span: &'ast Span,
        arena: &'ast lang_c::arena::Arena,
    ) {
        self.declaration_specifiers(&definition.specifiers, arena);
        self.visit_declarator(
            &definition.declarator.node,
            &definition.declarator.span,
            arena,
        );
        self.declaration_type(&definition.declarator.node, span.start, arena);
    }
}

/// Build a bounded cursor graph for enum expressions or trailing record attributes.
pub(crate) fn discover(
    unit: &TranslationUnit,
    syntax: &ast::TranslationUnit,
    arena: &lang_c::arena::Arena,
) -> Result<Option<Box<TagDiscoveries>>, Error> {
    let mut scanner = Scanner {
        unit,
        parent: None,
        events: Vec::new(),
        definitions: BTreeMap::new(),
        in_record_attribute: false,
        attribute_tags: BTreeSet::new(),
        error: None,
        declarations: unit
            .declarations
            .iter()
            .filter(|item| {
                matches!(
                    item.kind,
                    DeclarationKind::Function
                        | DeclarationKind::Variable
                        | DeclarationKind::Typedef
                )
            })
            .map(|item| (item.name.as_str(), &item.ty))
            .collect(),
        named_records: unit
            .records
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, item)| item.scope == Scope::File)
            .filter_map(|(id, item)| item.name.as_deref().map(|name| (name, id)))
            .collect(),
        named_enums: unit
            .enums
            .iter()
            .enumerate()
            .filter(|(_, item)| item.scope == Scope::File)
            .filter_map(|(id, item)| item.name.as_deref().map(|name| (name, id)))
            .collect(),
        anonymous_records: unit
            .lexical_tags
            .records
            .iter()
            .filter(|(id, _)| {
                unit.records[**id].scope == Scope::File && unit.records[**id].name.is_none()
            })
            .map(|(&id, origin)| (origin.order, id))
            .collect(),
        anonymous_enums: unit
            .lexical_tags
            .enums
            .iter()
            .filter(|(id, _)| {
                unit.enums[**id].scope == Scope::File && unit.enums[**id].name.is_none()
            })
            .map(|(&id, origin)| (origin.order, id))
            .collect(),
        remaining_type_work: 1_000_000,
    };
    scanner.visit_translation_unit(syntax, arena);
    if let Some(error) = scanner.error {
        return Err(error);
    }
    let mut affected = scanner.attribute_tags;
    for &tag in scanner.definitions.keys() {
        let mut parent = scanner.definitions[&tag];
        for depth in 0..128 {
            match parent {
                Some(Tag::Enum(_)) => {
                    affected.insert(tag);
                    break;
                }
                Some(record @ Tag::Record(_)) => {
                    parent = scanner.definitions.get(&record).copied().flatten()
                }
                None => break,
            }
            if depth == 127 {
                return Err(Error::new(
                    0,
                    "tag discovery containment exceeds the 128-level limit",
                ));
            }
        }
    }
    if affected.is_empty() {
        return Ok(None);
    }
    let mut roots = Vec::new();
    let mut children = BTreeMap::<usize, Vec<Event>>::new();
    for event in scanner.events {
        match event.parent {
            None => roots.push(event),
            Some(Tag::Record(id)) => children.entry(id).or_default().push(event),
            Some(Tag::Enum(_)) => {}
        }
    }
    roots.sort_by_key(|event| event.offset);
    for events in children.values_mut() {
        events.sort_by_key(|event| event.offset);
    }
    let mut discovered = BTreeMap::new();
    fn visit(
        event: Event,
        children: &BTreeMap<usize, Vec<Event>>,
        discovered: &mut BTreeMap<Tag, Event>,
        depth: usize,
    ) -> Result<(), Error> {
        if discovered.contains_key(&event.tag) {
            return Ok(());
        }
        if depth >= 128 {
            return Err(Error::new(
                event.offset,
                "tag discovery traversal exceeds the 128-level limit",
            ));
        }
        discovered.insert(event.tag, event);
        if let Tag::Record(id) = event.tag
            && let Some(events) = children.get(&id)
        {
            for &event in events {
                visit(event, children, discovered, depth + 1)?;
            }
        }
        Ok(())
    }
    for event in roots {
        visit(event, &children, &mut discovered, 0)?;
    }
    let mut result = TagDiscoveries::default();
    for tag in affected {
        let value =
            discovered
                .get(&tag)
                .map_or(TagDiscovery::Hidden, |event| TagDiscovery::Discovered {
                    record: event.naming_record,
                    order: event.offset,
                    offset: event.offset,
                });
        match tag {
            Tag::Record(id) => result.records.insert(id, value),
            Tag::Enum(id) => result.enums.insert(id, value),
        };
    }
    Ok(Some(Box::new(result)))
}

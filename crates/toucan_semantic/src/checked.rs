//! Owned semantic facts retained by [`crate::analyze_with_options`].
//!
//! IDs belong to one [`crate::Analysis`]. They are stable for that owner's lifetime,
//! but must not be mixed between analyses. Lookups return `None` for out-of-range
//! IDs; an in-range ID from another owner cannot be detected. The owner exposes no
//! mutable references, so callers cannot invalidate graph links.
//!
//! Source ranges refer to the original preprocessed input. These nodes own their
//! payloads and remain usable after that input is dropped. This is a checked syntax
//! graph, not lowered code or a control-flow graph. In particular, written operand
//! and initializer order does not impose an order on C side effects.

mod access;
pub(crate) mod attributes;
pub(crate) mod bounds;
pub(crate) mod expression;
pub(crate) mod initializer;
mod ownership;
mod query;
pub(crate) mod references;
pub(crate) mod statement;

pub use crate::object_extent::{
    ObjectSizeFoldStage, ObjectSizeProof, ObjectSizeResult, ObjectSizeUnknown,
};
pub use crate::sync::SyncOperation;
pub use attributes::{DiagnosticAttribute, DiagnosticAttributeKind};
pub use bounds::{
    Bound, BoundEvaluation, BoundId, BoundInput, BoundSite, BoundValue, Extent, FunctionUse,
    TypeStep, TypeUse, TypeUseId,
};
pub use expression::{
    Binary, Builtin, Conversion, ConversionStep, Coverage as ExpressionStatus, ExprId, ExprKind,
    ExprUse, Expression, ExpressionCoverage, GenericArm, OffsetMember, Unary, UseContext,
    ValueCategory,
};
pub use initializer::{
    Coverage as InitializerStatus, Entry as InitializerEntry, Initializer, InitializerCoverage,
    InitializerKind, Subobject,
};
pub use ownership::{TypeOperand, TypeOperandEvaluation, TypeOperandId, TypeOperandInput};
pub use query::{QueryEvaluation, QuerySideEffects, QuerySuppression};
pub use references::{Reference, ReferenceKind};
pub use statement::{
    Assembly, AssemblyLocation, AssemblyOperand, AssemblyText, Assertion, AssertionId, BlockItem,
    BodyId, Coverage as StatementStatus, DeclarationGroup, DeclarationGroupId, ForInitializer,
    FunctionBody, Label, Statement, StatementCoverage, StatementId, StatementKind,
};

use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::ops::Range;

use lang_c::ast;
use lang_c::span::{Node, Span};
use lang_c::visit::{self, Visit};
use serde::Serialize;

use crate::parser_extensions::SourceMap;
use crate::{Declaration, DeclarationKind, Error, FlexibleArrayStorage, Type, TypeKind};

macro_rules! id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
        /// An owner-local ID. Obtain values from [`CheckedCode`] iteration or links.
        pub struct $name(u32);
        impl $name {
            /// Returns the owner-local arena index.
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}
id!(OccurrenceId);
id!(EntityId);
id!(SiteId);
id!(ScopeId);
id!(TypeId);
id!(InitializerId);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct AssignmentId(u32);
impl AssignmentId {
    /// Returns the owner-local arena index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Limits logical retained nodes, references and owned payload, including the
/// occurrence catalog and interned type trees. Allocator overhead is not counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum logical retained nodes, capped at `u32::MAX`.
    pub nodes: usize,
    /// Maximum logical references between retained nodes.
    pub edges: usize,
    /// Maximum charged owned payload bytes; excludes allocator overhead.
    pub payload_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 1_000_000,
            edges: 4_000_000,
            payload_bytes: 128 * 1024 * 1024,
        }
    }
}

struct Budget {
    limits: Limits,
    nodes: usize,
    edges: usize,
    payload_bytes: usize,
}

impl Budget {
    fn charge(
        &mut self,
        nodes: usize,
        edges: usize,
        bytes: usize,
        offset: usize,
    ) -> Result<(), Error> {
        for (used, amount, limit, resource) in [
            (
                &mut self.nodes,
                nodes,
                self.limits.nodes.min(u32::MAX as usize),
                "node",
            ),
            (&mut self.edges, edges, self.limits.edges, "edge"),
            (
                &mut self.payload_bytes,
                bytes,
                self.limits.payload_bytes,
                "payload byte",
            ),
        ] {
            let next = used
                .checked_add(amount)
                .filter(|next| *next <= limit)
                .ok_or_else(|| {
                    Error::new(
                        offset,
                        format!("checked-code retention {resource} limit exceeded"),
                    )
                })?;
            *used = next;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[non_exhaustive]
pub enum OccurrenceKind {
    Declaration,
    InitDeclarator,
    Declarator,
    Parameter,
    Field,
    StructDeclarator,
    Record,
    Enum,
    Enumerator,
    Function,
    TypeName,
    TypeOf,
    Expression,
    Initializer,
    InitializerItem,
    Designator,
    Statement,
    StaticAssert,
    Label,
    StringLiteral,
    AsmOperand,
}

#[derive(Debug, Serialize)]
pub struct SourceSpan {
    /// Covering range in the original preprocessed source.
    pub(crate) range: Range<usize>,
    /// Only populated when the original pieces are disjoint or reordered.
    pub(crate) fragments: Vec<Range<usize>>,
    /// True when the occurrence consists entirely of parser-inserted text.
    pub(crate) synthetic: bool,
}

#[derive(Debug, Serialize)]
pub struct Occurrence {
    /// Nearest written declaration, parameter, field, function, or type name.
    /// Type owners point to themselves. This is syntax ownership, not execution.
    pub(crate) type_owner: Option<OccurrenceId>,
    pub(crate) type_operands: Vec<ownership::TypeOperandId>,
    /// Attribute argument grammar also uses expression nodes for metadata such as `printf`.
    pub(crate) attribute_argument: bool,
    pub(crate) kind: OccurrenceKind,
    pub(crate) source: SourceSpan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum ScopeKind {
    File,
    Prototype,
    Function,
    Block,
}

#[derive(Debug, Serialize)]
pub struct Scope {
    pub(crate) parent: Option<ScopeId>,
    pub(crate) kind: ScopeKind,
    pub(crate) source: SourceSpan,
    pub(crate) declarations: Vec<SiteId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum EntityKind {
    Variable,
    Function,
    Typedef,
    Parameter,
    Record(usize),
    Enum(usize),
    Enumerator { enumeration: usize, variant: usize },
    Field { record: usize, index: usize },
}

impl From<DeclarationKind> for EntityKind {
    fn from(kind: DeclarationKind) -> Self {
        match kind {
            DeclarationKind::Variable => Self::Variable,
            DeclarationKind::Function => Self::Function,
            DeclarationKind::Typedef => Self::Typedef,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Entity {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) returns_twice: bool,
    #[serde(skip_serializing_if = "crate::SymbolBinding::is_strong")]
    pub(crate) symbol_binding: crate::SymbolBinding,
    pub(crate) body: Option<statement::BodyId>,
    pub(crate) name: Option<String>,
    pub(crate) kind: EntityKind,
    /// Canonical file declaration, when one exists. Block externs may precede it.
    pub(crate) declaration: Option<usize>,
    pub(crate) linkage: Linkage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum Storage {
    None,
    Automatic,
    Static,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum Linkage {
    None,
    Internal,
    External,
}

#[derive(Debug, Serialize)]
pub struct DeclarationSite {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) returns_twice: bool,
    #[serde(skip_serializing_if = "crate::SymbolBinding::is_strong")]
    pub(crate) symbol_binding: crate::SymbolBinding,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) weak_attribute: Option<SourceSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) returns_twice_attribute: Option<SourceSpan>,
    pub(crate) body: Option<statement::BodyId>,
    pub(crate) initializer: Option<InitializerId>,
    pub(crate) type_use: bounds::TypeUseId,
    pub(crate) declared_type_use: Option<bounds::TypeUseId>,
    pub(crate) name_source: Option<SourceSpan>,
    pub(crate) entity: EntityId,
    pub(crate) occurrence: OccurrenceId,
    pub(crate) scope: ScopeId,
    pub(crate) ty: TypeId,
    pub(crate) storage: Storage,
    pub(crate) linkage: Linkage,
    pub(crate) register: bool,
    pub(crate) definition: bool,
    pub(crate) flexible_array_storage: Option<FlexibleArrayStorage>,
}

struct SiteProperties {
    storage: Storage,
    linkage: Linkage,
    register: bool,
    definition: bool,
}

pub(crate) struct LocalDeclaration<'a> {
    pub(crate) name: Option<&'a str>,
    pub(crate) name_span: Option<Span>,
    pub(crate) ty: &'a Type,
    pub(crate) kind: EntityKind,
    pub(crate) storage: Storage,
    pub(crate) linked: bool,
    pub(crate) register: bool,
    pub(crate) definition: bool,
    pub(crate) allocation: Option<&'a FlexibleArrayStorage>,
}

/// Name tokens survive parenthesized declarators and parser attribute adapters.
pub(crate) fn declarator_name_span(mut declaration: &Node<ast::Declarator>) -> Option<Span> {
    loop {
        match &declaration.node.kind.node {
            ast::DeclaratorKind::Identifier(name) => return Some(name.span),
            ast::DeclaratorKind::Declarator(inner) => declaration = inner,
            ast::DeclaratorKind::Abstract => return None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CheckedCode {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) diagnostic_attributes: Vec<attributes::DiagnosticAttribute>,
    pub(crate) type_operands: Vec<ownership::TypeOperand>,
    pub(crate) statements: Vec<statement::Statement>,
    pub(crate) statement_coverage: Vec<statement::StatementCoverage>,
    pub(crate) bodies: Vec<statement::FunctionBody>,
    pub(crate) declaration_groups: Vec<statement::DeclarationGroup>,
    pub(crate) assertions: Vec<statement::Assertion>,
    pub(crate) references: Vec<references::Reference>,
    pub(crate) initializers: Vec<initializer::Initializer>,
    pub(crate) initializer_coverage: Vec<initializer::InitializerCoverage>,
    pub(crate) type_uses: Vec<bounds::TypeUse>,
    pub(crate) bounds: Vec<bounds::Bound>,
    pub(crate) expressions: Vec<expression::Expression>,
    pub(crate) assignment_conversions: Vec<expression::ExprUse>,
    pub(crate) expression_coverage: Vec<expression::ExpressionCoverage>,
    pub(crate) occurrences: Vec<Occurrence>,
    pub(crate) scopes: Vec<Scope>,
    pub(crate) entities: Vec<Entity>,
    pub(crate) declarations: Vec<DeclarationSite>,
    pub(crate) types: Vec<Type>,
    /// Cloned or synthesized checker nodes with a non-unique source occurrence.
    /// The foundation records these; later retention slices must carry IDs through
    /// those helpers rather than guess which original node was meant.
    pub(crate) ambiguous_aliases: Vec<SourceSpan>,
}

#[derive(Clone, Copy)]
enum Alias {
    Unique(OccurrenceId),
    Ambiguous,
}

#[derive(Hash, PartialEq, Eq)]
enum EntityKey {
    Linked(String),
    BuiltinTypedef(String),
    Field(usize, usize),
    Ordinary(ScopeId, String),
    Unnamed(ScopeId, OccurrenceId),
    Record(usize),
    Enum(usize),
    Enumerator(usize, usize),
}

/// Bound and typeof arenas before a constant query begins.
#[derive(Clone, Copy)]
pub(crate) struct EvaluationCheckpoint {
    bounds: usize,
    type_operands: usize,
}

pub(crate) struct Builder {
    statement_builder: statement::StatementBuilder,
    reference_builder: references::ReferenceBuilder,
    initializer_builder: initializer::InitializerBuilder,
    bounds_builder: bounds::BoundsBuilder,
    ownership_builder: ownership::OwnershipBuilder,
    expression_builder: expression::ExpressionBuilder,
    code: CheckedCode,
    budget: Budget,
    addresses: HashMap<(OccurrenceKind, usize, usize, usize), OccurrenceId>,
    aliases: HashMap<(OccurrenceKind, usize, usize), Alias>,
    parsed_spans: Vec<Span>,
    scope_spans: Vec<Span>,
    name_spans: Vec<Option<Span>>,
    ambiguous_spans: Vec<Span>,
    entities: HashMap<EntityKey, EntityId>,
    names: HashMap<ScopeId, HashMap<String, EntityId>>,
    type_hashes: HashMap<u64, Vec<TypeId>>,
    hasher: RandomState,
    current: ScopeId,
    /// A function's synthetic declaration is the same written definition.
    pub(crate) definition: Option<OccurrenceId>,
    error: Option<Error>,
    depth: usize,
    attribute_depth: usize,
}

impl Builder {
    pub(crate) fn new(
        unit: &ast::TranslationUnit,
        source_len: usize,
        limits: Limits,
    ) -> Result<Self, Error> {
        let mut builder = Self {
            reference_builder: references::ReferenceBuilder::default(),
            initializer_builder: initializer::InitializerBuilder::default(),
            bounds_builder: bounds::BoundsBuilder::default(),
            ownership_builder: ownership::OwnershipBuilder::default(),
            expression_builder: expression::ExpressionBuilder::default(),
            statement_builder: statement::StatementBuilder::default(),
            code: CheckedCode {
                diagnostic_attributes: Vec::new(),
                type_operands: Vec::new(),
                statements: Vec::new(),
                statement_coverage: Vec::new(),
                bodies: Vec::new(),
                declaration_groups: Vec::new(),
                assertions: Vec::new(),
                references: Vec::new(),
                initializers: Vec::new(),
                initializer_coverage: Vec::new(),
                type_uses: Vec::new(),
                bounds: Vec::new(),
                expressions: Vec::new(),
                assignment_conversions: Vec::new(),
                expression_coverage: Vec::new(),
                occurrences: Vec::new(),
                scopes: Vec::new(),
                entities: Vec::new(),
                declarations: Vec::new(),
                types: Vec::new(),
                ambiguous_aliases: Vec::new(),
            },
            budget: Budget {
                limits,
                nodes: 0,
                edges: 0,
                payload_bytes: 0,
            },
            addresses: HashMap::new(),
            aliases: HashMap::new(),
            parsed_spans: Vec::new(),
            scope_spans: Vec::new(),
            name_spans: Vec::new(),
            ambiguous_spans: Vec::new(),
            entities: HashMap::new(),
            names: HashMap::new(),
            type_hashes: HashMap::new(),
            hasher: RandomState::new(),
            current: ScopeId(0),
            definition: None,
            error: None,
            depth: 0,
            attribute_depth: 0,
        };
        builder.scope(ScopeKind::File, Span::span(0, source_len), None)?;
        builder.visit_translation_unit(unit);
        if let Some(error) = builder.error.take() {
            return Err(error);
        }
        Ok(builder)
    }

    fn occurrence<T>(
        &mut self,
        kind: OccurrenceKind,
        node: &T,
        span: Span,
    ) -> Result<OccurrenceId, Error> {
        // Reserve the type-owner edge even for occurrences without a type owner.
        self.budget.charge(1, 3, 0, span.start)?;
        let id = OccurrenceId(self.code.occurrences.len() as u32);
        self.code.occurrences.push(Occurrence {
            type_owner: self.ownership_builder.catalog_owner,
            type_operands: Vec::new(),
            attribute_argument: self.attribute_depth != 0,
            kind,
            source: unmapped_span(span),
        });
        self.parsed_spans.push(span);
        self.addresses.insert(
            (kind, std::ptr::from_ref(node).addr(), span.start, span.end),
            id,
        );
        self.aliases
            .entry((kind, span.start, span.end))
            .and_modify(|alias| *alias = Alias::Ambiguous)
            .or_insert(Alias::Unique(id));
        Ok(id)
    }

    /// Exact AST identities take precedence. A clone can reuse a unique parser
    /// occurrence; ambiguity is retained explicitly for the borrowed-helper work.
    pub(crate) fn find<T>(
        &mut self,
        kind: OccurrenceKind,
        node: &Node<T>,
    ) -> Result<Option<OccurrenceId>, Error> {
        if let Some(id) = self.addresses.get(&(
            kind,
            std::ptr::from_ref(&node.node).addr(),
            node.span.start,
            node.span.end,
        )) {
            return Ok(Some(*id));
        }
        match self.aliases.get(&(kind, node.span.start, node.span.end)) {
            Some(Alias::Unique(id)) => Ok(Some(*id)),
            Some(Alias::Ambiguous) => {
                self.budget.charge(1, 0, 0, node.span.start)?;
                self.ambiguous_spans.push(node.span);
                Ok(None)
            }
            None => {
                // A synthesized checker-only node has no written occurrence.
                self.budget.charge(1, 0, 0, node.span.start)?;
                self.ambiguous_spans.push(node.span);
                Ok(None)
            }
        }
    }

    fn scope(
        &mut self,
        kind: ScopeKind,
        span: Span,
        parent: Option<ScopeId>,
    ) -> Result<ScopeId, Error> {
        self.budget
            .charge(1, usize::from(parent.is_some()), 0, span.start)?;
        let id = ScopeId(self.code.scopes.len() as u32);
        self.code.scopes.push(Scope {
            parent,
            kind,
            source: unmapped_span(span),
            declarations: Vec::new(),
        });
        self.scope_spans.push(span);
        self.current = id;
        Ok(id)
    }

    pub(crate) fn enter_scope(
        &mut self,
        kind: ScopeKind,
        span: Span,
        reuse: Option<ScopeId>,
    ) -> Result<ScopeId, Error> {
        if let Some(id) = reuse {
            let scope = &mut self.code.scopes[id.index()];
            if scope.parent != Some(self.current) || scope.kind != ScopeKind::Prototype {
                return Err(Error::new(span.start, "invalid retained function scope"));
            }
            scope.kind = kind;
            for site in &scope.declarations {
                let site = &mut self.code.declarations[site.index()];
                if self.code.entities[site.entity.index()].kind == EntityKind::Parameter {
                    site.definition = true;
                }
            }
            self.scope_spans[id.index()] = span;
            self.current = id;
            Ok(id)
        } else {
            self.scope(kind, span, Some(self.current))
        }
    }

    pub(crate) fn leave_scope(&mut self) -> ScopeId {
        let id = self.current;
        self.current = self.code.scopes[id.index()].parent.unwrap_or(ScopeId(0));
        id
    }

    pub(crate) fn intern_type(&mut self, ty: &Type, offset: usize) -> Result<TypeId, Error> {
        let hash = self.hasher.hash_one(ty);
        if let Some(ids) = self.type_hashes.get(&hash) {
            for id in ids {
                if self.code.types[id.index()] == *ty {
                    return Ok(*id);
                }
            }
        }
        charge_type(&mut self.budget, ty, offset, 0)?;
        self.budget.charge(0, 1, 0, offset)?;
        let id = TypeId(self.code.types.len() as u32);
        self.code.types.push(ty.clone());
        self.type_hashes.entry(hash).or_default().push(id);
        Ok(id)
    }

    fn entity(
        &mut self,
        key: EntityKey,
        name: Option<&str>,
        kind: EntityKind,
        offset: usize,
    ) -> Result<EntityId, Error> {
        if let Some(id) = self.entities.get(&key) {
            return Ok(*id);
        }
        self.budget.charge(
            1,
            1,
            name.map_or(0, |name| name.len().saturating_mul(2)),
            offset,
        )?;
        let id = EntityId(self.code.entities.len() as u32);
        self.code.entities.push(Entity {
            returns_twice: false,
            symbol_binding: crate::SymbolBinding::Strong,
            body: None,
            name: name.map(str::to_owned),
            kind,
            declaration: None,
            linkage: Linkage::None,
        });
        self.entities.insert(key, id);
        Ok(id)
    }

    fn site(
        &mut self,
        entity: EntityId,
        occurrence: OccurrenceId,
        ty: &Type,
        name_span: Option<Span>,
        properties: SiteProperties,
    ) -> Result<SiteId, Error> {
        let offset = self.parsed_spans[occurrence.index()].start;
        let (type_use, declared_type_use) =
            self.declaration_type_use(entity, occurrence, ty, offset)?;
        let ty = self.intern_type(ty, offset)?;
        self.budget.charge(1, 6, 0, offset)?;
        let id = SiteId(self.code.declarations.len() as u32);
        self.code.declarations.push(DeclarationSite {
            returns_twice: self.code.entities[entity.index()].returns_twice,
            returns_twice_attribute: None,
            symbol_binding: self.code.entities[entity.index()].symbol_binding,
            weak_attribute: None,
            body: None,
            initializer: None,
            type_use,
            declared_type_use,
            entity,
            occurrence,
            scope: self.current,
            ty,
            storage: properties.storage,
            linkage: properties.linkage,
            register: properties.register,
            definition: properties.definition,
            flexible_array_storage: None,
            name_source: None,
        });
        self.name_spans.push(name_span);
        self.code.scopes[self.current.index()].declarations.push(id);
        if !matches!(
            self.code.entities[entity.index()].kind,
            EntityKind::Record(_) | EntityKind::Enum(_) | EntityKind::Field { .. }
        ) {
            self.bind_name(entity, offset)?;
        }
        Ok(id)
    }

    fn bind_name(&mut self, entity: EntityId, offset: usize) -> Result<(), Error> {
        if let Some(name) = &self.code.entities[entity.index()].name {
            let names = self.names.entry(self.current).or_default();
            if let Some(binding) = names.get_mut(name) {
                *binding = entity;
            } else {
                self.budget.charge(0, 1, name.len(), offset)?;
                names.insert(name.clone(), entity);
            }
        }
        Ok(())
    }

    pub(crate) fn complete_declaration(
        &mut self,
        site: SiteId,
        ty: &Type,
        allocation: Option<&FlexibleArrayStorage>,
    ) -> Result<(), Error> {
        let occurrence = self.code.declarations[site.index()].occurrence;
        let offset = self.parsed_spans[occurrence.index()].start;
        let type_use =
            self.retype_use(self.code.declarations[site.index()].type_use, ty, offset)?;
        let ty = self.intern_type(ty, offset)?;
        let site = &mut self.code.declarations[site.index()];
        site.type_use = type_use;
        site.ty = ty;
        site.flexible_array_storage = allocation.cloned();
        Ok(())
    }

    pub(crate) fn synthetic_object(&mut self, name: &str, offset: usize) -> Result<(), Error> {
        let entity = self.entity(
            EntityKey::Ordinary(self.current, name.to_owned()),
            Some(name),
            EntityKind::Variable,
            offset,
        )?;
        self.bind_name(entity, offset)
    }

    pub(crate) fn file_declaration<T>(
        &mut self,
        item: &Node<T>,
        kind: OccurrenceKind,
        declaration: &Declaration,
        index: usize,
        definition: bool,
        name_span: Option<Span>,
    ) -> Result<Option<SiteId>, Error> {
        let occurrence = if let Some(id) = self.definition {
            Some(id)
        } else {
            self.find(kind, item)?
        };
        let Some(occurrence) = occurrence else {
            return Ok(None);
        };
        let entity_kind = declaration.kind.into();
        let key = if declaration.kind == DeclarationKind::Typedef {
            EntityKey::Ordinary(self.current, declaration.name.clone())
        } else {
            EntityKey::Linked(declaration.name.clone())
        };
        let entity = self.entity(key, Some(&declaration.name), entity_kind, item.span.start)?;
        self.code.entities[entity.index()].declaration = Some(index);
        let storage = if declaration.kind == DeclarationKind::Variable {
            Storage::Static
        } else {
            Storage::None
        };
        let linkage = if declaration.kind == DeclarationKind::Typedef {
            Linkage::None
        } else if declaration.is_static {
            Linkage::Internal
        } else {
            Linkage::External
        };
        self.code.entities[entity.index()].linkage = linkage;
        self.code.entities[entity.index()].returns_twice = declaration.returns_twice;
        self.code.entities[entity.index()].symbol_binding = declaration.symbol_binding;
        let site = self.site(
            entity,
            occurrence,
            &declaration.ty,
            name_span,
            SiteProperties {
                storage,
                linkage,
                register: false,
                definition,
            },
        )?;
        self.code.declarations[site.index()].flexible_array_storage =
            declaration.flexible_array_storage.clone();
        Ok(Some(site))
    }

    pub(crate) fn local_declaration<T>(
        &mut self,
        item: &Node<T>,
        occurrence_kind: OccurrenceKind,
        declaration: LocalDeclaration<'_>,
    ) -> Result<Option<SiteId>, Error> {
        let Some(occurrence) = self.find(occurrence_kind, item)? else {
            return Ok(None);
        };
        let key = match declaration.name {
            Some(name) if declaration.linked => EntityKey::Linked(name.to_owned()),
            Some(name) => EntityKey::Ordinary(self.current, name.to_owned()),
            None => EntityKey::Unnamed(self.current, occurrence),
        };
        let entity = self.entity(key, declaration.name, declaration.kind, item.span.start)?;
        let linkage = if declaration.linked {
            let entity = &mut self.code.entities[entity.index()];
            if entity.linkage == Linkage::None {
                entity.linkage = Linkage::External;
            }
            entity.linkage
        } else {
            Linkage::None
        };
        let site = self.site(
            entity,
            occurrence,
            declaration.ty,
            declaration.name_span,
            SiteProperties {
                storage: declaration.storage,
                linkage,
                register: declaration.register,
                definition: declaration.definition,
            },
        )?;
        self.code.declarations[site.index()].flexible_array_storage =
            declaration.allocation.cloned();
        Ok(Some(site))
    }

    pub(crate) fn tag<T>(
        &mut self,
        node: &Node<T>,
        kind: OccurrenceKind,
        name: Option<&str>,
        ty: Type,
        definition: bool,
        name_span: Option<Span>,
    ) -> Result<(), Error> {
        let Some(occurrence) = self.find(kind, node)? else {
            return Ok(());
        };
        let (key, entity_kind) = match ty.kind {
            TypeKind::Record(id) => (EntityKey::Record(id), EntityKind::Record(id)),
            TypeKind::Enum(id) => (EntityKey::Enum(id), EntityKind::Enum(id)),
            _ => {
                return Err(Error::new(
                    node.span.start,
                    "retained tag has a non-tag type",
                ));
            }
        };
        let is_reference = self.entities.contains_key(&key)
            && !definition
            && !self.reference_builder.standalone_tags.contains(&(
                kind,
                node.span.start,
                node.span.end,
            ));
        let entity = self.entity(key, name, entity_kind, node.span.start)?;
        if is_reference {
            if let Some(span) = name_span {
                self.reference(entity, references::ReferenceKind::Tag, span)?;
            }
            return Ok(());
        }
        self.site(
            entity,
            occurrence,
            &ty,
            name_span,
            SiteProperties {
                storage: Storage::None,
                linkage: Linkage::None,
                register: false,
                definition,
            },
        )?;
        Ok(())
    }

    pub(crate) fn enumerator(
        &mut self,
        node: &Node<ast::Enumerator>,
        enumeration: usize,
        variant: usize,
        ty: &Type,
    ) -> Result<(), Error> {
        let Some(occurrence) = self.find(OccurrenceKind::Enumerator, node)? else {
            return Ok(());
        };
        let name = &node.node.identifier.node.name;
        let entity = self.entity(
            EntityKey::Enumerator(enumeration, variant),
            Some(name),
            EntityKind::Enumerator {
                enumeration,
                variant,
            },
            node.span.start,
        )?;
        self.site(
            entity,
            occurrence,
            ty,
            Some(node.node.identifier.span),
            SiteProperties {
                storage: Storage::None,
                linkage: Linkage::None,
                register: false,
                definition: true,
            },
        )?;
        Ok(())
    }

    pub(crate) fn finish(mut self, offsets: &SourceMap) -> Result<CheckedCode, Error> {
        for (occurrence, span) in self.code.occurrences.iter_mut().zip(&self.parsed_spans) {
            occurrence.source = map_span(offsets, *span, &mut self.budget)?;
        }
        self.finish_expression_coverage()?;
        self.finish_statements()?;
        self.finish_references(offsets)?;
        self.finish_diagnostic_attributes(offsets)?;
        self.finish_initializer_coverage()?;
        self.finish_bounds(offsets)?;
        self.finish_type_ownership()?;
        for (occurrence, missing, kind) in self
            .code
            .expression_coverage
            .iter()
            .map(|entry| {
                (
                    entry.occurrence,
                    matches!(entry.status, expression::Coverage::Missing),
                    "expression",
                )
            })
            .chain(self.code.statement_coverage.iter().map(|entry| {
                (
                    entry.occurrence,
                    matches!(entry.status, statement::Coverage::Missing),
                    "statement",
                )
            }))
            .chain(self.code.initializer_coverage.iter().map(|entry| {
                (
                    entry.occurrence,
                    matches!(entry.status, initializer::Coverage::Missing),
                    "initializer",
                )
            }))
        {
            if missing {
                return Err(Error::new(
                    self.parsed_spans[occurrence.index()].start,
                    format!("checked-code retention does not support this {kind}"),
                ));
            }
        }
        if let Some(span) = self.ambiguous_spans.first() {
            return Err(Error::new(
                span.start,
                "checked-code retention has an ambiguous source occurrence",
            ));
        }
        for (scope, span) in self.code.scopes.iter_mut().zip(self.scope_spans) {
            if scope.kind != ScopeKind::File {
                scope.source = map_span(offsets, span, &mut self.budget)?;
            }
            scope.declarations.sort_by_key(|id| {
                let occurrence = self.code.declarations[id.index()].occurrence;
                self.code.occurrences[occurrence.index()].source.range.start
            });
        }
        for (site, span) in self.code.declarations.iter_mut().zip(self.name_spans) {
            if let Some(attribute) = &site.returns_twice_attribute {
                site.returns_twice_attribute = Some(map_span(
                    offsets,
                    Span::span(attribute.range.start, attribute.range.end),
                    &mut self.budget,
                )?);
            }
            if let Some(attribute) = &site.weak_attribute {
                site.weak_attribute = Some(map_span(
                    offsets,
                    Span::span(attribute.range.start, attribute.range.end),
                    &mut self.budget,
                )?);
            }
            site.name_source = span
                .map(|span| map_span(offsets, span, &mut self.budget))
                .transpose()?;
        }
        for span in self.ambiguous_spans {
            self.code
                .ambiguous_aliases
                .push(map_span(offsets, span, &mut self.budget)?);
        }
        Ok(self.code)
    }
}

fn unmapped_span(span: Span) -> SourceSpan {
    SourceSpan {
        range: span.start..span.end,
        fragments: Vec::new(),
        synthetic: false,
    }
}

fn map_span(offsets: &SourceMap, span: Span, budget: &mut Budget) -> Result<SourceSpan, Error> {
    let mut first: Option<Range<usize>> = None;
    let mut fragments = Vec::new();
    let mut anchor = None;
    offsets.original_ranges(span.start..span.end, |range| {
        if range.is_empty() {
            anchor.get_or_insert(range.start);
        } else {
            budget.charge(0, 1, 0, span.start)?;
            if let Some(first) = &mut first {
                if fragments.is_empty() && first.end == range.start {
                    first.end = range.end;
                } else {
                    if fragments.is_empty() {
                        budget.charge(0, 0, std::mem::size_of::<Range<usize>>(), span.start)?;
                        fragments.push(first.clone());
                    }
                    budget.charge(0, 0, std::mem::size_of::<Range<usize>>(), span.start)?;
                    fragments.push(range);
                }
            } else {
                first = Some(range);
            }
        }
        Ok(())
    })?;
    let synthetic = first.is_none();
    let range = if fragments.is_empty() {
        first.unwrap_or_else(|| {
            let anchor = anchor.unwrap_or(0);
            anchor..anchor
        })
    } else {
        let start = fragments.iter().map(|range| range.start).min().unwrap_or(0);
        let end = fragments
            .iter()
            .map(|range| range.end)
            .max()
            .unwrap_or(start);
        start..end
    };
    Ok(SourceSpan {
        range,
        fragments,
        synthetic,
    })
}

fn charge_type(budget: &mut Budget, ty: &Type, offset: usize, depth: usize) -> Result<(), Error> {
    if depth >= 128 {
        return Err(Error::new(offset, "retained type nesting limit exceeded"));
    }
    budget.charge(1, 0, std::mem::size_of::<Type>(), offset)?;
    match &ty.kind {
        TypeKind::Pointer(inner)
        | TypeKind::Array { element: inner, .. }
        | TypeKind::Vector { element: inner, .. }
        | TypeKind::VariableArray { element: inner } => {
            budget.charge(0, 1, 0, offset)?;
            charge_type(budget, inner, offset, depth + 1)?;
        }
        TypeKind::Function(function) => {
            budget.charge(0, function.parameters.len() + 1, 0, offset)?;
            charge_type(budget, &function.return_type, offset, depth + 1)?;
            for parameter in &function.parameters {
                budget.charge(0, 0, parameter.name.as_ref().map_or(0, String::len), offset)?;
                charge_type(budget, &parameter.ty, offset, depth + 1)?;
            }
        }
        TypeKind::Typedef(name) => budget.charge(0, 0, name.len(), offset)?,
        _ => {}
    }
    Ok(())
}

macro_rules! visit_occurrence {
    ($method:ident, $ty:ty, $kind:ident) => {
        visit_occurrence!($method, $ty, $kind, |_: &mut Builder, _: &$ty| Ok::<
            (),
            Error,
        >(()));
    };
    ($method:ident, $ty:ty, $kind:ident, $before:expr) => {
        fn $method(&mut self, node: &'ast $ty, span: &'ast Span) {
            if self.error.is_some() {
                return;
            }
            let occurrence = match self.occurrence(OccurrenceKind::$kind, node, *span) {
                Ok(id) => id,
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            };
            let previous_owner = self.catalog_type_owner(occurrence);
            if self.depth >= 256 {
                self.error = Some(Error::new(
                    span.start,
                    "checked-code occurrence nesting limit exceeded",
                ));
                return;
            }
            if let Err(error) = ($before)(self, node) {
                self.error = Some(error);
                return;
            }
            self.depth += 1;
            visit::$method(self, node, span);
            self.depth -= 1;
            self.ownership_builder.catalog_owner = previous_owner;
        }
    };
}

impl<'ast> Visit<'ast> for Builder {
    fn visit_attribute(&mut self, node: &'ast ast::Attribute, span: &'ast Span) {
        if self.error.is_some() {
            return;
        }
        self.attribute_depth += 1;
        visit::visit_attribute(self, node, span);
        self.attribute_depth -= 1;
    }

    visit_occurrence!(
        visit_declaration,
        ast::Declaration,
        Declaration,
        |builder: &mut Builder, declaration: &ast::Declaration| {
            if declaration.declarators.is_empty() {
                for specifier in &declaration.specifiers {
                    if let ast::DeclarationSpecifier::TypeSpecifier(ty) = &specifier.node {
                        builder.standalone_tag(ty)?;
                    }
                }
            }
            Ok::<(), Error>(())
        }
    );
    visit_occurrence!(
        visit_struct_field,
        ast::StructField,
        Field,
        |builder: &mut Builder, field: &ast::StructField| {
            if field.declarators.is_empty() {
                for specifier in &field.specifiers {
                    if let ast::SpecifierQualifier::TypeSpecifier(ty) = &specifier.node {
                        builder.standalone_tag(ty)?;
                    }
                }
            }
            Ok::<(), Error>(())
        }
    );
    visit_occurrence!(
        visit_struct_declarator,
        ast::StructDeclarator,
        StructDeclarator
    );
    visit_occurrence!(visit_init_declarator, ast::InitDeclarator, InitDeclarator);
    visit_occurrence!(visit_declarator, ast::Declarator, Declarator);
    visit_occurrence!(
        visit_parameter_declaration,
        ast::ParameterDeclaration,
        Parameter
    );
    visit_occurrence!(visit_struct_type, ast::StructType, Record);
    visit_occurrence!(visit_enum_type, ast::EnumType, Enum);
    visit_occurrence!(visit_enumerator, ast::Enumerator, Enumerator);
    visit_occurrence!(visit_function_definition, ast::FunctionDefinition, Function);
    visit_occurrence!(
        visit_type_name,
        ast::TypeName,
        TypeName,
        |builder: &mut Builder, name: &ast::TypeName| builder.catalog_type_name(name)
    );
    visit_occurrence!(visit_type_of, ast::TypeOf, TypeOf);
    visit_occurrence!(visit_expression, ast::Expression, Expression);
    visit_occurrence!(visit_initializer, ast::Initializer, Initializer);
    visit_occurrence!(
        visit_initializer_list_item,
        ast::InitializerListItem,
        InitializerItem
    );
    visit_occurrence!(visit_designator, ast::Designator, Designator);
    visit_occurrence!(visit_statement, ast::Statement, Statement);
    visit_occurrence!(visit_static_assert, ast::StaticAssert, StaticAssert);
    visit_occurrence!(visit_label, ast::Label, Label);
    visit_occurrence!(visit_string_literal, ast::StringLiteral, StringLiteral);
    visit_occurrence!(visit_gnu_asm_operand, ast::GnuAsmOperand, AsmOperand);
}

impl Builder {
    /// Keeps explicit spelling on its declaration and merged binding on the entity.
    pub(crate) fn attach_symbol_binding(
        &mut self,
        site: SiteId,
        binding: crate::SymbolBinding,
        attribute: Option<Span>,
    ) {
        let site = &mut self.code.declarations[site.index()];
        site.symbol_binding = binding;
        self.code.entities[site.entity.index()].symbol_binding = binding;
        site.weak_attribute = attribute.map(|span| SourceSpan {
            range: span.start..span.end,
            fragments: Vec::new(),
            synthetic: false,
        });
    }
}

impl Builder {
    /// Records the effective declaration state and its optional written attribute.
    pub(crate) fn attach_returns_twice(
        &mut self,
        id: SiteId,
        returns_twice: bool,
        attribute: Option<Span>,
    ) -> Result<(), Error> {
        if let Some(span) = attribute {
            self.budget.charge(0, 1, 0, span.start)?;
        }
        let site = &mut self.code.declarations[id.index()];
        site.returns_twice = returns_twice;
        self.code.entities[site.entity.index()].returns_twice = returns_twice;
        site.returns_twice_attribute = attribute.map(crate::checked::unmapped_span);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::analyze_inner;
    use crate::{IntegerKind, TranslationUnit};
    use toucan_target::Target;

    fn retained(source: &str) -> (TranslationUnit, CheckedCode) {
        let (unit, code) = analyze_inner(
            source,
            Target::X86_64UnknownLinuxGnu,
            Some(Limits::default()),
        )
        .unwrap();
        let code = code.unwrap();
        assert!(
            code.ambiguous_aliases.is_empty(),
            "{:#?}",
            code.ambiguous_aliases
        );
        (unit, code)
    }

    fn sites<'a>(code: &'a CheckedCode, name: &str) -> Vec<&'a DeclarationSite> {
        code.declarations
            .iter()
            .filter(|site| code.entities[site.entity.index()].name.as_deref() == Some(name))
            .collect()
    }

    #[test]
    fn declarations_keep_shadowed_entities_and_definition_parameters() {
        let source = r#"
            typedef int T;
            struct S { int value; };
            int f(int prototype_name);
            int f(int definition_name) {
                typedef long T;
                T local;
                { short local; struct S { char value; }; enum E { choice = 2 }; }
                for (int i = 0; i < 1; ++i) { int local; }
                return definition_name;
            }
        "#;
        let (_, code) = retained(source);
        let aliases = sites(&code, "T");
        assert_eq!(aliases.len(), 2);
        assert_ne!(aliases[0].entity, aliases[1].entity);
        let locals = sites(&code, "local");
        assert_eq!(locals.len(), 3);
        for pair in locals.windows(2) {
            assert_ne!(pair[0].entity, pair[1].entity);
            assert_ne!(pair[0].scope, pair[1].scope);
        }
        assert_eq!(
            code.types[locals[0].ty.index()].kind,
            TypeKind::Integer(IntegerKind::Long)
        );
        assert_eq!(
            code.types[locals[1].ty.index()].kind,
            TypeKind::Integer(IntegerKind::Short)
        );
        let prototype = sites(&code, "prototype_name")[0];
        let parameter = sites(&code, "definition_name")[0];
        assert_eq!(
            code.scopes[prototype.scope.index()].kind,
            ScopeKind::Prototype
        );
        assert_eq!(
            code.scopes[parameter.scope.index()].kind,
            ScopeKind::Function
        );
        assert!(!prototype.definition);
        assert!(parameter.definition);
        assert_eq!(parameter.scope, locals[0].scope);
        let records = sites(&code, "S");
        assert_eq!(records.len(), 2);
        assert_ne!(records[0].entity, records[1].entity);
        assert!(matches!(
            code.entities[sites(&code, "choice")[0].entity.index()].kind,
            EntityKind::Enumerator { .. }
        ));
    }

    #[test]
    fn redeclarations_share_entities_without_losing_sites() {
        let source = r#"
            static int shared;
            typedef int T;
            typedef int T;
            void first(void) {
                extern int shared;
                extern int later;
                extern int later;
                typedef int T;
                typedef int T;
            }
            int later;
            void second(void) { extern int later; int shared; }
            int unnamed(int, int);
        "#;
        let (unit, code) = retained(source);
        let later = sites(&code, "later");
        assert_eq!(later.len(), 4);
        assert!(later.iter().all(|site| site.entity == later[0].entity));
        let declaration = code.entities[later[0].entity.index()].declaration.unwrap();
        assert_eq!(unit.declarations[declaration].name, "later");
        let shared = sites(&code, "shared");
        assert_eq!(shared.len(), 3);
        assert_eq!(shared[0].entity, shared[1].entity);
        assert_eq!(shared[0].linkage, Linkage::Internal);
        assert_eq!(shared[1].linkage, Linkage::Internal);
        assert_ne!(shared[0].entity, shared[2].entity);
        assert_eq!(shared[2].linkage, Linkage::None);
        let aliases = sites(&code, "T");
        assert_eq!(aliases.len(), 4);
        assert_eq!(aliases[0].entity, aliases[1].entity);
        assert_eq!(aliases[2].entity, aliases[3].entity);
        assert_ne!(aliases[0].entity, aliases[2].entity);
        let unnamed: Vec<_> = code
            .declarations
            .iter()
            .filter(|site| {
                let entity = &code.entities[site.entity.index()];
                entity.name.is_none()
                    && entity.kind == EntityKind::Parameter
                    && code.types[site.ty.index()].kind == TypeKind::Integer(IntegerKind::Int)
            })
            .collect();
        assert_eq!(unnamed.len(), 2);
        assert_ne!(unnamed[0].entity, unnamed[1].entity);
        assert_eq!(unnamed[0].ty, unnamed[1].ty);
    }

    #[test]
    fn completed_local_arrays_and_static_allocation_survive_scope_exit() {
        let (_, code) = {
            let source = String::from(
                "void f(void) { char text[] = \"abc\"; struct S {int n; int a[];}; static struct S object = {2, {1,2}}; }",
            );
            retained(&source)
        };
        let text = sites(&code, "text")[0];
        assert!(matches!(
            code.types[text.ty.index()].kind,
            TypeKind::Array {
                length: Some(4),
                ..
            }
        ));
        assert_eq!(text.storage, Storage::Automatic);
        let object = sites(&code, "object")[0];
        assert_eq!(object.storage, Storage::Static);
        assert_eq!(object.flexible_array_storage.as_ref().unwrap().elements, 2);
        assert_eq!(
            object.flexible_array_storage.as_ref().unwrap().size_bits,
            96
        );
    }

    #[test]
    fn adapter_spans_cover_original_source_and_preserve_reordered_fragments() {
        let source = "struct S { int value; }; int f(void) { struct S local = (struct S){}; int (*pointer)(void) = (int (__attribute__((noinline)) *)(void))f; return pointer(); }";
        let (_, code) = retained(source);
        assert_eq!(code.scopes[0].source.range, 0..source.len());
        for occurrence in &code.occurrences {
            assert!(occurrence.source.range.start <= occurrence.source.range.end);
            assert!(occurrence.source.range.end <= source.len());
            for range in &occurrence.source.fragments {
                assert!(range.end <= source.len());
            }
        }
        for name in ["local", "pointer"] {
            let site = sites(&code, name)[0];
            let occurrence = &code.occurrences[site.occurrence.index()];
            assert!(source[occurrence.source.range.clone()].contains(name));
            assert_eq!(
                &source[site.name_source.as_ref().unwrap().range.clone()],
                name
            );
        }
        assert!(
            code.occurrences
                .iter()
                .any(|occurrence| !occurrence.source.fragments.is_empty())
        );
        assert!(
            code.occurrences
                .iter()
                .any(|occurrence| occurrence.source.synthetic)
        );
    }

    #[test]
    fn sparse_bounds_stay_compact_and_quota_offsets_are_remapped() {
        let (_, code) = retained("int values[1099511627776] = {[1099511627775] = 1};");
        assert!(code.occurrences.len() < 20);
        assert_eq!(sites(&code, "values").len(), 1);
        let source = "struct S {int x;}; struct S s = (struct S){}; int after;";
        let after = source.find("after").unwrap();
        assert!((1..100).any(|nodes| {
            analyze_inner(
                source,
                Target::X86_64UnknownLinuxGnu,
                Some(Limits {
                    nodes,
                    ..Limits::default()
                }),
            )
            .is_err_and(|error| {
                error.offset == after && error.message.contains("retention node limit")
            })
        }));
    }

    #[test]
    fn finalization_rejects_unchecked_written_code() {
        let parsed = lang_c::driver::parse_preprocessed(
            &lang_c::driver::Config::default(),
            "int f(void) { return 1; }".into(),
        )
        .unwrap();
        let builder = Builder::new(&parsed.unit, parsed.source.len(), Limits::default()).unwrap();
        let error = builder.finish(&SourceMap::default()).unwrap_err();
        assert!(error.message.contains("does not support this expression"));
        assert_eq!(&parsed.source[error.offset..error.offset + 1], "1");
    }

    #[test]
    fn clone_aliases_reuse_only_unambiguous_occurrences() {
        let parsed = lang_c::driver::parse_preprocessed(
            &lang_c::driver::Config::default(),
            "int value;".into(),
        )
        .unwrap();
        let mut builder =
            Builder::new(&parsed.unit, parsed.source.len(), Limits::default()).unwrap();
        let ast::ExternalDeclaration::Declaration(declaration) = &parsed.unit.0[0].node else {
            panic!("declaration")
        };
        let original = &declaration.node.declarators[0].node.declarator;
        let copied = original.clone();
        let id = builder
            .find(OccurrenceKind::Declarator, original)
            .unwrap()
            .unwrap();
        assert_eq!(
            builder.find(OccurrenceKind::Declarator, &copied).unwrap(),
            Some(id)
        );
        builder.aliases.insert(
            (
                OccurrenceKind::Declarator,
                original.span.start,
                original.span.end,
            ),
            Alias::Ambiguous,
        );
        assert_eq!(
            builder.find(OccurrenceKind::Declarator, original).unwrap(),
            Some(id)
        );
        assert_eq!(
            builder.find(OccurrenceKind::Declarator, &copied).unwrap(),
            None
        );
        let error = builder.finish(&SourceMap::default()).unwrap_err();
        assert!(error.message.contains("ambiguous source occurrence"));
        assert_eq!(error.offset, original.span.start);
    }

    #[test]
    fn retention_limits_fail_explicitly_without_changing_default_analysis() {
        let source = "int value;";
        for (limits, resource) in [
            (
                Limits {
                    nodes: 0,
                    ..Limits::default()
                },
                "node",
            ),
            (
                Limits {
                    edges: 0,
                    ..Limits::default()
                },
                "edge",
            ),
            (
                Limits {
                    payload_bytes: 0,
                    ..Limits::default()
                },
                "payload byte",
            ),
        ] {
            let error =
                analyze_inner(source, Target::X86_64UnknownLinuxGnu, Some(limits)).unwrap_err();
            assert!(error.message.contains(resource), "{error}");
            assert!(error.offset <= source.len());
        }
        let (unit, code) = analyze_inner(source, Target::X86_64UnknownLinuxGnu, None).unwrap();
        assert!(code.is_none());
        assert_eq!(unit.declarations.len(), 1);
        let (with_code, _) = retained(source);
        assert_eq!(format!("{unit:?}"), format!("{with_code:?}"));
        for invalid in [
            "int f(void) { return missing; }",
            "int f(void) { const int a=1; a=2; return a; }",
        ] {
            let without = analyze_inner(invalid, Target::X86_64UnknownLinuxGnu, None).unwrap_err();
            let with = analyze_inner(
                invalid,
                Target::X86_64UnknownLinuxGnu,
                Some(Limits::default()),
            )
            .unwrap_err();
            assert_eq!(without.message, with.message);
            assert_eq!(without.offset, with.offset);
        }
    }
}

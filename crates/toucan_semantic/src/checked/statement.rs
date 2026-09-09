//! Owned checked function bodies. Control-flow references identify their actual
//! enclosing statements; expressions preserve evaluation context on each use.

use std::collections::HashMap;

use lang_c::{ast, span::Node};
use serde::Serialize;

use super::expression::{Conversion, ExprUse, UseContext};
use super::{Builder, EntityId, EntityKind, OccurrenceId, OccurrenceKind, ScopeId, SiteId, TypeId};
use crate::analyze::Analyzer;
use crate::integer::{convert, integer_to_type};
use crate::{DecodedString, Error, IntegerValue, Type, TypeKind};

macro_rules! id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
        pub struct $name(pub(crate) u32);
        impl $name {
            /// Returns the owner-local arena index.
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}
id!(StatementId);
id!(DeclarationGroupId);
id!(AssertionId);
id!(BodyId);

#[derive(Debug, Serialize)]
pub struct FunctionBody {
    pub(crate) definition_kind: crate::FunctionDefinitionKind,
    pub(crate) entity: EntityId,
    pub(crate) declaration: SiteId,
    pub(crate) statement: StatementId,
    pub(crate) scope: ScopeId,
    pub(crate) signature: TypeId,
    pub(crate) parameters: Vec<SiteId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) old_style: Option<Box<OldStyleDefinition>>,
}

/// Entry facts for a definition written with an identifier list.
#[derive(Debug, Serialize)]
pub struct OldStyleDefinition {
    /// Declaration-list groups in source order, not runtime execution order.
    pub(crate) declarations: Vec<DeclarationGroupId>,
    /// Incoming arguments in identifier-list order.
    pub(crate) parameters: Vec<ParameterEntry>,
    pub(crate) evaluation_order: ParameterEvaluationOrder,
}

/// The entry ordering promised by the C source semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum ParameterEvaluationOrder {
    /// Parameter conversions and required bounds complete before the body;
    /// the lists do not specify a total order between different parameters.
    UnspecifiedBetweenParameters,
}

/// Conversion from an incoming C argument value to its local parameter object.
/// This describes C types, not machine registers or ABI lowering.
#[derive(Debug, Serialize)]
pub struct ParameterEntry {
    pub(crate) identifier: OccurrenceId,
    pub(crate) declaration: SiteId,
    pub(crate) incoming: super::TypeUseId,
    /// Value conversion at entry; never an atomic load from the incoming value.
    pub(crate) conversions: Vec<super::ConversionStep>,
}

#[derive(Debug, Serialize)]
pub struct StatementCoverage {
    pub(crate) occurrence: OccurrenceId,
    pub(crate) status: Coverage,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum Coverage {
    Checked(StatementId),
    AttributeArgument,
    ParserInserted,
    Missing,
}

#[derive(Debug, Serialize)]
pub struct Statement {
    pub(crate) occurrence: OccurrenceId,
    pub(crate) scope: ScopeId,
    pub(crate) kind: StatementKind,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum StatementKind {
    /// Reserved for construction; never present in a successful analysis.
    #[doc(hidden)]
    Checking,
    Block(Vec<BlockItem>),
    Expression(Option<ExprUse>),
    /// A warning-suppression annotation; execution continues normally.
    Fallthrough {
        switch: StatementId,
    },
    Return(Option<ExprUse>),
    If {
        condition: ExprUse,
        then_statement: StatementId,
        else_statement: Option<StatementId>,
    },
    While {
        condition: ExprUse,
        body: StatementId,
    },
    DoWhile {
        body: StatementId,
        condition: ExprUse,
    },
    For {
        initializer: ForInitializer,
        condition: Option<ExprUse>,
        step: Option<ExprUse>,
        body: StatementId,
    },
    Switch {
        expression: ExprUse,
        body: StatementId,
    },
    Labeled {
        occurrence: OccurrenceId,
        label: Label,
        statement: StatementId,
    },
    Goto {
        name: String,
        target: Option<StatementId>,
    },
    Break {
        target: StatementId,
    },
    Continue {
        target: StatementId,
    },
    Assembly(Assembly),
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum BlockItem {
    Declaration(DeclarationGroupId),
    Statement(StatementId),
    Assertion(AssertionId),
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum ForInitializer {
    Empty,
    Expression(ExprUse),
    Declaration(DeclarationGroupId),
    Assertion(AssertionId),
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum Label {
    Identifier(String),
    Case {
        expression: ExprUse,
        value: IntegerValue,
        switch: StatementId,
    },
    CaseRange {
        low: ExprUse,
        high: ExprUse,
        low_value: IntegerValue,
        high_value: IntegerValue,
        switch: StatementId,
    },
    Default {
        switch: StatementId,
    },
}

#[derive(Debug, Serialize)]
pub struct DeclarationGroup {
    pub(crate) occurrence: OccurrenceId,
    /// Sites introduced in this declaration's lexical scope. Nested prototype
    /// and statement-expression bindings retain their separate scopes.
    pub(crate) declarations: Vec<SiteId>,
}

#[derive(Debug, Serialize)]
pub struct Assertion {
    pub(crate) occurrence: OccurrenceId,
    pub(crate) scope: ScopeId,
    /// Evaluated at translation time, with no runtime effect.
    pub(crate) condition: ExprUse,
    pub(crate) value: IntegerValue,
    pub(crate) message: DecodedString,
    pub(crate) message_occurrence: OccurrenceId,
}

#[derive(Debug, Serialize)]
pub struct Assembly {
    pub(crate) template: AssemblyText,
    pub(crate) basic: bool,
    pub(crate) volatile: bool,
    pub(crate) outputs: Vec<AssemblyOperand>,
    pub(crate) inputs: Vec<AssemblyOperand>,
    pub(crate) clobbers: Vec<AssemblyText>,
}

#[derive(Debug, Serialize)]
pub struct AssemblyText {
    pub(crate) occurrence: OccurrenceId,
    pub(crate) value: String,
}

#[derive(Debug, Serialize)]
pub struct AssemblyOperand {
    pub(crate) occurrence: OccurrenceId,
    pub(crate) name: Option<String>,
    pub(crate) constraints: AssemblyText,
    pub(crate) read_write: bool,
    pub(crate) integer_constant: bool,
    pub(crate) alternatives: Vec<AssemblyLocation>,
    /// Memory alternatives use the object as a place. Register and immediate
    /// alternatives use its value; output values are written to the place.
    pub(crate) place: Option<ExprUse>,
    pub(crate) value: Option<ExprUse>,
}

#[derive(Debug, Serialize)]
pub struct AssemblyLocation {
    pub(crate) register: bool,
    pub(crate) memory: bool,
    pub(crate) immediate: bool,
    pub(crate) fixed: Vec<String>,
    pub(crate) matching: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlKind {
    Loop,
    Switch,
}

#[derive(Default)]
pub(super) struct StatementBuilder {
    ids: HashMap<OccurrenceId, StatementId>,
    active: Vec<StatementId>,
    controls: Vec<(ControlKind, StatementId)>,
    declaration_groups: HashMap<OccurrenceId, DeclarationGroupId>,
    assertions: HashMap<OccurrenceId, AssertionId>,
    assemblies: HashMap<StatementId, Assembly>,
    function: Option<ActiveFunction>,
}

struct ActiveFunction {
    declaration: SiteId,
    labels: HashMap<String, StatementId>,
    gotos: Vec<StatementId>,
}

impl Builder {
    pub(crate) fn begin_statement(
        &mut self,
        statement: &Node<ast::Statement>,
    ) -> Result<Option<StatementId>, Error> {
        let occurrence = self
            .find(OccurrenceKind::Statement, statement)?
            .ok_or_else(|| {
                Error::new(
                    statement.span.start,
                    "statement has no unique retained occurrence",
                )
            })?;
        if let Some(id) = self.statement_builder.ids.get(&occurrence) {
            if matches!(
                self.code.statements[id.index()].kind,
                StatementKind::Checking
            ) {
                return Err(Error::new(
                    statement.span.start,
                    "recursive retained statement check",
                ));
            }
            return Ok(None);
        }
        self.budget.charge(1, 4, 0, statement.span.start)?;
        let id = StatementId(self.code.statements.len() as u32);
        self.code.statements.push(Statement {
            occurrence,
            scope: self.current,
            kind: StatementKind::Checking,
        });
        self.statement_builder.ids.insert(occurrence, id);
        self.statement_builder.active.push(id);
        Ok(Some(id))
    }

    pub(crate) fn statement_id(
        &mut self,
        statement: &Node<ast::Statement>,
    ) -> Result<StatementId, Error> {
        let occurrence = self
            .find(OccurrenceKind::Statement, statement)?
            .ok_or_else(|| {
                Error::new(
                    statement.span.start,
                    "statement has no unique retained occurrence",
                )
            })?;
        self.statement_builder
            .ids
            .get(&occurrence)
            .copied()
            .ok_or_else(|| Error::new(statement.span.start, "statement was not retained"))
    }

    pub(crate) fn statement_scope(&mut self) {
        if let Some(id) = self.statement_builder.active.last() {
            self.code.statements[id.index()].scope = self.current;
        }
    }

    fn complete_statement(&mut self, id: StatementId, kind: StatementKind) -> Result<(), Error> {
        if self.statement_builder.active.pop() != Some(id) {
            return Err(Error::new(
                self.parsed_spans[self.code.statements[id.index()].occurrence.index()].start,
                "retained statement nesting is inconsistent",
            ));
        }
        self.code.statements[id.index()].kind = kind;
        Ok(())
    }

    pub(crate) fn enter_control(&mut self, kind: ControlKind, offset: usize) -> Result<(), Error> {
        let id = self
            .statement_builder
            .active
            .last()
            .copied()
            .ok_or_else(|| {
                Error::new(offset, "retained control flow has no enclosing statement")
            })?;
        let offset = self.parsed_spans[self.code.statements[id.index()].occurrence.index()].start;
        self.budget.charge(0, 1, 0, offset)?;
        self.statement_builder.controls.push((kind, id));
        Ok(())
    }

    pub(crate) fn leave_control(&mut self) {
        self.statement_builder.controls.pop();
    }

    fn control_target(
        &self,
        only: Option<ControlKind>,
        offset: usize,
    ) -> Result<StatementId, Error> {
        self.statement_builder
            .controls
            .iter()
            .rev()
            .find(|(kind, _)| only.is_none_or(|only| only == *kind))
            .map(|(_, id)| *id)
            .ok_or_else(|| Error::new(offset, "checked jump has no retained target"))
    }

    pub(crate) fn reserve_old_style(
        &mut self,
        parameters: usize,
        declarations: usize,
        offset: usize,
    ) -> Result<(), Error> {
        self.budget.charge(
            1,
            parameters * 4 + declarations,
            std::mem::size_of::<OldStyleDefinition>()
                + parameters
                    * (std::mem::size_of::<ParameterEntry>()
                        + std::mem::size_of::<super::ConversionStep>())
                + declarations * std::mem::size_of::<DeclarationGroupId>(),
            offset,
        )
    }

    pub(crate) fn declaration_checkpoint(&self) -> usize {
        self.code.declarations.len()
    }

    pub(crate) fn complete_declaration_group(
        &mut self,
        declaration: &Node<ast::Declaration>,
        start: usize,
    ) -> Result<(), Error> {
        let occurrence = self
            .find(OccurrenceKind::Declaration, declaration)?
            .ok_or_else(|| {
                Error::new(
                    declaration.span.start,
                    "block declaration has no retained occurrence",
                )
            })?;
        let count = self.code.declarations.len() - start;
        self.budget.charge(
            1,
            count + 2,
            count * std::mem::size_of::<SiteId>(),
            declaration.span.start,
        )?;
        let id = DeclarationGroupId(self.code.declaration_groups.len() as u32);
        let declarations = (start..self.code.declarations.len())
            .filter(|index| self.code.declarations[*index].scope == self.current)
            .map(|index| SiteId(index as u32))
            .collect();
        self.code.declaration_groups.push(DeclarationGroup {
            occurrence,
            declarations,
        });
        self.statement_builder
            .declaration_groups
            .insert(occurrence, id);
        Ok(())
    }

    pub(crate) fn declaration_group(
        &mut self,
        declaration: &Node<ast::Declaration>,
    ) -> Result<DeclarationGroupId, Error> {
        let occurrence = self
            .find(OccurrenceKind::Declaration, declaration)?
            .ok_or_else(|| {
                Error::new(
                    declaration.span.start,
                    "block declaration has no retained occurrence",
                )
            })?;
        self.statement_builder
            .declaration_groups
            .get(&occurrence)
            .copied()
            .ok_or_else(|| Error::new(declaration.span.start, "block declaration was not retained"))
    }

    pub(crate) fn begin_body(
        &mut self,
        definition: &Node<ast::FunctionDefinition>,
    ) -> Result<(), Error> {
        let occurrence = self.find(OccurrenceKind::Function, definition)?;
        let declaration = self.code.scopes[self.current.index()]
            .declarations
            .last()
            .copied()
            .filter(|site| Some(self.code.declarations[site.index()].occurrence) == occurrence)
            .ok_or_else(|| {
                Error::new(
                    definition.span.start,
                    "function definition has no retained declaration site",
                )
            })?;
        self.statement_builder.function = Some(ActiveFunction {
            declaration,
            labels: HashMap::new(),
            gotos: Vec::new(),
        });
        Ok(())
    }

    pub(super) fn finish_statements(&mut self) -> Result<(), Error> {
        for statement in &self.code.statements {
            if matches!(
                statement.kind,
                StatementKind::Checking | StatementKind::Goto { target: None, .. }
            ) {
                return Err(Error::new(
                    self.parsed_spans[statement.occurrence.index()].start,
                    "retained function body contains unfinished statements",
                ));
            }
        }
        for (index, occurrence) in self.code.occurrences.iter().enumerate() {
            if occurrence.kind != OccurrenceKind::Statement {
                continue;
            }
            let id = OccurrenceId(index as u32);
            let status = if let Some(statement) = self.statement_builder.ids.get(&id) {
                Coverage::Checked(*statement)
            } else if occurrence.attribute_argument {
                Coverage::AttributeArgument
            } else if occurrence.source.synthetic {
                Coverage::ParserInserted
            } else {
                Coverage::Missing
            };
            self.budget
                .charge(1, 1, 0, self.parsed_spans[index].start)?;
            self.code.statement_coverage.push(StatementCoverage {
                occurrence: id,
                status,
            });
        }
        Ok(())
    }
}

impl Analyzer {
    fn statement_builder(&mut self) -> &mut Builder {
        self.checked
            .as_deref_mut()
            .expect("statement retention is enabled")
    }

    pub(crate) fn retain_function_body(
        &mut self,
        definition: &Node<ast::FunctionDefinition>,
    ) -> Result<(), Error> {
        let offset = definition.span.start;
        let signature = self
            .current_function_signature()
            .ok_or_else(|| Error::new(offset, "retained body has no current function"))?
            .clone();
        let signature =
            self.retained_type(&Type::new(TypeKind::Function(Box::new(signature))), offset)?;
        let statement = self
            .statement_builder()
            .statement_id(&definition.node.statement)?;
        let old_style = if let Some(signature) = self
            .current_function
            .as_ref()
            .and_then(|function| function.old_style.as_ref())
        {
            let retained = signature
                .retained
                .as_ref()
                .ok_or_else(|| Error::new(offset, "old-style body has no retained parameters"))?;
            let builder = self.checked.as_mut().expect("retained analysis");
            let mut entries = Vec::with_capacity(retained.parameters.len());
            for ((identifier, declaration), incoming) in
                retained.parameters.iter().zip(&signature.incoming)
            {
                let site = &builder.code.declarations[declaration.index()];
                let local = site.ty;
                let use_id = site.type_use;
                let incoming = builder.retype_use(use_id, incoming, offset)?;
                let conversions = if builder.code.type_uses[incoming.index()].shape == local {
                    Vec::new()
                } else {
                    vec![super::ConversionStep {
                        kind: Conversion::Assignment,
                        target_type: local,
                    }]
                };
                entries.push(ParameterEntry {
                    identifier: *identifier,
                    declaration: *declaration,
                    incoming,
                    conversions,
                });
            }
            Some(Box::new(OldStyleDefinition {
                declarations: retained.declarations.clone(),
                parameters: entries,
                evaluation_order: ParameterEvaluationOrder::UnspecifiedBetweenParameters,
            }))
        } else {
            None
        };
        let builder = self.statement_builder();
        let function = builder
            .statement_builder
            .function
            .take()
            .ok_or_else(|| Error::new(offset, "retained body has no function declaration"))?;
        for goto in function.gotos {
            let StatementKind::Goto { name, target } =
                &mut builder.code.statements[goto.index()].kind
            else {
                return Err(Error::new(offset, "retained goto is not a jump"));
            };
            *target = Some(
                *function
                    .labels
                    .get(name)
                    .ok_or_else(|| Error::new(offset, "checked goto has no retained label"))?,
            );
        }
        let entity = builder.code.declarations[function.declaration.index()].entity;
        let parameters: Vec<_> = builder.code.scopes[builder.current.index()]
            .declarations
            .iter()
            .copied()
            .filter(|site| {
                builder.code.entities[builder.code.declarations[site.index()].entity.index()].kind
                    == EntityKind::Parameter
            })
            .collect();
        let parameters = if let Some(old_style) = &old_style {
            old_style
                .parameters
                .iter()
                .map(|parameter| parameter.declaration)
                .collect()
        } else {
            parameters
        };
        builder.budget.charge(
            1,
            5 + parameters.len(),
            parameters.len() * std::mem::size_of::<SiteId>(),
            offset,
        )?;
        let id = BodyId(builder.code.bodies.len() as u32);
        builder.code.bodies.push(FunctionBody {
            definition_kind: match builder.code.entities[entity.index()].linkage {
                super::Linkage::Internal => crate::FunctionDefinitionKind::Internal,
                _ => crate::FunctionDefinitionKind::External,
            },
            entity,
            declaration: function.declaration,
            statement,
            scope: builder.current,
            signature,
            parameters,
            old_style,
        });
        builder.code.entities[entity.index()].body = Some(id);
        builder.code.declarations[function.declaration.index()].body = Some(id);
        Ok(())
    }

    pub(crate) fn retain_static_assertion(
        &mut self,
        assertion: &Node<ast::StaticAssert>,
        value: IntegerValue,
        message: DecodedString,
    ) -> Result<(), Error> {
        let occurrence = self
            .statement_builder()
            .find(OccurrenceKind::StaticAssert, assertion)?
            .ok_or_else(|| {
                Error::new(
                    assertion.span.start,
                    "static assertion has no retained occurrence",
                )
            })?;
        if self
            .statement_builder()
            .statement_builder
            .assertions
            .contains_key(&occurrence)
        {
            return Ok(());
        }
        let condition =
            self.retained_use(&assertion.node.expression, UseContext::Unevaluated, None)?;
        let message_occurrence = self
            .statement_builder()
            .find(OccurrenceKind::StringLiteral, &assertion.node.message)?
            .ok_or_else(|| {
                Error::new(
                    assertion.span.start,
                    "assertion message has no retained occurrence",
                )
            })?;
        let builder = self.statement_builder();
        builder.budget.charge(
            1,
            4,
            message.code_units.len() * std::mem::size_of::<u32>(),
            assertion.span.start,
        )?;
        let id = AssertionId(builder.code.assertions.len() as u32);
        builder.code.assertions.push(Assertion {
            occurrence,
            scope: builder.current,
            condition,
            value,
            message,
            message_occurrence,
        });
        builder.statement_builder.assertions.insert(occurrence, id);
        Ok(())
    }

    fn retained_assertion(
        &mut self,
        assertion: &Node<ast::StaticAssert>,
    ) -> Result<AssertionId, Error> {
        let occurrence = self
            .statement_builder()
            .find(OccurrenceKind::StaticAssert, assertion)?
            .ok_or_else(|| {
                Error::new(
                    assertion.span.start,
                    "static assertion has no retained occurrence",
                )
            })?;
        self.statement_builder()
            .statement_builder
            .assertions
            .get(&occurrence)
            .copied()
            .ok_or_else(|| Error::new(assertion.span.start, "static assertion was not retained"))
    }

    fn retained_block(
        &mut self,
        items: &[Node<ast::BlockItem>],
        offset: usize,
    ) -> Result<Vec<BlockItem>, Error> {
        self.statement_builder().budget.charge(
            0,
            items.len(),
            items.len() * std::mem::size_of::<BlockItem>(),
            offset,
        )?;
        let mut retained = Vec::with_capacity(items.len());
        for item in items {
            retained.push(match &item.node {
                ast::BlockItem::Declaration(declaration) => {
                    BlockItem::Declaration(self.statement_builder().declaration_group(declaration)?)
                }
                ast::BlockItem::Statement(statement) => {
                    BlockItem::Statement(self.statement_builder().statement_id(statement)?)
                }
                ast::BlockItem::StaticAssert(assertion) => {
                    BlockItem::Assertion(self.retained_assertion(assertion)?)
                }
            });
        }
        Ok(retained)
    }

    pub(crate) fn retain_statement(
        &mut self,
        statement: &Node<ast::Statement>,
        id: StatementId,
    ) -> Result<(), Error> {
        let offset = statement.span.start;
        let kind = match &statement.node {
            ast::Statement::Compound(items) => {
                StatementKind::Block(self.retained_block(items, offset)?)
            }
            ast::Statement::Expression(expression) => StatementKind::Expression(
                expression
                    .as_ref()
                    .map(|expression| self.retained_value(expression))
                    .transpose()?,
            ),
            ast::Statement::Return(expression) => {
                let return_type = self
                    .current_function_signature()
                    .ok_or_else(|| Error::new(offset, "retained return has no function"))?
                    .return_type
                    .clone();
                let return_type = self.unqualified(&return_type)?;
                StatementKind::Return(
                    expression
                        .as_ref()
                        .map(|expression| {
                            self.retained_use(
                                expression,
                                UseContext::Value,
                                Some((return_type.clone(), Conversion::Assignment)),
                            )
                        })
                        .transpose()?,
                )
            }
            ast::Statement::Attribute(_) => StatementKind::Fallthrough {
                switch: self
                    .statement_builder()
                    .control_target(Some(ControlKind::Switch), offset)?,
            },
            ast::Statement::If(selection) => StatementKind::If {
                condition: self.retained_value(&selection.node.condition)?,
                then_statement: self
                    .statement_builder()
                    .statement_id(&selection.node.then_statement)?,
                else_statement: selection
                    .node
                    .else_statement
                    .as_ref()
                    .map(|statement| self.statement_builder().statement_id(statement))
                    .transpose()?,
            },
            ast::Statement::While(iteration) => StatementKind::While {
                condition: self.retained_value(&iteration.node.expression)?,
                body: self
                    .statement_builder()
                    .statement_id(&iteration.node.statement)?,
            },
            ast::Statement::DoWhile(iteration) => StatementKind::DoWhile {
                body: self
                    .statement_builder()
                    .statement_id(&iteration.node.statement)?,
                condition: self.retained_value(&iteration.node.expression)?,
            },
            ast::Statement::For(iteration) => {
                let initializer = match &iteration.node.initializer.node {
                    ast::ForInitializer::Empty => ForInitializer::Empty,
                    ast::ForInitializer::Expression(expression) => {
                        ForInitializer::Expression(self.retained_value(expression)?)
                    }
                    ast::ForInitializer::Declaration(declaration) => ForInitializer::Declaration(
                        self.statement_builder().declaration_group(declaration)?,
                    ),
                    ast::ForInitializer::StaticAssert(assertion) => {
                        ForInitializer::Assertion(self.retained_assertion(assertion)?)
                    }
                };
                StatementKind::For {
                    initializer,
                    condition: iteration
                        .node
                        .condition
                        .as_ref()
                        .map(|expression| self.retained_value(expression))
                        .transpose()?,
                    step: iteration
                        .node
                        .step
                        .as_ref()
                        .map(|expression| self.retained_value(expression))
                        .transpose()?,
                    body: self
                        .statement_builder()
                        .statement_id(&iteration.node.statement)?,
                }
            }
            ast::Statement::Switch(selection) => {
                let info = self.expression_info(&selection.node.expression)?;
                let promoted = integer_to_type(self.promoted_integer(&info, offset)?);
                StatementKind::Switch {
                    expression: self.retained_use(
                        &selection.node.expression,
                        UseContext::Value,
                        Some((promoted, Conversion::IntegerPromotion)),
                    )?,
                    body: self
                        .statement_builder()
                        .statement_id(&selection.node.statement)?,
                }
            }
            ast::Statement::Labeled(labeled) => {
                let label = match &labeled.node.label.node {
                    ast::Label::Identifier(identifier) => {
                        let builder = self.statement_builder();
                        builder
                            .budget
                            .charge(0, 1, identifier.node.name.len() * 2, offset)?;
                        builder
                            .statement_builder
                            .function
                            .as_mut()
                            .ok_or_else(|| Error::new(offset, "retained label has no function"))?
                            .labels
                            .insert(identifier.node.name.clone(), id);
                        Label::Identifier(identifier.node.name.clone())
                    }
                    ast::Label::Case(expression) => {
                        let switch_type = self.retained_switch_integer_type(offset)?;
                        Label::Case {
                            expression: self.retained_use(
                                expression,
                                UseContext::Unevaluated,
                                Some((integer_to_type(switch_type), Conversion::Assignment)),
                            )?,
                            value: convert(self.eval(expression)?, switch_type),
                            switch: self
                                .statement_builder()
                                .control_target(Some(ControlKind::Switch), offset)?,
                        }
                    }
                    ast::Label::CaseRange(range) => {
                        let switch_type = self.retained_switch_integer_type(offset)?;
                        Label::CaseRange {
                            low: self.retained_use(
                                &range.node.low,
                                UseContext::Unevaluated,
                                Some((integer_to_type(switch_type), Conversion::Assignment)),
                            )?,
                            high: self.retained_use(
                                &range.node.high,
                                UseContext::Unevaluated,
                                Some((integer_to_type(switch_type), Conversion::Assignment)),
                            )?,
                            low_value: convert(self.eval(&range.node.low)?, switch_type),
                            high_value: convert(self.eval(&range.node.high)?, switch_type),
                            switch: self
                                .statement_builder()
                                .control_target(Some(ControlKind::Switch), offset)?,
                        }
                    }
                    ast::Label::Default => Label::Default {
                        switch: self
                            .statement_builder()
                            .control_target(Some(ControlKind::Switch), offset)?,
                    },
                };
                StatementKind::Labeled {
                    occurrence: self
                        .statement_builder()
                        .find(OccurrenceKind::Label, &labeled.node.label)?
                        .ok_or_else(|| Error::new(offset, "label has no retained occurrence"))?,
                    label,
                    statement: self
                        .statement_builder()
                        .statement_id(&labeled.node.statement)?,
                }
            }
            ast::Statement::Goto(identifier) => {
                let builder = self.statement_builder();
                builder
                    .budget
                    .charge(0, 1, identifier.node.name.len(), offset)?;
                builder
                    .statement_builder
                    .function
                    .as_mut()
                    .ok_or_else(|| Error::new(offset, "retained goto has no function"))?
                    .gotos
                    .push(id);
                StatementKind::Goto {
                    name: identifier.node.name.clone(),
                    target: None,
                }
            }
            ast::Statement::Break => StatementKind::Break {
                target: self.statement_builder().control_target(None, offset)?,
            },
            ast::Statement::Continue => StatementKind::Continue {
                target: self
                    .statement_builder()
                    .control_target(Some(ControlKind::Loop), offset)?,
            },
            ast::Statement::Asm(_) => StatementKind::Assembly(
                self.statement_builder()
                    .statement_builder
                    .assemblies
                    .remove(&id)
                    .ok_or_else(|| Error::new(offset, "checked asm operands were not retained"))?,
            ),
        };
        self.statement_builder().budget.charge(0, 4, 0, offset)?;
        self.statement_builder().complete_statement(id, kind)
    }
}

impl Analyzer {
    fn retained_assembly_text(
        &mut self,
        literal: &Node<ast::StringLiteral>,
    ) -> Result<AssemblyText, Error> {
        let value = self.asm_string(literal)?;
        let occurrence = self
            .statement_builder()
            .find(OccurrenceKind::StringLiteral, literal)?
            .ok_or_else(|| Error::new(literal.span.start, "asm text has no retained occurrence"))?;
        Ok(AssemblyText { occurrence, value })
    }

    pub(crate) fn retain_assembly(
        &mut self,
        statement: &Node<ast::AsmStatement>,
        checked: &[crate::asm::Operand],
    ) -> Result<(), Error> {
        let offset = statement.span.start;
        let assembly = match &statement.node {
            ast::AsmStatement::GnuBasic(template) => Assembly {
                template: self.retained_assembly_text(template)?,
                basic: true,
                volatile: true,
                outputs: Vec::new(),
                inputs: Vec::new(),
                clobbers: Vec::new(),
            },
            ast::AsmStatement::GnuExtended(assembly) => {
                let mut outputs = Vec::with_capacity(assembly.outputs.len());
                let mut inputs = Vec::with_capacity(assembly.inputs.len());
                for (index, (operand, facts)) in assembly
                    .outputs
                    .iter()
                    .chain(&assembly.inputs)
                    .zip(checked)
                    .enumerate()
                {
                    let output = index < assembly.outputs.len();
                    let mut alternatives = Vec::with_capacity(facts.constraint.alternatives.len());
                    let mut value = false;
                    let mut place = output;
                    for (alternative, location) in facts.constraint.alternatives.iter().enumerate()
                    {
                        let effective = location.matching.map_or(location, |index| {
                            &checked[index].constraint.alternatives[alternative]
                        });
                        value |= effective.register || effective.immediate;
                        place |= effective.memory;
                        alternatives.push(AssemblyLocation {
                            register: location.register,
                            memory: location.memory,
                            immediate: location.immediate,
                            fixed: location
                                .fixed
                                .iter()
                                .map(|register| (*register).to_owned())
                                .collect(),
                            matching: location.matching,
                        });
                    }
                    let retained = AssemblyOperand {
                        occurrence: self
                            .statement_builder()
                            .find(OccurrenceKind::AsmOperand, operand)?
                            .ok_or_else(|| {
                                Error::new(
                                    operand.span.start,
                                    "asm operand has no retained occurrence",
                                )
                            })?,
                        name: operand
                            .node
                            .symbolic_name
                            .as_ref()
                            .map(|name| name.node.name.clone()),
                        constraints: self.retained_assembly_text(&operand.node.constraints)?,
                        read_write: facts.constraint.read_write,
                        integer_constant: facts.integer_constant,
                        alternatives,
                        place: if place {
                            Some(self.retained_use(
                                &operand.node.variable_name,
                                UseContext::Place,
                                None,
                            )?)
                        } else {
                            None
                        },
                        value: if value && (!output || facts.constraint.read_write) {
                            Some(self.retained_value(&operand.node.variable_name)?)
                        } else {
                            None
                        },
                    };
                    if output {
                        outputs.push(retained);
                    } else {
                        inputs.push(retained);
                    }
                }
                let clobbers = assembly
                    .clobbers
                    .iter()
                    .map(|clobber| self.retained_assembly_text(clobber))
                    .collect::<Result<Vec<_>, _>>()?;
                Assembly {
                    template: self.retained_assembly_text(&assembly.template)?,
                    basic: false,
                    volatile: assembly.qualifier.is_some() || assembly.outputs.is_empty(),
                    outputs,
                    inputs,
                    clobbers,
                }
            }
        };
        let mut payload = assembly.clobbers.len() * std::mem::size_of::<AssemblyText>()
            + assembly.template.value.len()
            + assembly
                .clobbers
                .iter()
                .map(|clobber| clobber.value.len())
                .sum::<usize>();
        let mut edges = assembly.clobbers.len();
        for operand in assembly.outputs.iter().chain(&assembly.inputs) {
            payload += std::mem::size_of::<AssemblyOperand>()
                + operand.name.as_ref().map_or(0, String::len)
                + operand.constraints.value.len()
                + operand.alternatives.len() * std::mem::size_of::<AssemblyLocation>();
            edges += 2 + operand.alternatives.len();
            for location in &operand.alternatives {
                edges += location.fixed.len();
                payload += location.fixed.len() * std::mem::size_of::<String>()
                    + location.fixed.iter().map(String::len).sum::<usize>();
            }
        }
        let builder = self.statement_builder();
        builder.budget.charge(0, edges + 1, payload, offset)?;
        let id = builder
            .statement_builder
            .active
            .last()
            .copied()
            .ok_or_else(|| Error::new(offset, "assembly has no retained statement"))?;
        builder.statement_builder.assemblies.insert(id, assembly);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IntegerKind;
    use crate::analyze::analyze_inner;
    use crate::checked::expression::{Coverage as ExpressionCoverage, ExprKind};
    use crate::checked::{CheckedCode, Limits};
    use toucan_target::Target;

    fn checked(source: &str, target: Target) -> CheckedCode {
        let (retained, code) = analyze_inner(source, target, Some(Limits::default()))
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        let plain = crate::analyze(source, target).unwrap();
        assert_eq!(format!("{plain:?}"), format!("{retained:?}"));
        let code = code.unwrap();
        assert!(code.ambiguous_aliases.is_empty());
        assert!(
            !code
                .expression_coverage
                .iter()
                .any(|coverage| matches!(coverage.status, ExpressionCoverage::Missing))
        );
        assert!(
            !code
                .statement_coverage
                .iter()
                .any(|coverage| matches!(coverage.status, Coverage::Missing))
        );
        assert_eq!(
            code.statements.len(),
            code.occurrences
                .iter()
                .filter(|occurrence| occurrence.kind == OccurrenceKind::Statement)
                .count()
        );
        assert!(code.statements.iter().all(|statement| !matches!(
            statement.kind,
            StatementKind::Checking | StatementKind::Goto { target: None, .. }
        )));
        code
    }

    #[test]
    fn function_roots_keep_definition_parameters_ordered_items_and_return_uses() {
        for target in Target::ALL {
            let code = checked(
                "double f(int prototype); double f(int actual) { int x = actual; _Static_assert(1, \"ok\"); { long x = 2; x++; } return x; }",
                target,
            );
            assert_eq!(code.bodies.len(), 1);
            let body = &code.bodies[0];
            assert_eq!(
                code.entities[body.entity.index()].name.as_deref(),
                Some("f")
            );
            assert_eq!(code.entities[body.entity.index()].body, Some(BodyId(0)));
            assert_eq!(
                code.declarations[body.declaration.index()].body,
                Some(BodyId(0))
            );
            assert_eq!(body.parameters.len(), 1);
            let parameter = &code.declarations[body.parameters[0].index()];
            assert_eq!(
                code.entities[parameter.entity.index()].name.as_deref(),
                Some("actual")
            );
            let TypeKind::Function(signature) = &code.types[body.signature.index()].kind else {
                panic!("function signature");
            };
            assert_eq!(signature.parameters[0].name.as_deref(), Some("actual"));
            let StatementKind::Block(items) = &code.statements[body.statement.index()].kind else {
                panic!("function block");
            };
            assert_eq!(items.len(), 4);
            let BlockItem::Declaration(group) = items[0] else {
                panic!("declaration");
            };
            assert_eq!(
                code.declaration_groups[group.0 as usize].declarations.len(),
                1
            );
            assert!(matches!(items[1], BlockItem::Assertion(_)));
            assert!(matches!(items[2], BlockItem::Statement(_)));
            let BlockItem::Statement(return_id) = items[3] else {
                panic!("return statement");
            };
            let StatementKind::Return(Some(value)) = &code.statements[return_id.index()].kind
            else {
                panic!("return value");
            };
            assert_eq!(
                value
                    .conversions
                    .iter()
                    .map(|step| step.kind)
                    .collect::<Vec<_>>(),
                [Conversion::Lvalue, Conversion::Assignment]
            );
            assert_eq!(
                code.types[value.effective_type.index()].kind,
                TypeKind::Float(crate::FloatKind::Double)
            );
        }
    }

    #[test]
    fn jumps_resolve_to_nested_loop_switch_and_forward_label_targets() {
        let source = "int f(short x) { int sum=0; for(int i=0;i<3;i++) { while(x) { switch(x) { case -1: x=0; continue; case 1 ... 3: break; default: goto done; } break; } if(i) continue; } done: return sum; }";
        let code = checked(source, Target::X86_64UnknownLinuxGnu);
        let find = |predicate: fn(&StatementKind) -> bool| {
            code.statements
                .iter()
                .position(|statement| predicate(&statement.kind))
                .map(|index| StatementId(index as u32))
                .unwrap()
        };
        let for_id = find(|kind| matches!(kind, StatementKind::For { .. }));
        let while_id = find(|kind| matches!(kind, StatementKind::While { .. }));
        let switch_id = find(|kind| matches!(kind, StatementKind::Switch { .. }));
        let continues: Vec<_> = code
            .statements
            .iter()
            .filter_map(|statement| match statement.kind {
                StatementKind::Continue { target } => Some(target),
                _ => None,
            })
            .collect();
        assert_eq!(continues, [while_id, for_id]);
        let breaks: Vec<_> = code
            .statements
            .iter()
            .filter_map(|statement| match statement.kind {
                StatementKind::Break { target } => Some(target),
                _ => None,
            })
            .collect();
        assert_eq!(breaks, [switch_id, while_id]);
        let goto_target = code
            .statements
            .iter()
            .find_map(|statement| match statement.kind {
                StatementKind::Goto { target, .. } => target,
                _ => None,
            })
            .unwrap();
        assert!(
            matches!(&code.statements[goto_target.index()].kind, StatementKind::Labeled {label:Label::Identifier(name), ..} if name=="done")
        );
        let StatementKind::Switch { expression, .. } = &code.statements[switch_id.index()].kind
        else {
            unreachable!()
        };
        assert_eq!(
            code.types[expression.effective_type.index()].kind,
            TypeKind::Integer(IntegerKind::Int)
        );
        let range = code
            .statements
            .iter()
            .find_map(|statement| match &statement.kind {
                StatementKind::Labeled {
                    label:
                        Label::CaseRange {
                            low_value,
                            high_value,
                            switch,
                            ..
                        },
                    ..
                } => Some((low_value, high_value, switch)),
                _ => None,
            })
            .unwrap();
        assert_eq!(range.0.signed_value(), 1);
        assert_eq!(range.1.signed_value(), 3);
        assert_eq!(*range.2, switch_id);
    }

    #[test]
    fn condition_statement_expression_jumps_follow_the_target_profile() {
        for target in Target::ALL {
            let code = checked(
                "void f(int x) { while(x) { while(({ break; x; })) {} } }",
                target,
            );
            let loops: Vec<_> = code
                .statements
                .iter()
                .enumerate()
                .filter(|(_, statement)| matches!(statement.kind, StatementKind::While { .. }))
                .map(|(index, _)| StatementId(index as u32))
                .collect();
            let target_id = code
                .statements
                .iter()
                .find_map(|statement| match statement.kind {
                    StatementKind::Break { target } => Some(target),
                    _ => None,
                })
                .unwrap();
            let gnu = matches!(
                target,
                Target::I686UnknownLinuxGnu
                    | Target::X86_64UnknownLinuxGnu
                    | Target::X86_64UnknownLinuxMusl
                    | Target::Aarch64UnknownLinuxGnu
                    | Target::Aarch64UnknownLinuxMusl
            );
            assert_eq!(target_id, loops[usize::from(!gnu)]);
        }
    }

    #[test]
    fn assertions_and_unevaluated_bodies_survive_local_scope_exit() {
        let code = checked(
            "_Static_assert(1, \"global\"); int f(void) { enum { VALUE=2 }; _Static_assert(VALUE, \"outer\"); return sizeof(({ ; enum { VALUE=3 }; _Static_assert(VALUE==3, \"inner\"); VALUE; ; })); }",
            Target::X86_64UnknownLinuxGnu,
        );
        assert_eq!(code.assertions.len(), 3);
        assert_eq!(code.assertions[1].value.signed_value(), 2);
        assert_eq!(code.assertions[2].value.signed_value(), 1);
        let operand = code
            .expressions
            .iter()
            .find_map(|expression| match &expression.kind {
                ExprKind::SizeOfValue { operand, .. } => Some(operand),
                _ => None,
            })
            .unwrap();
        assert_eq!(operand.context, UseContext::Unevaluated);
        let ExprKind::StatementExpression { body, result } =
            &code.expressions[operand.expression.0 as usize].kind
        else {
            panic!("statement expression");
        };
        assert!(result.is_some());
        let StatementKind::Block(items) = &code.statements[body.index()].kind else {
            panic!("statement-expression block");
        };
        assert_eq!(items.len(), 5);
        assert!(matches!(items[0], BlockItem::Statement(_)));
        assert!(matches!(items[2], BlockItem::Assertion(_)));
    }

    #[test]
    fn asm_retains_constraints_memory_places_matching_values_and_clobbers() {
        let source = "int f(int x) {int y; __asm__ volatile(\"\" : \"=r\"(y) : \"0\"(x) : \"cc\"); __asm__(\"\" : \"+rm\"(x) : \"m\"(y) : \"memory\"); __asm__(\"nop\"); return x;}";
        for target in Target::ALL
            .into_iter()
            .filter(|target| !target.is_windows())
        {
            if target.is_armv7() {
                for source in [source, "void f(void) { __asm__(\"nop\"); }"] {
                    let plain = crate::analyze(source, target).unwrap_err();
                    let retained =
                        analyze_inner(source, target, Some(Limits::default())).unwrap_err();
                    assert_eq!(
                        plain.message,
                        "GNU inline assembly is unsupported for this target"
                    );
                    assert_eq!(
                        (plain.offset, plain.message),
                        (retained.offset, retained.message)
                    );
                }
                continue;
            }
            let code = checked(source, target);
            let assemblies: Vec<_> = code
                .statements
                .iter()
                .filter_map(|statement| match &statement.kind {
                    StatementKind::Assembly(assembly) => Some(assembly),
                    _ => None,
                })
                .collect();
            assert_eq!(assemblies.len(), 3);
            assert!(assemblies[0].volatile);
            assert!(!assemblies[1].volatile);
            assert!(assemblies[2].basic && assemblies[2].volatile);
            assert!(assemblies[0].outputs[0].place.is_some());
            assert!(assemblies[0].outputs[0].value.is_none());
            assert_eq!(assemblies[0].inputs[0].alternatives[0].matching, Some(0));
            assert!(assemblies[0].inputs[0].value.is_some());
            assert!(assemblies[0].inputs[0].place.is_none());
            assert!(assemblies[1].outputs[0].read_write);
            assert!(
                assemblies[1].outputs[0].place.is_some()
                    && assemblies[1].outputs[0].value.is_some()
            );
            assert!(assemblies[1].inputs[0].place.is_some());
            assert!(assemblies[1].inputs[0].value.is_none());
            assert_eq!(assemblies[1].clobbers[0].value, "memory");
            let constraint = &assemblies[0].inputs[0].constraints;
            assert_eq!(
                &source[code.occurrences[constraint.occurrence.index()]
                    .source
                    .range
                    .clone()],
                "\"0\""
            );
            let clobber = &assemblies[1].clobbers[0];
            assert_eq!(
                &source[code.occurrences[clobber.occurrence.index()]
                    .source
                    .range
                    .clone()],
                "\"memory\""
            );
        }
    }

    #[test]
    fn declaration_groups_do_not_absorb_initializer_body_locals() {
        let code = checked(
            "int f(void) { int x = ({ int y = 1; y; }); return x; }",
            Target::X86_64UnknownLinuxGnu,
        );
        let mut names = Vec::new();
        for group in &code.declaration_groups {
            assert_eq!(group.declarations.len(), 1);
            names.push(
                code.entities[code.declarations[group.declarations[0].index()]
                    .entity
                    .index()]
                .name
                .as_deref()
                .unwrap(),
            );
        }
        assert_eq!(names, ["y", "x"]);
        let StatementKind::Block(items) = &code.statements[code.bodies[0].statement.index()].kind
        else {
            panic!("body");
        };
        assert!(matches!(
            items[0],
            BlockItem::Declaration(DeclarationGroupId(1))
        ));
    }

    #[test]
    fn invalid_flow_keeps_diagnostics_and_body_payload_is_bounded() {
        for source in [
            "void f(int n) { goto label; int a[n]; label: ; }",
            "void f(void) { goto absent; }",
            "void f(void) { continue; }",
            "void f(void) { switch(1) {case 1:;case 1:;} }",
        ] {
            let plain = crate::analyze(source, Target::X86_64UnknownLinuxGnu).unwrap_err();
            let retained = analyze_inner(
                source,
                Target::X86_64UnknownLinuxGnu,
                Some(Limits::default()),
            )
            .unwrap_err();
            assert_eq!(plain.to_string(), retained.to_string());
        }
        let source = format!("void f(void) {{ __asm__(\"{}\"); }}", " ".repeat(2048));
        checked(&source, Target::X86_64UnknownLinuxGnu);
        let error = analyze_inner(
            &source,
            Target::X86_64UnknownLinuxGnu,
            Some(Limits {
                payload_bytes: 1024,
                ..Limits::default()
            }),
        )
        .unwrap_err();
        assert!(error.message.contains("retention payload byte limit"));
        assert_eq!(error.offset, source.find("__asm__").unwrap());
    }

    #[test]
    #[ignore = "requires native GCC and Clang; run with --include-ignored"]
    fn retained_control_flow_matches_native_execution() {
        let body = r#"
            int run(int stop) {
                int total=0;
                for(int i=0;i<5;i++) {
                    if(i==1) continue;
                    int x=0;
                    do { x++; if(x==2) continue; total+=i+x; } while(x<3);
                    switch(i) { case 0: break; case 2: if(stop) goto done; break; default: total+=10; }
                }
            done: return total;
            }
            int profile(void) {
                int x=0;
                while(x<3) { while(({ x++; break; 1; })) {} x+=10; }
                return x;
            }
        "#;
        let directory = tempfile::tempdir().unwrap();
        for compiler in ["gcc", "clang"] {
            let version = std::process::Command::new(compiler)
                .arg("--version")
                .output()
                .unwrap();
            assert!(version.status.success());
            let gnu = String::from_utf8_lossy(&version.stdout).contains("Free Software Foundation");
            let target = if gnu {
                Target::X86_64UnknownLinuxGnu
            } else {
                Target::X86_64AppleDarwin
            };
            // This oracle selects only the GNU/Clang control-flow profile; it
            // contains no assertions about the native ABI or plain-char types.
            let expected = if gnu { 1 } else { 11 };
            let source = format!(
                "{body}\nint main(void) {{ return run(0)!=54 || run(1)!=12 || profile()!={expected}; }}\n"
            );
            let code = checked(&source, target);
            let profile = code
                .bodies
                .iter()
                .find(|body| code.entities[body.entity.index()].name.as_deref() == Some("profile"))
                .unwrap();
            let profile_range = &code.occurrences[code.declarations[profile.declaration.index()]
                .occurrence
                .index()]
            .source
            .range;
            let loops: Vec<_> = code
                .statements
                .iter()
                .enumerate()
                .filter(|(_, statement)| {
                    matches!(statement.kind, StatementKind::While { .. })
                        && profile_range.contains(
                            &code.occurrences[statement.occurrence.index()]
                                .source
                                .range
                                .start,
                        )
                })
                .map(|(index, _)| StatementId(index as u32))
                .collect();
            let jump = code
                .statements
                .iter()
                .find_map(|statement| match statement.kind {
                    StatementKind::Break { target }
                        if profile_range.contains(
                            &code.occurrences[statement.occurrence.index()]
                                .source
                                .range
                                .start,
                        ) =>
                    {
                        Some(target)
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(jump, loops[usize::from(!gnu)]);
            let input = directory.path().join(format!("{compiler}.c"));
            let executable = directory.path().join(format!("{compiler}.exe"));
            std::fs::write(&input, &source).unwrap();
            let output = std::process::Command::new(compiler)
                .args(["-std=gnu11", "-O2"])
                .arg(&input)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                std::process::Command::new(executable)
                    .status()
                    .unwrap()
                    .success(),
                "{compiler}"
            );
        }
    }
}

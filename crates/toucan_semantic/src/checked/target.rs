//! Written target attributes and feature-sensitive call facts.

use lang_c::{ast, span::Span};
use serde::Serialize;
use toucan_target::Compiler;

use super::{BodyId, Builder, CheckedCode, EntityId, ExprId, ExprKind, SiteId, SourceSpan};
use crate::analyze::Analyzer;
use crate::target_features::{ParsedMinimumVectorWidth, ParsedTarget};
use crate::{Error, FunctionOptions};

/// One written target annotation, including compiler-significant argument order.
#[derive(Debug, Serialize)]
pub struct TargetAttribute {
    arguments: Vec<String>,
    source: SourceSpan,
}
impl TargetAttribute {
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
    pub fn source(&self) -> &SourceSpan {
        &self.source
    }
}

/// One written Clang width hint; duplicate hints remain in source order.
#[derive(Debug, Serialize)]
pub struct MinimumVectorWidthAttribute {
    value: u32,
    source: SourceSpan,
}
impl MinimumVectorWidthAttribute {
    /// Width requested in bits, after Clang's unsigned argument interpretation.
    pub fn value(&self) -> u32 {
        self.value
    }
    pub fn source(&self) -> &SourceSpan {
        &self.source
    }
}

/// Options visible at one declaration; an entity may have later declarations.
#[derive(Debug, Serialize)]
pub struct FunctionOptionSite {
    declaration: SiteId,
    entity: EntityId,
    effective: FunctionOptions,
    attributes: Vec<TargetAttribute>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    minimum_vector_width: Vec<MinimumVectorWidthAttribute>,
    always_inline: Option<SourceSpan>,
    no_inline: Option<SourceSpan>,
}
impl FunctionOptionSite {
    pub fn declaration(&self) -> SiteId {
        self.declaration
    }
    pub fn entity(&self) -> EntityId {
        self.entity
    }
    pub fn effective(&self) -> &FunctionOptions {
        &self.effective
    }
    pub fn attributes(&self) -> &[TargetAttribute] {
        &self.attributes
    }
    /// Written width hints, including duplicates and annotations ignored after a definition.
    pub fn minimum_vector_width(&self) -> &[MinimumVectorWidthAttribute] {
        &self.minimum_vector_width
    }
    /// Present for an explicitly written always_inline, not inherited occurrences.
    pub fn always_inline(&self) -> Option<&SourceSpan> {
        self.always_inline.as_ref()
    }
    /// Present for an explicitly written noinline, not inherited occurrences.
    pub fn no_inline(&self) -> Option<&SourceSpan> {
        self.no_inline.as_ref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum InlineTargetStage {
    /// Clang checks attributes visible on the directly named callee during code generation.
    CodeGeneration,
    /// GNU inlining can use declarations encountered after the call was typed.
    AfterInlining,
}

/// A feature check associated with inlining, not with ordinary out-of-line calls.
/// The source expression's evaluation context still decides whether this call runs.
#[derive(Debug, Serialize)]
pub struct InlineTargetRequirement {
    expression: ExprId,
    callee: EntityId,
    caller_options: FunctionOptions,
    callee_options: FunctionOptions,
    stage: InlineTargetStage,
    definition_visible: bool,
}
impl InlineTargetRequirement {
    pub fn expression(&self) -> ExprId {
        self.expression
    }
    pub fn callee(&self) -> EntityId {
        self.callee
    }
    pub fn caller_options(&self) -> &FunctionOptions {
        &self.caller_options
    }
    pub fn callee_options(&self) -> &FunctionOptions {
        &self.callee_options
    }
    pub fn stage(&self) -> InlineTargetStage {
        self.stage
    }
    /// Whether this analysis has a definition; false leaves body availability unresolved.
    pub fn definition_visible(&self) -> bool {
        self.definition_visible
    }
}

impl CheckedCode {
    /// Source declarations with target, inline, or minimum vector width properties.
    pub fn function_option_sites(&self) -> impl Iterator<Item = &FunctionOptionSite> {
        self.function_options.values()
    }
    pub fn function_options(&self, site: SiteId) -> Option<&FunctionOptionSite> {
        self.function_options.get(&site.index())
    }
    /// The last effective declaration of this function entity.
    pub fn entity_function_options(&self, entity: EntityId) -> Option<&FunctionOptions> {
        self.function_option_entities
            .get(&entity.index())
            .and_then(|site| self.function_options.get(site))
            .map(|site| &site.effective)
    }
    pub fn body_function_options(&self, body: BodyId) -> Option<&FunctionOptions> {
        self.bodies
            .get(body.index())
            .and_then(|body| self.entity_function_options(body.entity))
    }
    pub fn inline_target_requirements(&self) -> impl Iterator<Item = &InlineTargetRequirement> {
        self.inline_targets.values()
    }
    pub fn inline_target_requirement(
        &self,
        expression: ExprId,
    ) -> Option<&InlineTargetRequirement> {
        self.inline_targets.get(&expression.index())
    }
}

impl Builder {
    pub(crate) fn attach_function_options(
        &mut self,
        site: SiteId,
        options: Option<&FunctionOptions>,
        written: (&[ParsedTarget], &[ParsedMinimumVectorWidth]),
        always_inline: Option<(Span, bool)>,
        no_inline: Option<(Span, bool)>,
        affects_entity: bool,
    ) -> Result<(), Error> {
        let (attributes, widths) = written;
        let Some(options) = options.filter(|options| {
            !options.is_default()
                || !attributes.is_empty()
                || !widths.is_empty()
                || always_inline.is_some()
                || no_inline.is_some()
        }) else {
            return Ok(());
        };
        let entity = self.code.declarations[site.index()].entity;
        let offset = self.code.occurrences[self.code.declarations[site.index()].occurrence.index()]
            .source
            .range
            .start;
        let bytes = attributes
            .iter()
            .map(|attribute| {
                attribute.arguments.iter().map(String::len).sum::<usize>()
                    + attribute.arguments.len() * std::mem::size_of::<String>()
            })
            .sum::<usize>()
            + option_bytes(options)
            + widths.len() * std::mem::size_of::<MinimumVectorWidthAttribute>();
        self.budget
            .charge(1 + attributes.len() + widths.len(), 2, bytes, offset)?;
        let mut attributes = attributes
            .iter()
            .map(|attribute| {
                Ok(TargetAttribute {
                    arguments: attribute.arguments.clone(),
                    source: self.budget.source_span(attribute.span)?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        attributes.sort_by_key(|attribute| attribute.source.range.start);
        let mut minimum_vector_width = widths
            .iter()
            .map(|attribute| {
                Ok(MinimumVectorWidthAttribute {
                    value: attribute.value,
                    source: self.budget.source_span(attribute.span)?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        minimum_vector_width.sort_by_key(|attribute| attribute.source.range.start);
        self.code.function_options.insert(
            site.index(),
            FunctionOptionSite {
                declaration: site,
                entity,
                effective: options.clone(),
                attributes,
                minimum_vector_width,
                always_inline: always_inline
                    .map(|(span, _)| self.budget.source_span(span))
                    .transpose()?,
                no_inline: no_inline
                    .map(|(span, _)| self.budget.source_span(span))
                    .transpose()?,
            },
        );
        if affects_entity {
            self.code
                .function_option_entities
                .insert(entity.index(), site.index());
        }
        Ok(())
    }

    fn attach_inline_target(
        &mut self,
        requirement: InlineTargetRequirement,
        offset: usize,
    ) -> Result<(), Error> {
        self.budget.charge(
            1,
            2,
            option_bytes(&requirement.caller_options) + option_bytes(&requirement.callee_options),
            offset,
        )?;
        self.code
            .inline_targets
            .insert(requirement.expression.index(), requirement);
        Ok(())
    }

    pub(super) fn finish_function_options(&self) -> Result<(), Error> {
        for (&index, options) in &self.code.function_options {
            if index != options.declaration.index()
                || self
                    .code
                    .declarations
                    .get(index)
                    .is_none_or(|site| site.entity != options.entity)
                || self
                    .code
                    .entities
                    .get(options.entity.index())
                    .is_none_or(|entity| entity.kind != super::EntityKind::Function)
            {
                return Err(Error::new(
                    0,
                    "invalid retained function option declaration",
                ));
            }
        }
        for (&entity, &site) in &self.code.function_option_entities {
            if self
                .code
                .function_options
                .get(&site)
                .is_none_or(|options| options.entity.index() != entity)
            {
                return Err(Error::new(0, "invalid retained function option entity"));
            }
        }
        for (&index, requirement) in &self.code.inline_targets {
            if index != requirement.expression.index()
                || self.code.expressions.get(index).is_none_or(|expression| !matches!(&expression.kind, ExprKind::Call { direct_callee: Some(callee), .. } if *callee == requirement.callee))
                || self.code.entities.get(requirement.callee.index()).is_none_or(|entity| entity.kind != super::EntityKind::Function)
            {
                return Err(Error::new(0, "invalid retained inline target requirement"));
            }
        }
        Ok(())
    }
}

fn option_bytes(options: &FunctionOptions) -> usize {
    options.target().map_or(0, |target| {
        std::mem::size_of_val(target.options()) + target.clang_spelling().map_or(0, str::len)
    })
}

impl Analyzer {
    pub(crate) fn retain_inline_target(
        &mut self,
        expression: &lang_c::span::Node<ast::Expression>,
    ) -> Result<(), Error> {
        if self.unit.compiler != Compiler::Clang {
            return Ok(());
        }
        let ast::Expression::Call(call) = &expression.node else {
            return Ok(());
        };
        let ast::Expression::Identifier(identifier) = &call.node.callee.node else {
            return Ok(());
        };
        let name = identifier.node.name.as_str();
        let Some(callee_options) = self
            .visible_function_options(name)
            .filter(|options| options.always_inline())
            .cloned()
        else {
            return Ok(());
        };
        let caller_options = self.current_function_options().cloned().unwrap_or_default();
        if caller_options.target().is_none() && callee_options.target().is_none() {
            return Ok(());
        }
        let builder = self.checked.as_mut().expect("retained expression");
        let id = builder.expression_id(expression)?;
        let ExprKind::Call {
            direct_callee: Some(callee),
            ..
        } = builder.code.expressions[id.index()].kind
        else {
            return Ok(());
        };
        let definition_visible = builder.code.entities[callee.index()]
            .declaration
            .is_some_and(|index| self.unit.declarations[index].is_definition);
        builder.attach_inline_target(
            InlineTargetRequirement {
                expression: id,
                callee,
                caller_options,
                callee_options,
                stage: InlineTargetStage::CodeGeneration,
                definition_visible,
            },
            expression.span.start,
        )
    }

    pub(crate) fn finish_inline_targets(&mut self) -> Result<(), Error> {
        if self.unit.compiler != Compiler::Gnu {
            return Ok(());
        }
        let Some(builder) = &mut self.checked else {
            return Ok(());
        };
        if builder.code.function_options.is_empty() {
            return Ok(());
        }
        let owners = builder
            .code
            .bodies
            .iter()
            .map(|body| (body.scope.index(), body.entity))
            .collect::<std::collections::BTreeMap<_, _>>();
        for index in 0..builder.code.expressions.len() {
            let expression = &builder.code.expressions[index];
            let ExprKind::Call {
                direct_callee: Some(callee),
                ..
            } = expression.kind
            else {
                continue;
            };
            let Some(callee_options) = builder
                .code
                .entity_function_options(callee)
                .filter(|options| options.always_inline())
                .cloned()
            else {
                continue;
            };
            let mut scope = Some(expression.scope);
            let mut owner = None;
            while let Some(current) = scope {
                if let Some(entity) = owners.get(&current.index()) {
                    owner = Some(*entity);
                    break;
                }
                scope = builder.code.scopes[current.index()].parent;
            }
            let caller_options = owner
                .and_then(|owner| builder.code.entity_function_options(owner))
                .cloned()
                .unwrap_or_default();
            if caller_options.target().is_none() && callee_options.target().is_none() {
                continue;
            }
            let definition_visible = builder.code.entities[callee.index()].body.is_some();
            let offset = builder.code.occurrences[expression.occurrence.index()]
                .source
                .range
                .start;
            builder.attach_inline_target(
                InlineTargetRequirement {
                    expression: ExprId(index as u32),
                    callee,
                    caller_options,
                    callee_options,
                    stage: InlineTargetStage::AfterInlining,
                    definition_visible,
                },
                offset,
            )?;
        }
        Ok(())
    }
}

//! Runtime dimensions belong to checked type uses, not canonical type shapes.
//! A bound is a static source site; `Required` never means unconditional execution.

use std::collections::{BTreeMap, HashMap, HashSet};

use lang_c::{
    ast,
    span::{Node, Span},
};
use serde::Serialize;

use super::expression::{Binary, Conversion, ExprId, ExprKind, Unary};
use super::{
    Builder, EntityId, EntityKind, OccurrenceId, OccurrenceKind, ScopeId, SourceSpan, TypeId,
    map_span, unmapped_span,
};
use crate::{Error, Type, TypeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct TypeUseId(pub(crate) u32);
impl TypeUseId {
    /// Returns the owner-local arena index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct BoundId(pub(crate) u32);
impl BoundId {
    /// Returns the owner-local arena index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[non_exhaustive]
pub enum TypeStep {
    Pointer,
    Element,
    Return,
    Parameter(usize),
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Extent {
    pub(crate) path: Vec<TypeStep>,
    pub(crate) bound: BoundId,
}
#[derive(Debug, Serialize)]
pub struct TypeUse {
    pub(crate) shape: TypeId,
    pub(crate) extents: Vec<Extent>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum BoundEvaluation {
    Constant,
    /// Required when the enclosing declaration or expression is evaluated.
    /// Conditional and short-circuit parents still govern whether it is reached.
    Required,
    Prototype,
    Unevaluated,
    MayBeOmitted,
    Derived,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum BoundSite {
    Declaration,
    FunctionEntry,
    Prototype,
    TypeName,
    Composite,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum BoundInput {
    Runtime(BoundId),
    Constant(u64),
    Unspecified,
}
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum BoundValue {
    Expression(ExprId),
    Constant {
        expression: ExprId,
        value: u64,
    },
    PrototypeStar,
    /// Compatibility does not prove that these runtime values are equal.
    Composite {
        inputs: Vec<BoundInput>,
        selection: Option<ExprId>,
    },
}
#[derive(Debug, Serialize)]
pub struct Bound {
    pub(crate) source: SourceSpan,
    pub(crate) owner: Option<OccurrenceId>,
    pub(crate) scope: ScopeId,
    pub(crate) site: BoundSite,
    pub(crate) minimum: bool,
    pub(crate) evaluation: BoundEvaluation,
    pub(crate) value: BoundValue,
}
pub(crate) struct BoundContext {
    pub(crate) prototype: bool,
    pub(crate) definition: bool,
    pub(crate) type_name: bool,
    pub(crate) minimum: bool,
    pub(crate) constant: Option<u64>,
}
#[derive(Default)]
pub(super) struct BoundsBuilder {
    plain: HashMap<TypeId, TypeUseId>,
    starts: HashMap<OccurrenceId, usize>,
    entities: HashMap<EntityId, TypeUseId>,
    pending: HashMap<(ScopeId, String), TypeUseId>,
    parameters: HashMap<OccurrenceId, TypeUseId>,
    type_names: HashMap<(usize, usize), TypeUseId>,
    spans: Vec<Span>,
}

impl Builder {
    pub(super) fn begin_bound_context(&mut self, occurrence: OccurrenceId) {
        self.bounds_builder
            .starts
            .insert(occurrence, self.code.bounds.len());
    }
    pub(super) fn finish_bound_context(
        &mut self,
        owner: OccurrenceId,
        kind: &ExprKind,
        type_name: Option<TypeUseId>,
    ) {
        let start = self
            .bounds_builder
            .starts
            .get(&owner)
            .copied()
            .unwrap_or(self.code.bounds.len());
        let sizeof_use = match kind {
            ExprKind::SizeOfType(_) => type_name,
            ExprKind::SizeOfValue { operand, .. } => Some(operand.type_use),
            _ => None,
        };
        if let Some(id) = sizeof_use {
            let mut required: Vec<_> = self.code.type_uses[id.index()]
                .extents
                .iter()
                .filter(|extent| extent.path.iter().all(|step| *step == TypeStep::Element))
                .map(|extent| extent.bound)
                .collect();
            let mut ranges = Vec::new();
            let mut seen: HashSet<_> = required.iter().copied().collect();
            let mut cursor = 0;
            while cursor < required.len() {
                let bound = &self.code.bounds[required[cursor].index()];
                match &bound.value {
                    BoundValue::Expression(expression) => ranges.push(
                        self.parsed_spans
                            [self.code.expressions[expression.index()].occurrence.index()],
                    ),
                    BoundValue::Composite { inputs, .. } => {
                        for input in inputs {
                            if let BoundInput::Runtime(id) = input
                                && seen.insert(*id)
                            {
                                required.push(*id);
                            }
                        }
                    }
                    BoundValue::PrototypeStar | BoundValue::Constant { .. } => {}
                }
                cursor += 1;
            }
            ranges.sort_unstable_by_key(|range| range.start);
            let mut merged: Vec<Span> = Vec::new();
            for range in ranges {
                if let Some(previous) = merged.last_mut()
                    && range.start <= previous.end
                {
                    previous.end = previous.end.max(range.end);
                } else {
                    merged.push(range);
                }
            }
            for (index, bound) in self.code.bounds.iter_mut().enumerate().skip(start) {
                if matches!(
                    bound.evaluation,
                    BoundEvaluation::Required | BoundEvaluation::MayBeOmitted
                ) {
                    let span = self.bounds_builder.spans[index];
                    let end = merged.partition_point(|range| range.start <= span.start);
                    let required_operand = bound.evaluation == BoundEvaluation::Required
                        && end != 0
                        && span.end <= merged[end - 1].end;
                    bound.evaluation = if seen.contains(&BoundId(index as u32)) || required_operand
                    {
                        BoundEvaluation::Required
                    } else {
                        BoundEvaluation::MayBeOmitted
                    };
                }
            }
        }
        if matches!(kind, ExprKind::AlignOf(_)) {
            for bound in &mut self.code.bounds[start..] {
                if matches!(
                    bound.evaluation,
                    BoundEvaluation::Required | BoundEvaluation::MayBeOmitted
                ) {
                    bound.evaluation = BoundEvaluation::Unevaluated;
                }
            }
        }
        if let ExprKind::Generic {
            control,
            arms,
            selected,
        } = kind
        {
            let ranges: Vec<_> = std::iter::once(control.expression)
                .chain(
                    arms.iter()
                        .enumerate()
                        .filter(|(index, _)| index != selected)
                        .map(|(_, arm)| arm.expression),
                )
                .map(|id| self.parsed_spans[self.code.expressions[id.index()].occurrence.index()])
                .collect();
            for (bound, span) in self.code.bounds[start..]
                .iter_mut()
                .zip(&self.bounds_builder.spans[start..])
            {
                if ranges
                    .iter()
                    .any(|range| range.start <= span.start && span.end <= range.end)
                    && matches!(
                        bound.evaluation,
                        BoundEvaluation::Required | BoundEvaluation::MayBeOmitted
                    )
                {
                    bound.evaluation = BoundEvaluation::Unevaluated;
                }
            }
        }
    }
    pub(crate) fn site_type_use(&self, site: super::SiteId) -> TypeUseId {
        self.code.declarations[site.index()].type_use
    }

    pub(crate) fn plain_type_use(&mut self, ty: &Type, offset: usize) -> Result<TypeUseId, Error> {
        let shape = self.intern_type(ty, offset)?;
        self.type_use(shape, Vec::new(), offset)
    }
    fn type_use(
        &mut self,
        shape: TypeId,
        extents: Vec<Extent>,
        offset: usize,
    ) -> Result<TypeUseId, Error> {
        if extents.is_empty()
            && let Some(id) = self.bounds_builder.plain.get(&shape)
        {
            return Ok(*id);
        }
        let edges = 1 + extents
            .iter()
            .map(|extent| 1 + extent.path.len())
            .sum::<usize>();
        self.budget.charge(
            1,
            edges,
            extents.len() * std::mem::size_of::<Extent>()
                + extents
                    .iter()
                    .map(|e| e.path.len() * std::mem::size_of::<TypeStep>())
                    .sum::<usize>(),
            offset,
        )?;
        let id = TypeUseId(self.code.type_uses.len() as u32);
        if extents.is_empty() {
            self.bounds_builder.plain.insert(shape, id);
        }
        self.code.type_uses.push(TypeUse { shape, extents });
        Ok(id)
    }
    pub(crate) fn retype_use(
        &mut self,
        id: TypeUseId,
        ty: &Type,
        offset: usize,
    ) -> Result<TypeUseId, Error> {
        let shape = self.intern_type(ty, offset)?;
        if self.code.type_uses[id.index()].shape == shape {
            return Ok(id);
        }
        let extents = self.code.type_uses[id.index()]
            .extents
            .iter()
            .filter(|extent| extent_type(ty, &extent.path).is_some())
            .cloned()
            .collect();
        self.type_use(shape, extents, offset)
    }
    pub(crate) fn wrap_type_use(
        &mut self,
        id: TypeUseId,
        ty: &Type,
        step: TypeStep,
        bound: Option<BoundId>,
        offset: usize,
    ) -> Result<TypeUseId, Error> {
        let mut extents = self.code.type_uses[id.index()].extents.clone();
        for extent in &mut extents {
            extent.path.insert(0, step.clone());
        }
        if let Some(bound) = bound {
            extents.insert(
                0,
                Extent {
                    path: Vec::new(),
                    bound,
                },
            );
        }
        let shape = self.intern_type(ty, offset)?;
        self.type_use(shape, extents, offset)
    }
    pub(crate) fn project_type_use(
        &mut self,
        id: TypeUseId,
        ty: &Type,
        step: TypeStep,
        offset: usize,
    ) -> Result<TypeUseId, Error> {
        let extents = self.code.type_uses[id.index()]
            .extents
            .iter()
            .filter(|extent| extent.path.first() == Some(&step))
            .map(|extent| Extent {
                path: extent.path[1..].to_vec(),
                bound: extent.bound,
            })
            .collect();
        let shape = self.intern_type(ty, offset)?;
        self.type_use(shape, extents, offset)
    }
    pub(crate) fn function_type_use(
        &mut self,
        result: TypeUseId,
        parameters: &[TypeUseId],
        ty: &Type,
        offset: usize,
    ) -> Result<TypeUseId, Error> {
        let mut extents = Vec::new();
        for (id, step) in std::iter::once((result, TypeStep::Return)).chain(
            parameters
                .iter()
                .enumerate()
                .map(|(i, id)| (*id, TypeStep::Parameter(i))),
        ) {
            for extent in &self.code.type_uses[id.index()].extents {
                let mut path = vec![step.clone()];
                path.extend_from_slice(&extent.path);
                extents.push(Extent {
                    path,
                    bound: extent.bound,
                });
            }
        }
        let shape = self.intern_type(ty, offset)?;
        self.type_use(shape, extents, offset)
    }
    pub(crate) fn array_bound(
        &mut self,
        declaration: &Node<ast::Declarator>,
        expression: Option<&Node<ast::Expression>>,
        span: Span,
        context: BoundContext,
    ) -> Result<BoundId, Error> {
        let BoundContext {
            prototype,
            definition,
            type_name,
            minimum,
            constant,
        } = context;
        let owner = self.find(OccurrenceKind::Declarator, declaration)?;
        let value = match expression {
            Some(expression) => {
                let expression = self.expression_id(expression)?;
                match constant {
                    Some(value) => BoundValue::Constant { expression, value },
                    None => BoundValue::Expression(expression),
                }
            }
            None => BoundValue::PrototypeStar,
        };
        let site = if prototype {
            BoundSite::Prototype
        } else if definition {
            BoundSite::FunctionEntry
        } else if type_name {
            BoundSite::TypeName
        } else {
            BoundSite::Declaration
        };
        self.budget.charge(1, 3, 0, span.start)?;
        let id = BoundId(self.code.bounds.len() as u32);
        self.code.bounds.push(Bound {
            source: unmapped_span(span),
            owner,
            scope: self.current,
            site,
            minimum,
            evaluation: if constant.is_some() {
                BoundEvaluation::Constant
            } else if prototype {
                BoundEvaluation::Prototype
            } else {
                BoundEvaluation::Required
            },
            value,
        });
        self.bounds_builder.spans.push(span);
        Ok(id)
    }
    pub(crate) fn declarator_type_use(
        &mut self,
        node: &Node<ast::Declarator>,
        name: Option<&str>,
        ty: TypeUseId,
    ) -> Result<(), Error> {
        if let Some(name) = name {
            self.budget.charge(0, 1, name.len(), node.span.start)?;
            self.bounds_builder
                .pending
                .insert((self.current, name.to_owned()), ty);
        }
        Ok(())
    }
    pub(crate) fn parameter_type_use(
        &mut self,
        parameter: &Node<ast::ParameterDeclaration>,
        id: TypeUseId,
    ) -> Result<(), Error> {
        if let Some(occurrence) = self.find(OccurrenceKind::Parameter, parameter)? {
            self.budget.charge(0, 1, 0, parameter.span.start)?;
            self.bounds_builder.parameters.insert(occurrence, id);
        }
        Ok(())
    }
    pub(super) fn declaration_type_use(
        &mut self,
        entity: EntityId,
        occurrence: OccurrenceId,
        ty: &Type,
        offset: usize,
    ) -> Result<(TypeUseId, Option<TypeUseId>), Error> {
        let named = self.code.entities[entity.index()]
            .name
            .as_ref()
            .and_then(|name| {
                self.bounds_builder
                    .pending
                    .remove(&(self.current, name.clone()))
            });
        let declared = self.bounds_builder.parameters.remove(&occurrence).or(named);
        let mut result = match declared {
            Some(id) => id,
            None => self.plain_type_use(ty, offset)?,
        };
        let parameter = self.code.entities[entity.index()].kind == EntityKind::Parameter;
        if parameter {
            let original = &self.code.types[self.code.type_uses[result.index()].shape.index()];
            if matches!(
                original.kind,
                TypeKind::Array { .. } | TypeKind::VariableArray { .. }
            ) {
                let mut extents = self.code.type_uses[result.index()].extents.clone();
                extents.retain(|extent| !extent.path.is_empty());
                for extent in &mut extents {
                    if extent.path.first() == Some(&TypeStep::Element) {
                        extent.path[0] = TypeStep::Pointer;
                    }
                }
                let shape = self.intern_type(ty, offset)?;
                result = self.type_use(shape, extents, offset)?;
            } else {
                result = self.retype_use(result, ty, offset)?;
            }
        } else {
            result = self.retype_use(result, ty, offset)?;
        }
        self.budget.charge(0, 1, 0, offset)?;
        self.bounds_builder.entities.insert(entity, result);
        Ok((result, declared))
    }
    pub(crate) fn base_type_use(
        &mut self,
        specs: &[Node<ast::TypeSpecifier>],
        ty: &Type,
        offset: usize,
    ) -> Result<TypeUseId, Error> {
        let inherited = if let [specifier] = specs {
            match &specifier.node {
                ast::TypeSpecifier::TypedefName(name) => self
                    .entity_for_name(&name.node.name)
                    .and_then(|id| self.bounds_builder.entities.get(&id).copied()),
                ast::TypeSpecifier::TypeOf(value) => match &value.node {
                    ast::TypeOf::Type(name) => self.type_name_use(&name.node),
                    ast::TypeOf::Expression(expr) => {
                        let expression = self.expression_id(expr)?;
                        let id = self.code.expressions[expression.index()].type_use;
                        if self.code.type_uses[id.index()].extents.is_empty() {
                            let occurrence = self.code.expressions[expression.index()].occurrence;
                            let start = self.bounds_builder.starts[&occurrence];
                            for bound in &mut self.code.bounds[start..] {
                                if matches!(
                                    bound.evaluation,
                                    BoundEvaluation::Required | BoundEvaluation::MayBeOmitted
                                ) {
                                    bound.evaluation = BoundEvaluation::Unevaluated;
                                }
                            }
                        }
                        Some(id)
                    }
                },
                _ => None,
            }
        } else {
            None
        };
        match inherited {
            Some(id) => self.retype_use(id, ty, offset),
            None => self.plain_type_use(ty, offset),
        }
    }
    pub(crate) fn type_name_use(&self, name: &ast::TypeName) -> Option<TypeUseId> {
        self.bounds_builder
            .type_names
            .get(&type_name_key(name))
            .copied()
    }
    pub(crate) fn save_type_name_use(
        &mut self,
        name: &ast::TypeName,
        id: TypeUseId,
    ) -> Result<(), Error> {
        let key = type_name_key(name);
        self.budget.charge(0, 1, 0, key.0)?;
        self.bounds_builder.type_names.insert(key, id);
        Ok(())
    }
    pub(crate) fn converted_type_use(
        &mut self,
        id: TypeUseId,
        ty: &Type,
        conversions: &[super::expression::ConversionStep],
        offset: usize,
    ) -> Result<TypeUseId, Error> {
        let mut extents = self.code.type_uses[id.index()].extents.clone();
        for conversion in conversions {
            match conversion.kind {
                Conversion::ArrayDecay => {
                    extents.retain(|e| !e.path.is_empty());
                    for extent in &mut extents {
                        if extent.path.first() == Some(&TypeStep::Element) {
                            extent.path[0] = TypeStep::Pointer;
                        }
                    }
                }
                Conversion::FunctionDecay => {
                    for extent in &mut extents {
                        extent.path.insert(0, TypeStep::Pointer);
                    }
                }
                _ => {}
            }
        }
        if !matches!(
            ty.kind,
            TypeKind::Pointer(_)
                | TypeKind::VariableArray { .. }
                | TypeKind::Array { .. }
                | TypeKind::Function(_)
                | TypeKind::Typedef(_)
        ) {
            extents.clear();
        }
        extents.retain(|extent| extent_type(ty, &extent.path).is_some());
        let shape = self.intern_type(ty, offset)?;
        self.type_use(shape, extents, offset)
    }
    pub(super) fn expression_type_use(
        &mut self,
        kind: &ExprKind,
        ty: &Type,
        explicit: Option<TypeUseId>,
        owner: OccurrenceId,
    ) -> Result<TypeUseId, Error> {
        let offset = self.parsed_spans[owner.index()].start;
        if let Some(id) = explicit {
            return self.retype_use(id, ty, offset);
        }
        let plain = |this: &mut Self| this.plain_type_use(ty, offset);
        match kind {
            ExprKind::Name(entity) => match self.bounds_builder.entities.get(entity).copied() {
                Some(id) => self.retype_use(id, ty, offset),
                None => plain(self),
            },
            ExprKind::Unary {
                operator: Unary::Address,
                operand,
                ..
            } => self.wrap_type_use(
                self.code.expressions[operand.expression.index()].type_use,
                ty,
                TypeStep::Pointer,
                None,
                offset,
            ),
            ExprKind::Unary {
                operator: Unary::Indirection,
                operand,
                ..
            } => self.project_type_use(operand.type_use, ty, TypeStep::Pointer, offset),
            ExprKind::Unary {
                operator:
                    Unary::PostIncrement
                    | Unary::PostDecrement
                    | Unary::PreIncrement
                    | Unary::PreDecrement,
                operand,
                ..
            } => self.retype_use(operand.type_use, ty, offset),
            ExprKind::AddressIndirection { pointer, .. } => {
                self.retype_use(pointer.type_use, ty, offset)
            }
            ExprKind::Binary {
                operator: Binary::Index,
                left,
                right,
                ..
            } => {
                let id = if matches!(
                    self.code.types[left.effective_type.index()].kind,
                    TypeKind::Pointer(_)
                ) {
                    left.type_use
                } else {
                    right.type_use
                };
                self.project_type_use(id, ty, TypeStep::Pointer, offset)
            }
            ExprKind::Binary {
                operator: Binary::Assign | Binary::AssignPlus | Binary::AssignMinus,
                left,
                ..
            } => self.retype_use(left.type_use, ty, offset),
            ExprKind::Binary {
                operator: Binary::Plus | Binary::Minus,
                left,
                right,
                ..
            } if matches!(ty.kind, TypeKind::Pointer(_)) => {
                let id = if matches!(
                    self.code.types[left.effective_type.index()].kind,
                    TypeKind::Pointer(_)
                ) {
                    left.type_use
                } else {
                    right.type_use
                };
                self.retype_use(id, ty, offset)
            }
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                let left = self.before_composite_conversion(then_value, offset)?;
                let right = self.before_composite_conversion(else_value, offset)?;
                self.composite_type_use(&[left, right], Some(condition.expression), ty, owner)
            }
            ExprKind::Generic { arms, selected, .. } => self.retype_use(
                self.code.expressions[arms[*selected].expression.index()].type_use,
                ty,
                offset,
            ),
            ExprKind::Comma(values) => match values.last() {
                Some(value) => self.retype_use(value.type_use, ty, offset),
                None => plain(self),
            },
            ExprKind::StatementExpression {
                result: Some(value),
                ..
            } => self.retype_use(value.type_use, ty, offset),
            ExprKind::Call { callee, .. } => {
                let extents = self.code.type_uses[callee.type_use.index()]
                    .extents
                    .iter()
                    .filter_map(|extent| {
                        extent
                            .path
                            .strip_prefix(&[TypeStep::Pointer, TypeStep::Return])
                            .map(|path| Extent {
                                path: path.to_vec(),
                                bound: extent.bound,
                            })
                    })
                    .collect();
                let shape = self.intern_type(ty, offset)?;
                self.type_use(shape, extents, offset)
            }
            _ => plain(self),
        }
    }
    fn before_composite_conversion(
        &mut self,
        value: &super::expression::ExprUse,
        offset: usize,
    ) -> Result<TypeUseId, Error> {
        let source = self.code.expressions[value.expression.index()].type_use;
        if let Some(step) = value.conversions.first()
            && matches!(
                step.kind,
                Conversion::ArrayDecay | Conversion::FunctionDecay
            )
        {
            let ty = self.code.types[step.target_type.index()].clone();
            self.converted_type_use(source, &ty, &value.conversions[..1], offset)
        } else {
            Ok(source)
        }
    }
    fn composite_type_use(
        &mut self,
        uses: &[TypeUseId],
        selection: Option<ExprId>,
        ty: &Type,
        owner: OccurrenceId,
    ) -> Result<TypeUseId, Error> {
        let span = self.parsed_spans[owner.index()];
        let mut paths: BTreeMap<Vec<TypeStep>, Vec<BoundInput>> = BTreeMap::new();
        for id in uses {
            for extent in &self.code.type_uses[id.index()].extents {
                if extent_type(ty, &extent.path).is_some() {
                    paths.entry(extent.path.clone()).or_default();
                }
            }
        }
        for (path, inputs) in &mut paths {
            for id in uses {
                let source = &self.code.type_uses[id.index()];
                if !matches!(
                    self.code.types[source.shape.index()].kind,
                    TypeKind::Pointer(_)
                        | TypeKind::Array { .. }
                        | TypeKind::VariableArray { .. }
                        | TypeKind::Typedef(_)
                ) {
                    continue;
                }
                let input = match source.extents.iter().find(|extent| extent.path == *path) {
                    Some(extent) => BoundInput::Runtime(extent.bound),
                    None => match extent_type(&self.code.types[source.shape.index()], path) {
                        Some(TypeKind::Array {
                            length: Some(value),
                            ..
                        }) => BoundInput::Constant(*value),
                        _ => BoundInput::Unspecified,
                    },
                };
                if !inputs.contains(&input) {
                    inputs.push(input);
                }
            }
        }
        let mut extents = Vec::new();
        for (path, inputs) in paths {
            let bound = if let [BoundInput::Runtime(id)] = inputs.as_slice() {
                *id
            } else {
                self.budget.charge(
                    1,
                    inputs.len() + 2,
                    inputs.len() * std::mem::size_of::<BoundInput>(),
                    span.start,
                )?;
                let id = BoundId(self.code.bounds.len() as u32);
                self.code.bounds.push(Bound {
                    source: unmapped_span(span),
                    owner: Some(owner),
                    scope: self.current,
                    site: BoundSite::Composite,
                    minimum: false,
                    evaluation: BoundEvaluation::Derived,
                    value: BoundValue::Composite { inputs, selection },
                });
                self.bounds_builder.spans.push(span);
                id
            };
            extents.push(Extent { path, bound });
        }
        let shape = self.intern_type(ty, span.start)?;
        self.type_use(shape, extents, span.start)
    }
    pub(super) fn finish_bounds(
        &mut self,
        offsets: &crate::parser_extensions::SourceMap,
    ) -> Result<(), Error> {
        for (bound, span) in self.code.bounds.iter_mut().zip(&self.bounds_builder.spans) {
            bound.source = map_span(offsets, *span, &mut self.budget)?;
        }
        Ok(())
    }
}
fn extent_type<'a>(mut ty: &'a Type, path: &[TypeStep]) -> Option<&'a TypeKind> {
    for step in path {
        ty = match (step, &ty.kind) {
            (_, TypeKind::Typedef(_)) => return Some(&ty.kind),
            (TypeStep::Pointer, TypeKind::Pointer(inner)) => inner,
            (
                TypeStep::Element,
                TypeKind::Array { element, .. } | TypeKind::VariableArray { element },
            ) => element,
            (TypeStep::Return, TypeKind::Function(function)) => &function.return_type,
            (TypeStep::Parameter(index), TypeKind::Function(function)) => {
                &function.parameters.get(*index)?.ty
            }
            _ => return None,
        };
    }
    matches!(
        ty.kind,
        TypeKind::Array { .. } | TypeKind::VariableArray { .. } | TypeKind::Typedef(_)
    )
    .then_some(&ty.kind)
}

pub(crate) fn type_name_key(name: &ast::TypeName) -> (usize, usize) {
    let start = name.specifiers.first().map_or(0, |n| n.span.start);
    let end = name.declarator.as_ref().map_or_else(
        || name.specifiers.last().map_or(start, |n| n.span.end),
        |n| n.span.end,
    );
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::analyze_inner;
    use crate::checked::{CheckedCode, DeclarationSite, Limits};
    use toucan_target::Target;

    fn checked(source: &str) -> CheckedCode {
        let (ordinary, _) = analyze_inner(source, Target::X86_64UnknownLinuxGnu, None).unwrap();
        let (retained, code) = analyze_inner(
            source,
            Target::X86_64UnknownLinuxGnu,
            Some(Limits::default()),
        )
        .unwrap();
        assert_eq!(format!("{ordinary:?}"), format!("{retained:?}"));
        code.unwrap()
    }
    fn site<'a>(code: &'a CheckedCode, name: &str) -> &'a DeclarationSite {
        code.declarations
            .iter()
            .find(|site| code.entities[site.entity.index()].name.as_deref() == Some(name))
            .unwrap()
    }
    fn extents(code: &CheckedCode, id: TypeUseId) -> &[Extent] {
        &code.type_uses[id.index()].extents
    }
    #[test]
    fn local_typedef_dimensions_are_shared_without_reevaluation() {
        let code =
            checked("int f(int n) { typedef int A[n++]; A a; A *p; return sizeof a + sizeof *p; }");
        assert_eq!(code.bounds.len(), 1);
        assert_eq!(code.bounds[0].site, BoundSite::Declaration);
        assert_eq!(code.bounds[0].evaluation, BoundEvaluation::Required);
        assert_eq!(
            extents(&code, site(&code, "A").type_use),
            extents(&code, site(&code, "a").type_use)
        );
        assert_eq!(
            extents(&code, site(&code, "p").type_use)[0].path,
            vec![TypeStep::Pointer]
        );
        for expression in &code.expressions {
            if let ExprKind::SizeOfValue { operand, .. } = &expression.kind {
                assert_eq!(extents(&code, operand.type_use)[0].bound, BoundId(0));
                assert!(extents(&code, operand.type_use)[0].path.is_empty());
            }
        }
    }

    #[test]
    fn typeof_typedefs_reuse_bounds_and_retain_written_alias_references() {
        let source = "int f(int n) { typedef int A[n++]; __typeof__(A) a; __typeof__(A *) p = &a; __typeof__(A[2]) b; __typeof__(A[n++]) c; return sizeof a + sizeof *p + sizeof b + sizeof c; }";
        let code = checked(source);
        let alias = site(&code, "A");
        let original = extents(&code, alias.type_use)[0].bound;
        assert_eq!(
            extents(&code, site(&code, "a").type_use),
            extents(&code, alias.type_use)
        );
        assert_eq!(
            extents(&code, site(&code, "p").type_use),
            &[Extent {
                path: vec![TypeStep::Pointer],
                bound: original
            }]
        );
        for name in ["b", "c"] {
            let dimensions = extents(&code, site(&code, name).type_use);
            assert!(
                dimensions
                    .iter()
                    .any(|extent| extent.bound == original && extent.path == [TypeStep::Element])
            );
            if name == "c" {
                assert!(
                    dimensions
                        .iter()
                        .any(|extent| extent.bound != original && extent.path.is_empty())
                );
            }
        }
        let increments: Vec<_> = code
            .bounds
            .iter()
            .filter(|bound| {
                let BoundValue::Expression(expression) = bound.value else {
                    return false;
                };
                let occurrence = code.expressions[expression.index()].occurrence;
                &source[code.occurrences[occurrence.index()].source.range.clone()] == "n++"
            })
            .collect();
        assert_eq!(
            increments.len(),
            2,
            "the typedef bound must not be duplicated"
        );
        assert!(
            increments
                .iter()
                .all(|bound| bound.evaluation == BoundEvaluation::Required)
        );
        let references: Vec<_> = code
            .references
            .iter()
            .filter(|reference| reference.target == alias.entity)
            .collect();
        assert_eq!(references.len(), 4);
        for reference in references {
            assert_eq!(&source[reference.source.range.clone()], "A");
            assert_eq!(
                reference.kind,
                super::super::references::ReferenceKind::Typedef
            );
        }
    }
    #[test]
    fn prototype_and_definition_bounds_keep_declared_parameter_dimensions() {
        let code = checked(
            "int f(int n, int a[n][n+1]); int f(int n, int a[n][n+1]) { return sizeof *a; }",
        );
        assert_eq!(code.bounds.len(), 4);
        assert_eq!(
            code.bounds
                .iter()
                .filter(|b| b.evaluation == BoundEvaluation::Prototype)
                .count(),
            2
        );
        assert_eq!(
            code.bounds
                .iter()
                .filter(|b| b.site == BoundSite::FunctionEntry
                    && b.evaluation == BoundEvaluation::Required)
                .count(),
            2
        );
        for site in code
            .declarations
            .iter()
            .filter(|s| code.entities[s.entity.index()].name.as_deref() == Some("a"))
        {
            let declared = site.declared_type_use.unwrap();
            assert_eq!(extents(&code, declared).len(), 2);
            assert_eq!(extents(&code, site.type_use).len(), 1);
            assert_eq!(
                extents(&code, site.type_use)[0].path,
                vec![TypeStep::Pointer]
            );
        }
    }
    #[test]
    fn sizeof_alignof_and_generic_preserve_evaluation_rules() {
        let code = checked(
            "int f(int n) { return sizeof(int[n++]) + sizeof(int (*)[n++]) + _Alignof(int[n++]) + _Generic((int (*)[n++])0, default: sizeof(int[n++]), int: sizeof(int[n++])); }",
        );
        let mut ordered: Vec<_> = code.bounds.iter().collect();
        ordered.sort_by_key(|bound| bound.source.range.start);
        let modes: Vec<_> = ordered.iter().map(|b| b.evaluation).collect();
        assert_eq!(
            modes,
            vec![
                BoundEvaluation::Required,
                BoundEvaluation::MayBeOmitted,
                BoundEvaluation::Unevaluated,
                BoundEvaluation::Unevaluated,
                BoundEvaluation::Required,
                BoundEvaluation::Unevaluated
            ]
        );
        assert!(
            code.expressions
                .iter()
                .filter(|e| matches!(e.kind, ExprKind::SizeOfType(_) | ExprKind::AlignOf(_)))
                .all(|e| e.type_name_use.is_some())
        );
    }
    #[test]
    fn pointer_composites_keep_differing_runtime_bounds() {
        let code = checked(
            "int f(int n, int m, int choose) { int (*p)[n]; int (*q)[m]; return sizeof *(choose ? p : q); }",
        );
        assert_eq!(code.bounds.len(), 3);
        let BoundValue::Composite { inputs, selection } = &code.bounds[2].value else {
            panic!("composite extent")
        };
        assert_eq!(
            inputs,
            &vec![
                BoundInput::Runtime(BoundId(0)),
                BoundInput::Runtime(BoundId(1))
            ]
        );
        assert!(selection.is_some());
        let sizeof = code
            .expressions
            .iter()
            .find_map(|e| {
                if let ExprKind::SizeOfValue { operand, .. } = &e.kind {
                    Some(operand)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(
            extents(&code, sizeof.type_use),
            &[Extent {
                path: vec![],
                bound: BoundId(2)
            }]
        );
    }
    #[test]
    fn cast_and_decay_keep_nested_paths_and_occurrences() {
        let code = checked(
            "int sink(int (*)[]); int f(int n) { int a[2][n]; int (*p)[n+1]; sink(a); return sizeof *((int (*)[n+2])p); }",
        );
        assert_eq!(code.bounds.len(), 3);
        let call = code
            .expressions
            .iter()
            .find_map(|e| {
                if let ExprKind::Call { arguments, .. } = &e.kind {
                    Some(arguments)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(
            extents(&code, call[0].type_use)[0].path,
            vec![TypeStep::Pointer]
        );
        assert_eq!(extents(&code, call[0].type_use)[0].bound, BoundId(0));
        let sizeof = code
            .expressions
            .iter()
            .find_map(|e| {
                if let ExprKind::SizeOfValue { operand, .. } = &e.kind {
                    Some(operand)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(extents(&code, sizeof.type_use)[0].bound, BoundId(2));
        assert_eq!(code.bounds[2].evaluation, BoundEvaluation::Required);
    }
    #[test]
    fn constant_composites_null_branches_and_pointer_updates_preserve_facts() {
        let code = checked(
            "int f(int n, int choose) { int (*p)[n]; int (*q)[4]; sizeof *(choose ? p : q); sizeof *(choose ? p : 0); return sizeof *p++; }",
        );
        let BoundValue::Composite { inputs, .. } = &code.bounds[1].value else {
            panic!("runtime/constant composite")
        };
        assert_eq!(
            inputs,
            &vec![BoundInput::Runtime(BoundId(0)), BoundInput::Constant(4)]
        );
        assert_eq!(code.bounds.len(), 2);
        let dimensions: Vec<_> = code
            .expressions
            .iter()
            .filter_map(|e| {
                if let ExprKind::SizeOfValue { operand, .. } = &e.kind {
                    Some(extents(&code, operand.type_use)[0].bound)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(dimensions, vec![BoundId(1), BoundId(0), BoundId(0)]);
    }

    #[test]
    fn star_bounds_typeof_and_compound_literals_keep_their_sites() {
        let code = checked(
            "void proto(int prototype_array[*]); int f(int n) { typedef int A[n]; A a; typeof(a) *p; sizeof *((int (*)[n+1]){0}); return sizeof *p; }",
        );
        assert!(matches!(code.bounds[0].value, BoundValue::PrototypeStar));
        assert_eq!(code.bounds[0].evaluation, BoundEvaluation::Prototype);
        assert_eq!(
            extents(&code, site(&code, "A").type_use),
            extents(&code, site(&code, "a").type_use)
        );
        assert_eq!(
            extents(&code, site(&code, "p").type_use)[0].bound,
            BoundId(1)
        );
        let literal = code
            .expressions
            .iter()
            .find(|expression| matches!(expression.kind, ExprKind::CompoundLiteral { .. }))
            .unwrap();
        assert_eq!(literal.type_name_use, Some(literal.type_use));
        assert_eq!(extents(&code, literal.type_use)[0].bound, BoundId(2));
    }

    #[test]
    #[ignore = "requires native GCC and Clang; run with --include-ignored"]
    fn required_and_optional_bound_effects_match_native_c() {
        let source = r#"
            static int calls;
            static int step(void) { ++calls; return 3; }
            static int parameter(int n, int a[n++]) { return n; }
            int main(void) {
                int n=3; typedef int A[n++]; A a; n=19;
                if (sizeof a != 3*sizeof(int) || n != 19) return 1;
                int ordinary[4]; if (parameter(3, ordinary) != 4) return 2;
                calls=0; if (sizeof(int[step()]) != 3*sizeof(int) || calls != 1) return 3;
                calls=0; (void)sizeof(int (*)[step()]); if (calls != 0 && calls != 1) return 4;
                calls=0; (void)_Alignof(int[step()]); if (calls != 0) return 5;
                calls=0; (void)_Generic((int (*)[step()])0, default: 1); if (calls != 0) return 6;
                return 0;
            }
        "#;
        let _ = checked(source);
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("bounds.c");
        std::fs::write(&input, source).unwrap();
        for compiler in [
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            "clang".into(),
        ] {
            let executable = directory.path().join("bounds");
            let output = std::process::Command::new(&compiler)
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
                std::process::Command::new(&executable)
                    .status()
                    .unwrap()
                    .success(),
                "{compiler}: bound effects differ"
            );
        }
    }
    #[test]
    fn required_bound_expression_dependencies_and_unevaluated_typeof_are_distinct() {
        let code = checked(
            "int f(int n) { sizeof(int[sizeof(int[n++])]); typeof(sizeof(int[n++])) x; return 0; }",
        );
        assert_eq!(
            code.bounds.iter().map(|b| b.evaluation).collect::<Vec<_>>(),
            vec![
                BoundEvaluation::Required,
                BoundEvaluation::Required,
                BoundEvaluation::Unevaluated
            ]
        );
    }
    #[test]
    fn unnamed_parameters_retain_adjusted_away_bounds() {
        let code = checked("void f(int n, int [n]);");
        let site = code
            .declarations
            .iter()
            .find(|site| {
                code.entities[site.entity.index()].kind == EntityKind::Parameter
                    && code.entities[site.entity.index()].name.is_none()
            })
            .unwrap();
        assert_eq!(
            extents(&code, site.declared_type_use.unwrap())[0].bound,
            BoundId(0)
        );
        assert!(extents(&code, site.type_use).is_empty());
    }
    #[test]
    fn static_parameter_minimums_and_pointer_qualifiers_remain_written_facts() {
        let code =
            checked("void f(int n, int a[static const restrict volatile 4], int b[static n]);");
        assert_eq!(code.bounds.len(), 2);
        assert!(code.bounds.iter().all(|bound| bound.minimum));
        assert!(matches!(
            code.bounds[0].value,
            BoundValue::Constant { value: 4, .. }
        ));
        assert_eq!(code.bounds[0].evaluation, BoundEvaluation::Constant);
        assert_eq!(code.bounds[1].evaluation, BoundEvaluation::Prototype);
        let a = site(&code, "a");
        let shape = &code.types[code.type_uses[a.type_use.index()].shape.index()];
        assert!(
            shape.qualifiers.is_const
                && shape.qualifiers.is_restrict
                && shape.qualifiers.is_volatile
        );
        assert_eq!(
            extents(&code, a.declared_type_use.unwrap())[0].bound,
            BoundId(0)
        );
        assert!(extents(&code, a.type_use).is_empty());
    }
    #[test]
    fn retention_preserves_target_results_and_invalid_bound_diagnostics() {
        let source =
            "int f(int n, int a[static n]) { typedef int A[n]; A x; return sizeof x + sizeof a; }";
        for target in Target::ALL {
            let (plain, _) = analyze_inner(source, target, None).unwrap();
            let (retained, code) = analyze_inner(source, target, Some(Limits::default())).unwrap();
            assert_eq!(format!("{plain:?}"), format!("{retained:?}"));
            assert_eq!(code.unwrap().bounds.len(), 2);
            for invalid in [
                "void f(int a[*]) {}",
                "int f(int n) { int a[n] = {1}; return 0; }",
                "int f(void) { int a[missing]; return 0; }",
            ] {
                let plain = analyze_inner(invalid, target, None).unwrap_err();
                let retained = analyze_inner(invalid, target, Some(Limits::default())).unwrap_err();
                assert_eq!(
                    (plain.offset, plain.message),
                    (retained.offset, retained.message)
                );
            }
        }
    }
}

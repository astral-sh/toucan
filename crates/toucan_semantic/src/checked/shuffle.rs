//! A vector shuffle keeps evaluation separate from its lane-selection plan.

use lang_c::{ast, span::Node};
use serde::Serialize;

use super::{ExprKind, ExprUse, OccurrenceKind, UseContext};
use crate::{Error, analyze::Analyzer};

/// Selection from the concatenated vector operands of a constant shuffle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum ShuffleLane {
    Index(u64),
    /// Signed -1 permits an arbitrary value; no particular lane or bit pattern is promised.
    Undefined,
}

#[derive(Debug, Serialize)]
pub struct ShuffleIndex {
    pub(crate) operand: ExprUse,
    pub(crate) lane: ShuffleLane,
}
impl ShuffleIndex {
    /// Written constant expression; its runtime evaluation is suppressed.
    pub fn operand(&self) -> &ExprUse {
        &self.operand
    }
    pub fn lane(&self) -> ShuffleLane {
        self.lane
    }
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum ShuffleMask {
    /// Ordered output lanes selected from both inputs.
    Constant(Vec<ShuffleIndex>),
    /// Clang's second vector is a runtime mask over the first vector. Each index
    /// is reduced modulo the input lane count. Negative values, including -1,
    /// select a lane; they do not denote undefined output values in this form.
    Dynamic,
}

impl Analyzer {
    pub(super) fn retain_shuffle_vector(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<ExprKind, Error> {
        let offset = call.span.start;
        let payload = self
            .shuffle_vector_signature(call)?
            .indices
            .as_ref()
            .map_or(0, Vec::len);
        self.code_builder().budget.charge(
            0,
            3 + payload,
            payload * std::mem::size_of::<ShuffleIndex>(),
            offset,
        )?;
        let indices = self.shuffle_vector_signature(call)?.indices.clone();
        let first = self.retained_value(&call.node.arguments[0])?;
        let second = self.retained_value(&call.node.arguments[1])?;
        let callee_occurrence = self
            .code_builder()
            .find(OccurrenceKind::Expression, &call.node.callee)?
            .ok_or_else(|| Error::new(offset, "shuffle callee has no retained occurrence"))?;
        let mask = if let Some(indices) = indices {
            let mut lanes = Vec::with_capacity(indices.len());
            for (argument, lane) in call.node.arguments[2..].iter().zip(indices) {
                lanes.push(ShuffleIndex {
                    operand: self.retained_use(argument, UseContext::UnevaluatedValue, None)?,
                    lane,
                });
            }
            ShuffleMask::Constant(lanes)
        } else {
            ShuffleMask::Dynamic
        };
        Ok(ExprKind::ShuffleVector {
            callee_occurrence,
            operands: [first, second],
            mask,
        })
    }
}

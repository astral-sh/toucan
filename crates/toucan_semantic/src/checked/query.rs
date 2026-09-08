//! Compiler-query evaluation boundaries, independent of constant-query results.

use serde::Serialize;

use super::{Binary, Builder, Builtin, Conversion, ExprKind, ExprUse, Unary};

/// Whether Clang's ordinary-side-effect test permits scalar code generation.
/// This test intentionally does not inspect `sizeof` or `_Alignof` operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum QuerySideEffects {
    /// No ordinary side effects were found. Runtime type operands can still act.
    Absent,
    /// An assignment, increment, volatile/atomic read, or known effectful builtin exists.
    Present,
    /// The graph does not resolve this test for ordinary calls (whose `pure` or
    /// `const` annotations are not retained), statement expressions, or compound
    /// literals. This is an unresolved gate, not a claim that execution occurs.
    Unresolved,
}

impl QuerySideEffects {
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Present, _) | (_, Self::Present) => Self::Present,
            (Self::Unresolved, _) | (_, Self::Unresolved) => Self::Unresolved,
            _ => Self::Absent,
        }
    }
}

/// Why a compiler query cannot execute its value operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum QuerySuppression {
    GnuProfile,
    NonNumericConstantQuery,
    MinimumSubobjectSize,
    OrdinarySideEffects,
    /// Clang or GCC completed this object-size query before scalar code generation.
    ObjectSizeFrontendFold,
}

/// Evaluation policy for argument zero of a constant or object-size query.
///
/// This is a conditional plan, not an optimizer prediction or an inferred query
/// value. In a required constant-expression context no runtime fallback executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum QueryEvaluation {
    Unevaluated(QuerySuppression),
    /// Clang first attempts constant evaluation of the entire intrinsic. Only
    /// if that does not produce its result, and the side-effect gate is Absent,
    /// does it evaluate argument zero as a scalar expression. Unresolved gates
    /// describe both possibilities. A conservative Toucan query value of zero
    /// does not establish that Clang reaches this fallback.
    ///
    /// The argument's existing expression tree is the plan: conditionals and
    /// short-circuit operators select their operands; comma operands are ordered;
    /// other C operand orders remain unspecified. VLA sizeof evaluates its value
    /// operand, or the fresh bounds/typeof operands of its written type. Non-VLA
    /// sizeof and alignof do not evaluate their operands. Casts evaluate fresh
    /// variably modified type operands. Previously declared typedef bounds are
    /// reused, not evaluated again. Nested queries apply their own policies.
    ClangFallback {
        side_effects: QuerySideEffects,
    },
}

impl QueryEvaluation {
    pub(crate) fn may_evaluate(self) -> bool {
        matches!(self, Self::ClangFallback { .. })
    }
}

/// A small construction-only cache: each child is summarized once, without
/// recursively walking the whole subtree for each enclosing query.
#[derive(Clone, Copy)]
pub(super) struct QuerySummary {
    pub(super) effects: QuerySideEffects,
    pub(super) volatile_lvalue: bool,
}

impl Builder {
    pub(crate) fn retain_object_size_proof(
        &mut self,
        proof: super::ObjectSizeProof,
        offset: usize,
    ) -> Result<Option<Box<super::ObjectSizeProof>>, crate::Error> {
        self.budget
            .charge(1, 1, std::mem::size_of::<super::ObjectSizeProof>(), offset)?;
        Ok(Some(Box::new(proof)))
    }

    pub(super) fn query_use_effects(&self, value: &ExprUse) -> QuerySideEffects {
        let summary = self.expression_builder.query_summaries[value.expression.index()];
        if value
            .conversions
            .iter()
            .any(|c| c.kind == Conversion::AtomicLoad)
            || summary.volatile_lvalue
                && value
                    .conversions
                    .iter()
                    .any(|c| c.kind == Conversion::Lvalue)
        {
            QuerySideEffects::Present
        } else {
            summary.effects
        }
    }

    pub(super) fn query_effects(&self, kind: &ExprKind) -> QuerySideEffects {
        use QuerySideEffects::{Absent, Present, Unresolved};
        let operands = |values: &[ExprUse]| {
            values.iter().fold(Absent, |effects, value| {
                effects.combine(self.query_use_effects(value))
            })
        };
        match kind {
            ExprKind::TypesCompatible { .. }
            | ExprKind::Integer(_)
            | ExprKind::Float { .. }
            | ExprKind::String(_)
            | ExprKind::Name(_)
            | ExprKind::SizeOfType(_)
            | ExprKind::SizeOfValue { .. }
            | ExprKind::AlignOf(_)
            | ExprKind::OffsetOf { .. } => Absent,
            ExprKind::Unary {
                operator, operand, ..
            } => {
                if matches!(
                    operator,
                    Unary::PostIncrement
                        | Unary::PostDecrement
                        | Unary::PreIncrement
                        | Unary::PreDecrement
                ) {
                    Present
                } else {
                    self.query_use_effects(operand)
                }
            }
            ExprKind::AddressIndirection { pointer, .. } => self.query_use_effects(pointer),
            ExprKind::Binary {
                operator,
                left,
                right,
                ..
            } => {
                if matches!(
                    operator,
                    Binary::Assign
                        | Binary::AssignMultiply
                        | Binary::AssignDivide
                        | Binary::AssignModulo
                        | Binary::AssignPlus
                        | Binary::AssignMinus
                        | Binary::AssignShiftLeft
                        | Binary::AssignShiftRight
                        | Binary::AssignBitwiseAnd
                        | Binary::AssignBitwiseXor
                        | Binary::AssignBitwiseOr
                ) {
                    Present
                } else {
                    // Unlike execution, this gate inspects even a dead RHS.
                    self.query_use_effects(left)
                        .combine(self.query_use_effects(right))
                }
            }
            ExprKind::Cast { value, .. } => self.query_use_effects(value),
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
            } => self
                .query_use_effects(condition)
                .combine(self.query_use_effects(then_value))
                .combine(self.query_use_effects(else_value)),
            ExprKind::Member { base, .. } => self.query_use_effects(base),
            ExprKind::Call {
                callee, arguments, ..
            } => Unresolved
                .combine(self.query_use_effects(callee))
                .combine(operands(arguments)),
            ExprKind::BuiltinCall {
                builtin, arguments, ..
            } => match builtin {
                Builtin::X86(intrinsic) => {
                    if intrinsic.has_side_effects() {
                        Present
                    } else {
                        operands(arguments)
                    }
                }
                Builtin::Infinity
                | Builtin::InfinityFloat
                | Builtin::InfinityLongDouble
                | Builtin::HugeValue
                | Builtin::HugeValueFloat
                | Builtin::HugeValueLongDouble => Absent,
                Builtin::ConstantQuery
                | Builtin::Expect
                | Builtin::Nan
                | Builtin::NanFloat
                | Builtin::NanLongDouble
                | Builtin::SignalingNan
                | Builtin::SignalingNanFloat
                | Builtin::SignalingNanLongDouble
                | Builtin::ByteSwap16
                | Builtin::ByteSwap32
                | Builtin::ByteSwap64
                | Builtin::CountLeadingZeros
                | Builtin::CountLeadingZerosLong
                | Builtin::CountLeadingZerosLongLong
                | Builtin::CountTrailingZeros
                | Builtin::CountTrailingZerosLong
                | Builtin::CountTrailingZerosLongLong => operands(arguments),
                Builtin::Overflow(intrinsic) if intrinsic.is_predicate() => operands(arguments),
                Builtin::Overflow(_) => Present,
                Builtin::Atomic(crate::atomic::AtomicOperation::AlwaysLockFree) => Absent,
                Builtin::Atomic(crate::atomic::AtomicOperation::IsLockFree) => operands(arguments),
                // Clang's object-size builtins do not carry the const attribute.
                Builtin::Atomic(_)
                | Builtin::Sync(_)
                | Builtin::ObjectSize
                | Builtin::DynamicObjectSize
                | Builtin::VaStart
                | Builtin::VaEnd
                | Builtin::VaCopy
                | Builtin::Unreachable
                | Builtin::Trap
                | Builtin::Memset
                | Builtin::Memcpy
                | Builtin::Memmove
                | Builtin::MemcpyChecked
                | Builtin::MemmoveChecked
                | Builtin::MempcpyChecked
                | Builtin::MemsetChecked
                | Builtin::StrcpyChecked
                | Builtin::StpcpyChecked
                | Builtin::StrcatChecked
                | Builtin::StrncpyChecked
                | Builtin::StpncpyChecked
                | Builtin::StrncatChecked
                | Builtin::SprintfChecked
                | Builtin::SnprintfChecked
                | Builtin::VsprintfChecked
                | Builtin::VsnprintfChecked
                | Builtin::PrintfChecked
                | Builtin::VprintfChecked
                | Builtin::FprintfChecked
                | Builtin::VfprintfChecked => Present,
                Builtin::Memcmp => Unresolved.combine(operands(arguments)),
                // These intrinsics are rejected on Clang profiles.
                Builtin::VaArgPack | Builtin::VaArgPackLength | Builtin::VectorShuffle => {
                    Unresolved
                }
            },
            ExprKind::VaArg { .. } => Present,
            ExprKind::Choose {
                then_expression,
                else_expression,
                then_selected,
                ..
            } => {
                self.expression_builder.query_summaries[if *then_selected {
                    then_expression.index()
                } else {
                    else_expression.index()
                }]
                .effects
            }
            ExprKind::Generic { arms, selected, .. } => {
                self.expression_builder.query_summaries[arms[*selected].expression.index()].effects
            }
            ExprKind::Comma(values) => operands(values),
            ExprKind::CompoundLiteral { .. } | ExprKind::StatementExpression { .. } => Unresolved,
        }
    }
}

impl crate::analyze::Analyzer {
    pub(super) fn retained_query_evaluation(
        &mut self,
        builtin: Builtin,
        call: &lang_c::span::Node<lang_c::ast::CallExpression>,
        arguments: &[ExprUse],
    ) -> Result<Option<QueryEvaluation>, crate::Error> {
        use QuerySuppression::{
            GnuProfile, MinimumSubobjectSize, NonNumericConstantQuery, OrdinarySideEffects,
        };
        if !matches!(
            builtin,
            Builtin::ConstantQuery | Builtin::ObjectSize | Builtin::DynamicObjectSize
        ) {
            return Ok(None);
        }
        if matches!(
            self.unit.target,
            toucan_target::Target::X86_64UnknownLinuxGnu
                | toucan_target::Target::Aarch64UnknownLinuxGnu
        ) {
            return Ok(Some(QueryEvaluation::Unevaluated(GnuProfile)));
        }
        if builtin == Builtin::ConstantQuery {
            let ty = self.code_builder().code.types[arguments[0].effective_type.index()].clone();
            if !matches!(
                self.unit.resolve(&ty)?.kind,
                crate::TypeKind::Integer(_)
                    | crate::TypeKind::Enum(_)
                    | crate::TypeKind::Float(_)
                    | crate::TypeKind::Bool
            ) {
                return Ok(Some(QueryEvaluation::Unevaluated(NonNumericConstantQuery)));
            }
        } else {
            // The mode has already passed Clang's ICE and int-conversion checks.
            let mode = self.eval(&call.node.arguments[1])?;
            if mode.value as u32 == 3 {
                return Ok(Some(QueryEvaluation::Unevaluated(MinimumSubobjectSize)));
            }
        }
        let side_effects = self.code_builder().query_use_effects(&arguments[0]);
        Ok(Some(if side_effects == QuerySideEffects::Present {
            QueryEvaluation::Unevaluated(OrdinarySideEffects)
        } else {
            QueryEvaluation::ClangFallback { side_effects }
        }))
    }
}

//! Owned expression facts and operand conversions, linked to retained initializers.

use std::collections::HashMap;

use lang_c::{ast, span::Node};
use serde::Serialize;

use super::{
    AssignmentId, Builder, EntityId, EntityKind, InitializerId, OccurrenceId, OccurrenceKind,
    ScopeId, TypeId,
};
use crate::analyze::Analyzer;
use crate::expression::ExpressionInfo;
use crate::integer::integer_to_type;
use crate::{DecodedString, Error, FloatKind, IntegerKind, IntegerValue, Type, TypeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct ExprId(pub(crate) u32);
impl ExprId {
    /// Returns the owner-local arena index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum ValueCategory {
    Value,
    ObjectLvalue,
    FunctionDesignator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum Conversion {
    Lvalue,
    /// Sequentially-consistent C atomic lvalue conversion. In a ReadModifyWrite
    /// use, this belongs to the enclosing single atomic update, not a separate load.
    AtomicLoad,
    ArrayDecay,
    FunctionDecay,
    IntegerPromotion,
    Arithmetic,
    Pointer,
    Assignment,
    /// A compiler intrinsic's argument conversion, including GCC pointer/integer
    /// bridges and qualifier erasure that ordinary assignment does not permit.
    IntrinsicArgument,
    /// Constructs a transparent-union argument through the selected field.
    TransparentUnion {
        field: super::EntityId,
    },
    DefaultArgument,
    ExplicitCast,
    /// Convert a scalar to the lane type and repeat it in every vector lane.
    VectorSplat,
    Conditional,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum UseContext {
    Value,
    /// Expand the enclosing inline function's anonymous arguments here. The
    /// expression's int type is only the GNU builtin's type-checking placeholder;
    /// no ordinary argument conversion or single integer argument is implied.
    VariadicPack,
    Place,
    ReadModifyWrite,
    Unevaluated,
    UnevaluatedValue,
    /// GNU overflow predicates discard this operand's value, while volatile
    /// reads and other effects still follow its expression plan. No integer
    /// promotion is applied; the original expression retains bitfield precision.
    DiscardedValue,
    /// Conditional scalar evaluation governed by the enclosing builtin's
    /// [`super::QueryEvaluation`]. Ordinary value conversions are preserved.
    CompilerQuery,
}

#[derive(Debug, Serialize)]
pub struct ConversionStep {
    pub(crate) kind: Conversion,
    pub(crate) target_type: TypeId,
}

#[derive(Debug, Serialize)]
pub struct ExprUse {
    pub(crate) type_use: super::bounds::TypeUseId,
    pub(crate) expression: ExprId,
    pub(crate) effective_type: TypeId,
    pub(crate) context: UseContext,
    pub(crate) conversions: Vec<ConversionStep>,
}

#[derive(Debug, Serialize)]
pub struct Expression {
    /// Store/update performed by this operator; atomic loads are use conversions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) atomic_access: Option<crate::atomic_type::AtomicAccess>,
    /// Written type-name owner, independent of its reusable canonical/type-use shape.
    pub(crate) type_name: Option<OccurrenceId>,
    pub(crate) type_name_use: Option<super::bounds::TypeUseId>,
    pub(crate) type_use: super::bounds::TypeUseId,
    pub(crate) occurrence: OccurrenceId,
    pub(crate) scope: ScopeId,
    pub(crate) ty: TypeId,
    pub(crate) category: ValueCategory,
    pub(crate) bitfield: Option<u64>,
    pub(crate) register: bool,
    pub(crate) vector_element: bool,
    pub(crate) kind: ExprKind,
}

macro_rules! operators {
    ($name:ident, $ast:ident, $($variant:ident),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
        #[non_exhaustive]
        pub enum $name { $($variant),* }
        impl $name {
            fn from_ast(operator: &ast::$ast) -> Self {
                match operator { $(ast::$ast::$variant => Self::$variant),* }
            }
        }
    };
}
operators!(
    Unary,
    UnaryOperator,
    PostIncrement,
    PostDecrement,
    PreIncrement,
    PreDecrement,
    Address,
    Indirection,
    Plus,
    Minus,
    Complement,
    Negate
);
operators!(
    Binary,
    BinaryOperator,
    Index,
    Multiply,
    Divide,
    Modulo,
    Plus,
    Minus,
    ShiftLeft,
    ShiftRight,
    Less,
    Greater,
    LessOrEqual,
    GreaterOrEqual,
    Equals,
    NotEquals,
    BitwiseAnd,
    BitwiseXor,
    BitwiseOr,
    LogicalAnd,
    LogicalOr,
    Assign,
    AssignMultiply,
    AssignDivide,
    AssignModulo,
    AssignPlus,
    AssignMinus,
    AssignShiftLeft,
    AssignShiftRight,
    AssignBitwiseAnd,
    AssignBitwiseXor,
    AssignBitwiseOr
);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum Builtin {
    /// GNU one- or two-vector shuffle. The last argument is an integer mask;
    /// each mask lane selects modulo the concatenated input lane count. All
    /// operands are evaluated once in ordinary unspecified argument order.
    VectorShuffle,
    X86(crate::x86::X86Intrinsic),
    Sync(crate::sync::SyncOperation),
    Atomic(crate::atomic::AtomicOperation),
    C11Atomic(crate::c11_atomic::C11AtomicOperation),
    Overflow(crate::overflow::OverflowIntrinsic),
    VaStart,
    VaEnd,
    VaCopy,
    VaArgPack,
    VaArgPackLength,
    Expect,
    Unreachable,
    Trap,
    Memset,
    Memcpy,
    Memmove,
    Memcmp,
    ByteSwap16,
    ByteSwap32,
    ByteSwap64,
    ConstantQuery,
    Infinity,
    InfinityFloat,
    InfinityLongDouble,
    HugeValue,
    HugeValueFloat,
    HugeValueLongDouble,
    Nan,
    NanFloat,
    NanLongDouble,
    SignalingNan,
    SignalingNanFloat,
    SignalingNanLongDouble,
    ObjectSize,
    DynamicObjectSize,
    MemcpyChecked,
    MemmoveChecked,
    MempcpyChecked,
    MemsetChecked,
    StrcpyChecked,
    StpcpyChecked,
    StrcatChecked,
    StrncpyChecked,
    StpncpyChecked,
    StrncatChecked,
    SprintfChecked,
    SnprintfChecked,
    VsprintfChecked,
    VsnprintfChecked,
    PrintfChecked,
    VprintfChecked,
    FprintfChecked,
    VfprintfChecked,
    CountLeadingZeros,
    CountLeadingZerosLong,
    CountLeadingZerosLongLong,
    CountTrailingZeros,
    CountTrailingZerosLong,
    CountTrailingZerosLongLong,
}
impl Builtin {
    /// Whether evaluated uses must be expanded with a concrete inlined caller.
    /// These operations have no standalone scalar runtime implementation.
    pub fn requires_inline_expansion(self) -> bool {
        matches!(self, Self::VaArgPack | Self::VaArgPackLength)
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "__builtin_shuffle" => Self::VectorShuffle,
            "__builtin_va_start" => Self::VaStart,
            "__builtin_va_end" => Self::VaEnd,
            "__builtin_va_copy" => Self::VaCopy,
            "__builtin_va_arg_pack" => Self::VaArgPack,
            "__builtin_va_arg_pack_len" => Self::VaArgPackLength,
            "__builtin_expect" => Self::Expect,
            "__builtin_unreachable" => Self::Unreachable,
            "__builtin_trap" => Self::Trap,
            "__builtin_memset" => Self::Memset,
            "__builtin_memcpy" => Self::Memcpy,
            "__builtin_memmove" => Self::Memmove,
            "__builtin_memcmp" => Self::Memcmp,
            "__builtin_bswap16" => Self::ByteSwap16,
            "__builtin_bswap32" => Self::ByteSwap32,
            "__builtin_bswap64" => Self::ByteSwap64,
            "__builtin_constant_p" => Self::ConstantQuery,
            "__builtin_inf" => Self::Infinity,
            "__builtin_inff" => Self::InfinityFloat,
            "__builtin_infl" => Self::InfinityLongDouble,
            "__builtin_huge_val" => Self::HugeValue,
            "__builtin_huge_valf" => Self::HugeValueFloat,
            "__builtin_huge_vall" => Self::HugeValueLongDouble,
            "__builtin_nan" => Self::Nan,
            "__builtin_nanf" => Self::NanFloat,
            "__builtin_nanl" => Self::NanLongDouble,
            "__builtin_nans" => Self::SignalingNan,
            "__builtin_nansf" => Self::SignalingNanFloat,
            "__builtin_nansl" => Self::SignalingNanLongDouble,
            "__builtin_object_size" => Self::ObjectSize,
            "__builtin_dynamic_object_size" => Self::DynamicObjectSize,
            "__builtin___memcpy_chk" => Self::MemcpyChecked,
            "__builtin___memmove_chk" => Self::MemmoveChecked,
            "__builtin___mempcpy_chk" => Self::MempcpyChecked,
            "__builtin___memset_chk" => Self::MemsetChecked,
            "__builtin___strcpy_chk" => Self::StrcpyChecked,
            "__builtin___stpcpy_chk" => Self::StpcpyChecked,
            "__builtin___strcat_chk" => Self::StrcatChecked,
            "__builtin___strncpy_chk" => Self::StrncpyChecked,
            "__builtin___stpncpy_chk" => Self::StpncpyChecked,
            "__builtin___strncat_chk" => Self::StrncatChecked,
            "__builtin___sprintf_chk" => Self::SprintfChecked,
            "__builtin___snprintf_chk" => Self::SnprintfChecked,
            "__builtin___vsprintf_chk" => Self::VsprintfChecked,
            "__builtin___vsnprintf_chk" => Self::VsnprintfChecked,
            "__builtin___printf_chk" => Self::PrintfChecked,
            "__builtin___vprintf_chk" => Self::VprintfChecked,
            "__builtin___fprintf_chk" => Self::FprintfChecked,
            "__builtin___vfprintf_chk" => Self::VfprintfChecked,

            "__builtin_clz" => Self::CountLeadingZeros,
            "__builtin_clzl" => Self::CountLeadingZerosLong,
            "__builtin_clzll" => Self::CountLeadingZerosLongLong,
            "__builtin_ctz" => Self::CountTrailingZeros,
            "__builtin_ctzl" => Self::CountTrailingZerosLong,
            "__builtin_ctzll" => Self::CountTrailingZerosLongLong,
            name => {
                if let Some(intrinsic) = crate::x86::X86Intrinsic::from_name(name) {
                    Self::X86(intrinsic)
                } else if let Some(intrinsic) = crate::overflow::OverflowIntrinsic::from_name(name)
                {
                    Self::Overflow(intrinsic)
                } else if let Some(operation) =
                    crate::c11_atomic::C11AtomicOperation::from_name(name)
                {
                    Self::C11Atomic(operation)
                } else if let Some(operation) = crate::atomic::AtomicOperation::from_name(name) {
                    Self::Atomic(operation)
                } else {
                    Self::Sync(crate::sync::SyncOperation::from_name(name)?)
                }
            }
        })
    }
}

/// A checked written type, including its source occurrence and runtime bounds.
#[derive(Debug, Serialize)]
pub struct TypeNameOperand {
    /// The written type-name source site.
    pub occurrence: OccurrenceId,
    /// The checked shape and identities of any runtime bounds.
    pub type_use: super::TypeUseId,
}

#[derive(Debug, Serialize)]
pub struct GenericArm {
    pub(crate) ty: Option<TypeId>,
    pub(crate) expression: ExprId,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum ExprKind {
    Integer(IntegerValue),
    Float {
        digits: String,
        hexadecimal: bool,
    },
    String(DecodedString),
    Name(EntityId),
    Unary {
        operator: Unary,
        operand: ExprUse,
        computation_type: Option<TypeId>,
        write_back: Option<TypeId>,
    },
    /// C's &* rule cancels the written indirection, even for a void pointer.
    AddressIndirection {
        indirection: OccurrenceId,
        pointer: ExprUse,
    },
    Binary {
        operator: Binary,
        left: ExprUse,
        right: ExprUse,
        computation_type: Option<TypeId>,
        write_back: Option<TypeId>,
    },
    Cast {
        destination: TypeId,
        value: ExprUse,
    },
    Conditional {
        condition: ExprUse,
        then_value: ExprUse,
        else_value: ExprUse,
    },
    Member {
        base: ExprUse,
        indirect: bool,
        fields: Vec<usize>,
    },
    Call {
        callee: ExprUse,
        direct_callee: Option<EntityId>,
        arguments: Vec<ExprUse>,
    },
    BuiltinCall {
        builtin: Builtin,
        /// Present only for constant and object-size query intrinsics.
        query_evaluation: Option<super::QueryEvaluation>,
        /// Structural extent and compiler-result facts for object-size queries.
        object_size: Option<Box<super::ObjectSizeProof>>,
        callee_occurrence: OccurrenceId,
        arguments: Vec<ExprUse>,
    },
    VaArg {
        list: ExprUse,
        requested_type: TypeId,
    },
    SizeOfType(TypeId),
    SizeOfValue {
        operand: ExprUse,
        variable: bool,
    },
    AlignOf(TypeId),
    OffsetOf {
        record: TypeId,
        members: Vec<OffsetMember>,
    },
    /// Both written types are checked, but their bounds and typeof operands are
    /// unevaluated. The int result is an integer constant expression.
    TypesCompatible {
        left: TypeNameOperand,
        right: TypeNameOperand,
        compatible: bool,
    },
    /// The condition is an unevaluated integer constant expression. Only the
    /// selected arm executes; its original type and value category are preserved.
    Choose {
        condition: ExprUse,
        then_expression: ExprId,
        else_expression: ExprId,
        then_selected: bool,
    },
    Generic {
        control: ExprUse,
        arms: Vec<GenericArm>,
        selected: usize,
    },
    Comma(Vec<ExprUse>),
    CompoundLiteral {
        initializer: InitializerId,
    },
    StatementExpression {
        body: super::statement::StatementId,
        result: Option<ExprUse>,
    },
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum OffsetMember {
    Field(Vec<usize>),
    Index(ExprId),
}

/// Every parsed expression is accounted for, including metadata grammar and
/// operators represented by their parent's dedicated semantic form.
#[derive(Debug, Serialize)]
pub struct ExpressionCoverage {
    pub(crate) occurrence: OccurrenceId,
    pub(crate) status: Coverage,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub enum Coverage {
    Typed(ExprId),
    BuiltinCallee(ExprId),
    CanceledIndirection(ExprId),
    AttributeArgument,
    ParserInserted,
    Missing,
}

pub(crate) enum BeginExpression {
    Cached(ExpressionInfo),
    Check(Option<OccurrenceId>),
}

#[derive(Clone, Copy)]
enum State {
    Checking,
    Complete(ExprId),
}

struct ExpressionProperties {
    function: bool,
    volatile_lvalue: bool,
    atomic_access: Option<crate::atomic_type::AtomicAccess>,
}

#[derive(Default)]
pub(super) struct ExpressionBuilder {
    pub(super) query_summaries: Vec<super::query::QuerySummary>,
    states: HashMap<OccurrenceId, State>,
    assignments: HashMap<(ExprId, TypeId), AssignmentId>,
    statement_results: HashMap<OccurrenceId, Option<ExprId>>,
}

impl Builder {
    pub(super) fn finish_expression_coverage(&mut self) -> Result<(), Error> {
        let mut represented = HashMap::new();
        for (index, expression) in self.code.expressions.iter().enumerate() {
            let id = ExprId(index as u32);
            let extra = match expression.kind {
                ExprKind::BuiltinCall {
                    callee_occurrence, ..
                } => Some((callee_occurrence, Coverage::BuiltinCallee(id))),
                ExprKind::AddressIndirection { indirection, .. } => {
                    Some((indirection, Coverage::CanceledIndirection(id)))
                }
                _ => None,
            };
            if let Some((occurrence, status)) = extra {
                self.budget
                    .charge(0, 1, 0, self.parsed_spans[occurrence.index()].start)?;
                represented.insert(occurrence, status);
            }
        }
        for (index, occurrence) in self.code.occurrences.iter().enumerate() {
            if occurrence.kind != OccurrenceKind::Expression {
                continue;
            }
            let id = OccurrenceId(index as u32);
            let status = if let Some(State::Complete(expression)) =
                self.expression_builder.states.get(&id)
            {
                Coverage::Typed(*expression)
            } else if let Some(status) = represented.remove(&id) {
                status
            } else if occurrence.attribute_argument {
                Coverage::AttributeArgument
            } else if occurrence.source.synthetic {
                Coverage::ParserInserted
            } else {
                Coverage::Missing
            };
            self.budget
                .charge(1, 1, 0, self.parsed_spans[index].start)?;
            self.code.expression_coverage.push(ExpressionCoverage {
                occurrence: id,
                status,
            });
        }
        Ok(())
    }

    pub(crate) fn begin_expression(
        &mut self,
        node: &Node<ast::Expression>,
    ) -> Result<BeginExpression, Error> {
        let Some(occurrence) = self.find(OccurrenceKind::Expression, node)? else {
            return Ok(BeginExpression::Check(None));
        };
        match self.expression_builder.states.get(&occurrence) {
            Some(State::Complete(id)) => Ok(BeginExpression::Cached(self.expression_info(*id))),
            Some(State::Checking) => Err(Error::new(
                node.span.start,
                "recursive retained expression type query",
            )),
            None => {
                self.begin_bound_context(occurrence);
                self.budget.charge(0, 2, 0, node.span.start)?;
                self.expression_builder
                    .states
                    .insert(occurrence, State::Checking);
                Ok(BeginExpression::Check(Some(occurrence)))
            }
        }
    }

    /// Retains facts created while evaluating a constant in the enclosing
    /// expression's original context, without checking its syntax twice.
    pub(crate) fn prepare_evaluated_expression(
        &mut self,
        node: &Node<ast::Expression>,
        checkpoint: super::EvaluationCheckpoint,
    ) -> Result<bool, Error> {
        let Some(occurrence) = self.find(OccurrenceKind::Expression, node)? else {
            return Ok(false);
        };
        if self.expression_builder.states.contains_key(&occurrence) {
            return Ok(false);
        }
        self.restore_bound_context(occurrence, checkpoint);
        Ok(true)
    }

    pub(crate) fn expression_needs_check(
        &mut self,
        node: &Node<ast::Expression>,
    ) -> Result<bool, Error> {
        Ok(self
            .find(OccurrenceKind::Expression, node)?
            .is_some_and(|id| !self.expression_builder.states.contains_key(&id)))
    }

    fn expression_info(&self, id: ExprId) -> ExpressionInfo {
        let expression = &self.code.expressions[id.index()];
        ExpressionInfo {
            ty: self.code.types[expression.ty.index()].clone(),
            lvalue: expression.category == ValueCategory::ObjectLvalue,
            bitfield: expression.bitfield,
            register: expression.register,
            vector_element: expression.vector_element,
        }
    }

    pub(super) fn expression_id(&mut self, node: &Node<ast::Expression>) -> Result<ExprId, Error> {
        let occurrence = self
            .find(OccurrenceKind::Expression, node)?
            .ok_or_else(|| {
                Error::new(
                    node.span.start,
                    "expression has no unique retained occurrence",
                )
            })?;
        match self.expression_builder.states.get(&occurrence) {
            Some(State::Complete(id)) => Ok(*id),
            _ => Err(Error::new(
                node.span.start,
                "expression was not checked before retention",
            )),
        }
    }

    /// Saves the final expression while its lexical bindings are still visible.
    /// The containing expression is lowered after the block scope is restored.
    pub(crate) fn statement_expression_result(
        &mut self,
        statement: &Node<ast::Statement>,
        expression: Option<&Node<ast::Expression>>,
    ) -> Result<(), Error> {
        let occurrence = self
            .find(OccurrenceKind::Statement, statement)?
            .ok_or_else(|| {
                Error::new(
                    statement.span.start,
                    "statement expression has no retained block occurrence",
                )
            })?;
        let expression = expression
            .map(|expression| self.expression_id(expression))
            .transpose()?;
        self.budget.charge(0, 2, 0, statement.span.start)?;
        self.expression_builder
            .statement_results
            .insert(occurrence, expression);
        Ok(())
    }

    pub(super) fn unevaluated_selection_ranges(&self, kind: &ExprKind) -> Vec<lang_c::span::Span> {
        let range =
            |id: ExprId| self.parsed_spans[self.code.expressions[id.index()].occurrence.index()];
        match kind {
            ExprKind::Generic {
                control,
                arms,
                selected,
            } => std::iter::once(control.expression)
                .chain(
                    arms.iter()
                        .enumerate()
                        .filter(|(index, _)| index != selected)
                        .map(|(_, arm)| arm.expression),
                )
                .map(range)
                .collect(),
            ExprKind::Choose {
                condition,
                then_expression,
                else_expression,
                then_selected,
            } => {
                vec![
                    range(condition.expression),
                    range(if *then_selected {
                        *else_expression
                    } else {
                        *then_expression
                    }),
                ]
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn entity_for_name(&self, name: &str) -> Option<EntityId> {
        let mut scope = Some(self.current);
        while let Some(id) = scope {
            if let Some(entity) = self.names.get(&id).and_then(|names| names.get(name)) {
                return Some(*entity);
            }
            scope = self.code.scopes[id.index()].parent;
        }
        None
    }

    fn finish_expression(
        &mut self,
        occurrence: OccurrenceId,
        info: &ExpressionInfo,
        properties: ExpressionProperties,
        kind: ExprKind,
        explicit_type_use: Option<super::bounds::TypeUseId>,
        written_type: (Option<super::bounds::TypeUseId>, Option<OccurrenceId>),
    ) -> Result<(), Error> {
        let (type_name_use, type_name) = written_type;
        let offset = self.parsed_spans[occurrence.index()].start;
        self.finish_bound_context(occurrence, &kind, type_name_use);
        let type_use = self.expression_type_use(&kind, &info.ty, explicit_type_use, occurrence)?;
        let ty = self.intern_type(&info.ty, offset)?;
        self.budget
            .charge(1, 4 + usize::from(type_name.is_some()), 0, offset)?;
        let id = ExprId(self.code.expressions.len() as u32);
        self.budget.charge(
            0,
            0,
            std::mem::size_of::<super::query::QuerySummary>(),
            offset,
        )?;
        self.expression_builder
            .query_summaries
            .push(super::query::QuerySummary {
                effects: self.query_effects(&kind),
                volatile_lvalue: properties.volatile_lvalue,
            });
        self.code.expressions.push(Expression {
            atomic_access: properties.atomic_access,
            type_use,
            type_name_use,
            type_name,
            occurrence,
            scope: self.current,
            ty,
            category: if properties.function {
                ValueCategory::FunctionDesignator
            } else if info.lvalue {
                ValueCategory::ObjectLvalue
            } else {
                ValueCategory::Value
            },
            bitfield: info.bitfield,
            register: info.register,
            vector_element: info.vector_element,
            kind,
        });
        self.expression_builder
            .states
            .insert(occurrence, State::Complete(id));
        Ok(())
    }
}

impl Analyzer {
    pub(crate) fn code_builder(&mut self) -> &mut Builder {
        self.checked
            .as_deref_mut()
            .expect("expression retention is enabled")
    }

    pub(crate) fn retained_type(&mut self, ty: &Type, offset: usize) -> Result<TypeId, Error> {
        self.code_builder().intern_type(ty, offset)
    }

    pub(crate) fn retained_expression_id(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ExprId, Error> {
        if self.code_builder().expression_needs_check(expression)? {
            self.expression_info(expression)?;
        }
        self.code_builder().expression_id(expression)
    }

    pub(crate) fn retained_use(
        &mut self,
        expression: &Node<ast::Expression>,
        context: UseContext,
        destination: Option<(Type, Conversion)>,
    ) -> Result<ExprUse, Error> {
        let expression_id = self.retained_expression_id(expression)?;
        self.retained_use_by_id(expression_id, context, destination, expression.span.start)
    }

    fn retained_use_by_id(
        &mut self,
        expression_id: ExprId,
        context: UseContext,
        destination: Option<(Type, Conversion)>,
        offset: usize,
    ) -> Result<ExprUse, Error> {
        let info = self.code_builder().expression_info(expression_id);
        let mut conversions = Vec::new();
        let mut ty = info.ty.clone();
        if matches!(
            context,
            UseContext::Value
                | UseContext::UnevaluatedValue
                | UseContext::DiscardedValue
                | UseContext::ReadModifyWrite
                | UseContext::CompilerQuery
        ) {
            let conversion = match self.unit.resolve(&info.ty)?.kind {
                TypeKind::Array { .. } | TypeKind::VariableArray { .. } => {
                    Some(Conversion::ArrayDecay)
                }
                TypeKind::Function(_) => Some(Conversion::FunctionDecay),
                TypeKind::Atomic(_) if info.lvalue => Some(Conversion::AtomicLoad),
                _ if info.lvalue => Some(Conversion::Lvalue),
                _ => None,
            };
            ty = self.converted_type(&info, offset)?;
            if let Some(kind) = conversion {
                conversions.push(ConversionStep {
                    kind,
                    target_type: self.retained_type(&ty, offset)?,
                });
            }
        }
        if let Some((destination, kind)) = destination {
            // C11 6.3.1.8 selects a floating common type before considering
            // integer promotions. Mixed operands convert directly to that type.
            if matches!(kind, Conversion::Arithmetic | Conversion::Conditional)
                && matches!(
                    self.unit.resolve(&destination)?.kind,
                    TypeKind::Integer(_) | TypeKind::Bool | TypeKind::Enum(_)
                )
                && matches!(
                    ty.kind,
                    TypeKind::Integer(_) | TypeKind::Bool | TypeKind::Enum(_)
                )
            {
                let promoted = integer_to_type(self.promoted_integer(&info, offset)?);
                if ty != promoted {
                    conversions.push(ConversionStep {
                        kind: Conversion::IntegerPromotion,
                        target_type: self.retained_type(&promoted, offset)?,
                    });
                    ty = promoted;
                }
            }
            if kind == Conversion::VectorSplat {
                let TypeKind::Vector { element, .. } = &destination.kind else {
                    unreachable!()
                };
                if ty != **element {
                    conversions.push(ConversionStep {
                        kind: Conversion::Arithmetic,
                        target_type: self.retained_type(element, offset)?,
                    });
                }
            }
            if ty != destination {
                conversions.push(ConversionStep {
                    kind,
                    target_type: self.retained_type(&destination, offset)?,
                });
            }
            ty = destination;
        }
        let source_use = self.code_builder().code.expressions[expression_id.index()].type_use;
        let type_use =
            self.code_builder()
                .converted_type_use(source_use, &ty, &conversions, offset)?;
        let effective_type = self.retained_type(&ty, offset)?;
        self.code_builder().budget.charge(
            0,
            2 + conversions.len(),
            std::mem::size_of::<ExprUse>()
                + conversions.len() * std::mem::size_of::<ConversionStep>(),
            offset,
        )?;
        Ok(ExprUse {
            type_use,
            expression: expression_id,
            effective_type,
            context,
            conversions,
        })
    }

    pub(crate) fn retained_value(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ExprUse, Error> {
        self.retained_use(expression, UseContext::Value, None)
    }

    fn retained_type_name_operand(
        &mut self,
        name: &Node<ast::TypeName>,
    ) -> Result<TypeNameOperand, Error> {
        let builder = self.code_builder();
        builder.budget.charge(0, 2, 0, name.span.start)?;
        let occurrence = builder.type_name_occurrence(&name.node).ok_or_else(|| {
            Error::new(name.span.start, "type operand has no retained occurrence")
        })?;
        let type_use = builder
            .type_name_use(&name.node)
            .ok_or_else(|| Error::new(name.span.start, "type operand has no retained type use"))?;
        Ok(TypeNameOperand {
            occurrence,
            type_use,
        })
    }

    fn retained_field_path(
        &self,
        ty: &Type,
        name: &str,
        offset: usize,
    ) -> Result<Vec<usize>, Error> {
        self.member_designator(ty, name, 0)?
            .map(|path| path.into_iter().map(|index| index as usize).collect())
            .ok_or_else(|| Error::new(offset, "checked member has no retained field path"))
    }

    fn retained_direct_callee(&mut self, expression: ExprId) -> Option<EntityId> {
        let code = &self.code_builder().code;
        let mut id = expression;
        for _ in 0..128 {
            match &code.expressions[id.index()].kind {
                ExprKind::Name(entity)
                    if code.entities[entity.index()].kind == EntityKind::Function =>
                {
                    return Some(*entity);
                }
                ExprKind::Unary {
                    operator: Unary::Address | Unary::Indirection,
                    operand,
                    ..
                } => id = operand.expression,
                ExprKind::Cast { value, .. } => id = value.expression,
                ExprKind::Choose {
                    then_expression,
                    else_expression,
                    then_selected,
                    ..
                } => {
                    id = if *then_selected {
                        *then_expression
                    } else {
                        *else_expression
                    };
                }
                ExprKind::Generic { arms, selected, .. } => id = arms[*selected].expression,
                _ => return None,
            }
        }
        None
    }

    pub(crate) fn retain_assignment(
        &mut self,
        expression: &Node<ast::Expression>,
        destination: &Type,
    ) -> Result<AssignmentId, Error> {
        let destination = self.unqualified(destination)?;
        let id = self.retained_expression_id(expression)?;
        let ty = self.retained_type(&destination, expression.span.start)?;
        if let Some(assignment) = self
            .code_builder()
            .expression_builder
            .assignments
            .get(&(id, ty))
        {
            return Ok(*assignment);
        }
        let operand = self.retained_use(
            expression,
            UseContext::Value,
            Some((destination, Conversion::Assignment)),
        )?;
        let builder = self.code_builder();
        builder.budget.charge(1, 1, 0, expression.span.start)?;
        let assignment = AssignmentId(builder.code.assignment_conversions.len() as u32);
        builder
            .expression_builder
            .assignments
            .insert((id, ty), assignment);
        builder.code.assignment_conversions.push(operand);
        Ok(assignment)
    }

    pub(crate) fn retain_expression(
        &mut self,
        expression: &Node<ast::Expression>,
        occurrence: OccurrenceId,
        info: &ExpressionInfo,
    ) -> Result<(), Error> {
        let offset = expression.span.start;
        let mut explicit_type_use = None;
        let mut type_name_use = None;
        let mut type_name = None;
        let kind = match &expression.node {
            ast::Expression::Constant(constant) => match &constant.node {
                ast::Constant::Integer(integer) => {
                    ExprKind::Integer(self.literal(integer, offset)?)
                }
                ast::Constant::Character(character) => {
                    ExprKind::Integer(crate::decode_character_literal(
                        self.character_literals
                            .get(&constant.span.start)
                            .map_or(character.as_str(), String::as_str),
                        self.unit.target,
                        offset,
                    )?)
                }
                ast::Constant::Float(float) => {
                    self.code_builder()
                        .budget
                        .charge(0, 0, float.number.len(), offset)?;
                    ExprKind::Float {
                        digits: float.number.to_string(),
                        hexadecimal: float.base == ast::FloatBase::Hexadecimal,
                    }
                }
            },
            ast::Expression::StringLiteral(strings) => {
                let decoded = self.decode_string_literal(strings, offset)?;
                self.code_builder().budget.charge(
                    0,
                    0,
                    decoded.code_units.len() * std::mem::size_of::<u32>(),
                    offset,
                )?;
                ExprKind::String(decoded)
            }
            ast::Expression::Identifier(identifier) => {
                let entity = self
                    .code_builder()
                    .entity_for_name(&identifier.node.name)
                    .ok_or_else(|| {
                        Error::new(
                            offset,
                            format!(
                                "checked identifier `{}` has no retained entity",
                                identifier.node.name
                            ),
                        )
                    })?;
                ExprKind::Name(entity)
            }
            ast::Expression::UnaryOperator(unary) => {
                if unary.node.operator.node == ast::UnaryOperator::Address
                    && let ast::Expression::UnaryOperator(indirection) = &unary.node.operand.node
                    && indirection.node.operator.node == ast::UnaryOperator::Indirection
                {
                    let source = self
                        .code_builder()
                        .find(OccurrenceKind::Expression, &unary.node.operand)?
                        .ok_or_else(|| {
                            Error::new(offset, "canceled indirection has no retained occurrence")
                        })?;
                    ExprKind::AddressIndirection {
                        indirection: source,
                        pointer: self.retained_value(&indirection.node.operand)?,
                    }
                } else {
                    let operator = Unary::from_ast(&unary.node.operator.node);
                    let operand_info = self.expression_info(&unary.node.operand)?;
                    let update = matches!(
                        operator,
                        Unary::PreIncrement
                            | Unary::PreDecrement
                            | Unary::PostIncrement
                            | Unary::PostDecrement
                    );
                    let integer = matches!(
                        self.unit
                            .resolve(
                                self.unit
                                    .atomic_value(&operand_info.ty)?
                                    .unwrap_or(&operand_info.ty)
                            )?
                            .kind,
                        TypeKind::Integer(_) | TypeKind::Bool | TypeKind::Enum(_)
                    );
                    let (context, destination) = if update {
                        let destination = if integer {
                            Some((
                                integer_to_type(self.promoted_integer(&operand_info, offset)?),
                                Conversion::IntegerPromotion,
                            ))
                        } else {
                            None
                        };
                        (UseContext::ReadModifyWrite, destination)
                    } else if operator == Unary::Address {
                        (UseContext::Place, None)
                    } else if matches!(operator, Unary::Plus | Unary::Minus | Unary::Complement)
                        && integer
                    {
                        (
                            UseContext::Value,
                            Some((
                                integer_to_type(self.promoted_integer(&operand_info, offset)?),
                                Conversion::IntegerPromotion,
                            )),
                        )
                    } else {
                        (UseContext::Value, None)
                    };
                    let operand = self.retained_use(&unary.node.operand, context, destination)?;
                    let computation_type = if update {
                        Some(operand.effective_type)
                    } else {
                        None
                    };
                    let write_back = if update {
                        Some(self.retained_type(
                            if self.unit.atomic_value(&operand_info.ty)?.is_some() {
                                &operand_info.ty
                            } else {
                                &info.ty
                            },
                            offset,
                        )?)
                    } else {
                        None
                    };
                    ExprKind::Unary {
                        operator,
                        operand,
                        computation_type,
                        write_back,
                    }
                }
            }
            ast::Expression::BinaryOperator(binary) => self.retain_binary(binary, info)?,
            ast::Expression::Cast(cast) => {
                let destination = self.type_name(&cast.node.type_name.node)?;
                explicit_type_use = self.code_builder().type_name_use(&cast.node.type_name.node);
                type_name_use = explicit_type_use;
                type_name = self
                    .code_builder()
                    .type_name_occurrence(&cast.node.type_name.node);
                let destination =
                    if self.gnu_sync_profile() && self.unit.atomic_value(&destination)?.is_some() {
                        if let Some(id) = explicit_type_use {
                            explicit_type_use = Some(self.code_builder().project_type_use(
                                id,
                                &info.ty,
                                super::bounds::TypeStep::AtomicValue,
                                offset,
                            )?);
                        }
                        self.atomic_value_type(&destination)?
                    } else {
                        self.unqualified(&destination)?
                    };
                ExprKind::Cast {
                    destination: self.retained_type(&destination, offset)?,
                    value: self.retained_use(
                        &cast.node.expression,
                        UseContext::Value,
                        Some((destination, Conversion::ExplicitCast)),
                    )?,
                }
            }
            ast::Expression::Conditional(conditional) => ExprKind::Conditional {
                condition: self.retained_value(&conditional.node.condition)?,
                then_value: self.retained_use(
                    &conditional.node.then_expression,
                    UseContext::Value,
                    Some((info.ty.clone(), Conversion::Conditional)),
                )?,
                else_value: self.retained_use(
                    &conditional.node.else_expression,
                    UseContext::Value,
                    Some((info.ty.clone(), Conversion::Conditional)),
                )?,
            },
            ast::Expression::Member(member) => {
                let base_info = self.expression_info(&member.node.expression)?;
                let indirect = member.node.operator.node == ast::MemberOperator::Indirect;
                let ty = if indirect {
                    let TypeKind::Pointer(ty) = self.converted_type(&base_info, offset)?.kind
                    else {
                        return Err(Error::new(
                            offset,
                            "retained indirect member requires pointer",
                        ));
                    };
                    *ty
                } else {
                    base_info.ty
                };
                let fields =
                    self.retained_field_path(&ty, &member.node.identifier.node.name, offset)?;
                explicit_type_use =
                    self.retain_member_reference(&ty, &fields, &member.node.identifier)?;
                self.code_builder().budget.charge(
                    0,
                    fields.len(),
                    fields.len() * std::mem::size_of::<usize>(),
                    offset,
                )?;
                ExprKind::Member {
                    base: self.retained_use(
                        &member.node.expression,
                        if indirect {
                            UseContext::Value
                        } else {
                            UseContext::Place
                        },
                        None,
                    )?,
                    indirect,
                    fields,
                }
            }
            ast::Expression::Call(call) => self.retain_call(call)?,
            ast::Expression::VaArg(argument) => {
                explicit_type_use = self
                    .code_builder()
                    .type_name_use(&argument.node.type_name.node);
                type_name_use = explicit_type_use;
                type_name = self
                    .code_builder()
                    .type_name_occurrence(&argument.node.type_name.node);
                ExprKind::VaArg {
                    list: self.retained_use(&argument.node.va_list, UseContext::Place, None)?,
                    requested_type: self.retained_type(&info.ty, offset)?,
                }
            }
            ast::Expression::SizeOfTy(size) => {
                let ty = self.type_name(&size.node.0.node)?;
                type_name_use = self.code_builder().type_name_use(&size.node.0.node);
                type_name = self.code_builder().type_name_occurrence(&size.node.0.node);
                ExprKind::SizeOfType(self.retained_type(&ty, offset)?)
            }
            ast::Expression::SizeOfVal(size) => {
                let operand = self.expression_info(&size.node.0)?;
                let variable = self.unit.is_variable_length_array(&operand.ty)?;
                ExprKind::SizeOfValue {
                    operand: self.retained_use(
                        &size.node.0,
                        if variable {
                            UseContext::Place
                        } else {
                            UseContext::Unevaluated
                        },
                        None,
                    )?,
                    variable,
                }
            }
            ast::Expression::AlignOf(alignment) => {
                let ty = self.type_name(&alignment.node.0.node)?;
                type_name_use = self.code_builder().type_name_use(&alignment.node.0.node);
                type_name = self
                    .code_builder()
                    .type_name_occurrence(&alignment.node.0.node);
                ExprKind::AlignOf(self.retained_type(&ty, offset)?)
            }
            ast::Expression::OffsetOf(offset_of) => self.retain_offset_of(offset_of)?,
            ast::Expression::TypesCompatible(query) => {
                let compatible = self.eval_types_compatible(query)?.truth();
                ExprKind::TypesCompatible {
                    left: self.retained_type_name_operand(&query.node.left)?,
                    right: self.retained_type_name_operand(&query.node.right)?,
                    compatible,
                }
            }
            ast::Expression::Choose(selection) => {
                let then_selected = *self
                    .choose_selections
                    .get(&(selection.span.start, selection.span.end))
                    .ok_or_else(|| Error::new(offset, "compile-time selection was not checked"))?;
                self.code_builder().budget.charge(0, 3, 0, offset)?;
                ExprKind::Choose {
                    condition: self.retained_use(
                        &selection.node.condition,
                        UseContext::UnevaluatedValue,
                        None,
                    )?,
                    then_expression: self
                        .retained_expression_id(&selection.node.then_expression)?,
                    else_expression: self
                        .retained_expression_id(&selection.node.else_expression)?,
                    then_selected,
                }
            }
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)? as *const Node<ast::Expression>;
                let mut arms = Vec::new();
                let mut selected_index = None;
                for (index, association) in selection.node.associations.iter().enumerate() {
                    let (ty, expression) = match &association.node {
                        ast::GenericAssociation::Type(association) => {
                            let ty = self.type_name(&association.node.type_name.node)?;
                            (
                                Some(self.retained_type(&ty, offset)?),
                                association.node.expression.as_ref(),
                            )
                        }
                        ast::GenericAssociation::Default(expression) => (None, expression.as_ref()),
                    };
                    if std::ptr::eq(selected, expression) {
                        selected_index = Some(index);
                    }
                    self.code_builder().budget.charge(
                        0,
                        2,
                        std::mem::size_of::<GenericArm>(),
                        offset,
                    )?;
                    arms.push(GenericArm {
                        ty,
                        expression: self.retained_expression_id(expression)?,
                    });
                }
                ExprKind::Generic {
                    control: self.retained_use(
                        &selection.node.expression,
                        UseContext::UnevaluatedValue,
                        None,
                    )?,
                    arms,
                    selected: selected_index.ok_or_else(|| {
                        Error::new(offset, "generic selection has no retained selected arm")
                    })?,
                }
            }
            ast::Expression::Comma(expressions) => {
                let mut operands = Vec::new();
                for operand in expressions.iter() {
                    operands.push(self.retained_value(operand)?);
                }
                ExprKind::Comma(operands)
            }
            ast::Expression::CompoundLiteral(literal) => {
                explicit_type_use = self
                    .code_builder()
                    .type_name_use(&literal.node.type_name.node);
                type_name_use = explicit_type_use;
                type_name = self
                    .code_builder()
                    .type_name_occurrence(&literal.node.type_name.node);
                ExprKind::CompoundLiteral {
                    initializer: self.code_builder().initializer_id(occurrence, offset)?,
                }
            }
            ast::Expression::Statement(statement) => {
                let body = self
                    .code_builder()
                    .find(OccurrenceKind::Statement, statement)?
                    .ok_or_else(|| {
                        Error::new(
                            offset,
                            "statement expression has no retained block occurrence",
                        )
                    })?;
                let result = self
                    .code_builder()
                    .expression_builder
                    .statement_results
                    .get(&body)
                    .copied()
                    .ok_or_else(|| {
                        Error::new(offset, "statement expression result was not retained")
                    })?;
                let result = result
                    .map(|expression| {
                        self.retained_use_by_id(
                            expression,
                            if info.lvalue {
                                UseContext::Place
                            } else {
                                UseContext::Value
                            },
                            None,
                            offset,
                        )
                    })
                    .transpose()?;
                let body = self.code_builder().statement_id(statement)?;
                ExprKind::StatementExpression { body, result }
            }
        };
        let function = matches!(self.unit.resolve(&info.ty)?.kind, TypeKind::Function(_));
        let volatile_lvalue = info.lvalue && self.unit.qualifiers(&info.ty)?.is_volatile;
        let atomic_access = match &kind {
            ExprKind::Unary {
                operand,
                write_back: Some(_),
                ..
            } => {
                let place = self.code_builder().expression_info(operand.expression);
                self.unit
                    .atomic_value(&place.ty)?
                    .is_some()
                    .then_some(crate::atomic_type::AtomicAccess::ReadModifyWrite)
            }
            ExprKind::Binary {
                operator,
                left,
                write_back: Some(_),
                ..
            } => {
                let place = self.code_builder().expression_info(left.expression);
                self.unit.atomic_value(&place.ty)?.is_some().then_some(
                    if *operator == Binary::Assign {
                        crate::atomic_type::AtomicAccess::Store
                    } else {
                        crate::atomic_type::AtomicAccess::ReadModifyWrite
                    },
                )
            }
            _ => None,
        };
        self.code_builder().finish_expression(
            occurrence,
            info,
            ExpressionProperties {
                function,
                volatile_lvalue,
                atomic_access,
            },
            kind,
            explicit_type_use,
            (type_name_use, type_name),
        )
    }

    fn retain_call(&mut self, call: &Node<ast::CallExpression>) -> Result<ExprKind, Error> {
        let offset = call.span.start;
        if let Some(name) = self.builtin_name(call)
            && let Some(builtin) = Builtin::from_name(name)
        {
            let x86 = if let Builtin::X86(intrinsic) = builtin {
                intrinsic.signature(self.unit.target)
            } else {
                None
            };
            let overflow = if let Builtin::Overflow(intrinsic) = builtin {
                Some((intrinsic, self.overflow_signature(intrinsic, call)?))
            } else {
                None
            };
            let atomic = match builtin {
                Builtin::Atomic(operation) => Some(self.atomic_signature(operation, call)?),
                Builtin::C11Atomic(operation) => Some(self.c11_atomic_signature(operation, call)?),
                _ => None,
            };
            let sync = if let Builtin::Sync(operation) = builtin {
                Some((operation, self.sync_signature(operation, call)?))
            } else {
                None
            };
            let memory = self.memory_builtin_signature(name);
            let nan = self.nan_builtin(name);
            let object_size = self.object_size_signature(name);
            let fortified = self.fortified_signature(name, offset)?;
            let unary_parameter = self
                .byte_swap_type(name)
                .or_else(|| self.bit_count_type(name));
            let callee_occurrence = self
                .code_builder()
                .find(OccurrenceKind::Expression, &call.node.callee)?
                .ok_or_else(|| Error::new(offset, "builtin callee has no retained occurrence"))?;
            let mut arguments = Vec::new();
            let array_list = matches!(
                self.unit
                    .resolve(&self.unit.typedefs["__builtin_va_list"])?
                    .kind,
                TypeKind::Array { .. }
            );
            for (index, argument) in call.node.arguments.iter().enumerate() {
                if fortified
                    .as_ref()
                    .is_some_and(|signature| signature.variadic)
                    && self.argument_pack(argument)?
                {
                    arguments.push(self.retained_use(argument, UseContext::VariadicPack, None)?);
                    continue;
                }
                let (context, destination) = if let Some(signature) = &x86 {
                    let destination = signature.parameters()[index].clone();
                    let conversion = if !self.gnu_vector_profile()
                        && matches!(destination.kind, TypeKind::Vector { .. })
                    {
                        Conversion::IntrinsicArgument
                    } else {
                        Conversion::Assignment
                    };
                    (UseContext::Value, Some((destination, conversion)))
                } else if let Some((intrinsic, signature)) = &overflow {
                    (
                        if intrinsic.is_predicate() && index == 2 {
                            UseContext::DiscardedValue
                        } else {
                            UseContext::Value
                        },
                        signature.parameters[index]
                            .as_ref()
                            .map(|ty| (ty.clone(), signature.conversions[index])),
                    )
                } else if let Some(signature) = &atomic {
                    (
                        signature.context,
                        Some((
                            signature.parameters[index]
                                .as_ref()
                                .expect("atomic argument")
                                .clone(),
                            signature.conversions[index],
                        )),
                    )
                } else if let Some((operation, signature)) = &sync {
                    if index >= operation.required() {
                        (
                            if self.gnu_sync_profile() {
                                UseContext::UnevaluatedValue
                            } else {
                                UseContext::Unevaluated
                            },
                            None,
                        )
                    } else {
                        let destination = if index == 0 {
                            signature.address.as_ref()
                        } else {
                            signature.value.as_ref()
                        }
                        .expect("required sync argument")
                        .clone();
                        (
                            UseContext::Value,
                            Some((
                                destination,
                                if self.gnu_sync_profile() {
                                    Conversion::IntrinsicArgument
                                } else {
                                    Conversion::Assignment
                                },
                            )),
                        )
                    }
                } else if let Some(signature) = &fortified {
                    let destination = if let Some(parameter) = signature.parameters.get(index) {
                        (parameter.clone(), Conversion::Assignment)
                    } else {
                        (
                            self.default_argument_type(argument)?,
                            Conversion::DefaultArgument,
                        )
                    };
                    (UseContext::Value, Some(destination))
                } else if let Some(signature) = &memory {
                    (
                        UseContext::Value,
                        Some((signature.parameters[index].clone(), Conversion::Assignment)),
                    )
                } else if let Some(signature) = &object_size {
                    (
                        UseContext::UnevaluatedValue,
                        Some((signature.parameters[index].clone(), Conversion::Assignment)),
                    )
                } else if let Some(ty) = &unary_parameter {
                    (
                        UseContext::Value,
                        Some((ty.clone(), Conversion::Assignment)),
                    )
                } else if nan.is_some() {
                    (
                        UseContext::Value,
                        Some((self.nan_parameter_type(), Conversion::Assignment)),
                    )
                } else if builtin == Builtin::Expect {
                    (
                        UseContext::Value,
                        Some((
                            Type::new(TypeKind::Integer(IntegerKind::Long)),
                            Conversion::Assignment,
                        )),
                    )
                } else if builtin == Builtin::VectorShuffle {
                    (UseContext::Value, None)
                } else if builtin == Builtin::ConstantQuery {
                    (UseContext::UnevaluatedValue, None)
                } else if builtin == Builtin::VaStart && index == 1 {
                    (UseContext::Unevaluated, None)
                } else if array_list {
                    (UseContext::Value, None)
                } else {
                    (UseContext::Place, None)
                };
                arguments.push(self.retained_use(argument, context, destination)?);
            }
            let object_size = if object_size.is_some() {
                let proof = self.infer_object_size(call)?;
                self.code_builder()
                    .retain_object_size_proof(proof, offset)?
            } else {
                None
            };
            let mut query_evaluation = self.retained_query_evaluation(builtin, call, &arguments)?;
            if object_size
                .as_ref()
                .is_some_and(|proof| proof.frontend_fold())
            {
                query_evaluation = Some(super::QueryEvaluation::Unevaluated(
                    super::QuerySuppression::ObjectSizeFrontendFold,
                ));
            }
            if query_evaluation.is_some_and(super::QueryEvaluation::may_evaluate) {
                arguments[0].context = UseContext::CompilerQuery;
            }
            return Ok(ExprKind::BuiltinCall {
                builtin,
                query_evaluation,
                object_size,
                callee_occurrence,
                arguments,
            });
        }
        let callee = self.retained_value(&call.node.callee)?;
        let direct_callee = self.retained_direct_callee(callee.expression);
        let ty = self.code_builder().code.types[callee.effective_type.index()].clone();
        let TypeKind::Pointer(pointee) = ty.kind else {
            return Err(Error::new(offset, "retained callee has no pointer type"));
        };
        let TypeKind::Function(function) = self.unit.resolve(&pointee)?.kind.clone() else {
            return Err(Error::new(offset, "retained callee has no function type"));
        };
        let mut arguments = Vec::new();
        for (index, argument) in call.node.arguments.iter().enumerate() {
            if self.argument_pack(argument)? {
                arguments.push(self.retained_use(argument, UseContext::VariadicPack, None)?);
                continue;
            }
            let (destination, conversion) = if function.prototype
                && let Some(parameter) = function.parameters.get(index)
            {
                if let Some(selected) = self.transparent_argument(&parameter.ty, argument)? {
                    let member = self.unit.records[selected.record].fields.as_ref().unwrap()
                        [selected.field]
                        .ty
                        .clone();
                    let mut operand = self.retained_use(
                        argument,
                        UseContext::Value,
                        Some((member, Conversion::Assignment)),
                    )?;
                    let union = self.unqualified(&parameter.ty)?;
                    let union_type = self.retained_type(&union, offset)?;
                    let origin = self.unit.record_origin(selected.record)?;
                    let field = self
                        .code_builder()
                        .entities
                        .get(&super::EntityKey::Field(origin, selected.field))
                        .copied()
                        .ok_or_else(|| {
                            Error::new(
                                offset,
                                "transparent_union field has no retained declaration",
                            )
                        })?;
                    self.code_builder().budget.charge(
                        0,
                        2,
                        std::mem::size_of::<ConversionStep>(),
                        offset,
                    )?;
                    operand.conversions.push(ConversionStep {
                        kind: Conversion::TransparentUnion { field },
                        target_type: union_type,
                    });
                    operand.effective_type = union_type;
                    operand.type_use = self.code_builder().plain_type_use(&union, offset)?;
                    arguments.push(operand);
                    continue;
                }
                (self.unqualified(&parameter.ty)?, Conversion::Assignment)
            } else {
                (
                    self.default_argument_type(argument)?,
                    Conversion::DefaultArgument,
                )
            };
            arguments.push(self.retained_use(
                argument,
                UseContext::Value,
                Some((destination, conversion)),
            )?);
        }
        Ok(ExprKind::Call {
            callee,
            direct_callee,
            arguments,
        })
    }

    fn default_argument_type(&mut self, argument: &Node<ast::Expression>) -> Result<Type, Error> {
        let offset = argument.span.start;
        let info = self.expression_info(argument)?;
        let ty = self.converted_type(&info, offset)?;
        Ok(match ty.kind {
            TypeKind::Float(FloatKind::Float) => Type::new(TypeKind::Float(FloatKind::Double)),
            TypeKind::Integer(_) | TypeKind::Bool | TypeKind::Enum(_) => {
                integer_to_type(self.promoted_integer(&info, offset)?)
            }
            _ => ty,
        })
    }

    fn retain_binary(
        &mut self,
        binary: &Node<ast::BinaryOperatorExpression>,
        result: &ExpressionInfo,
    ) -> Result<ExprKind, Error> {
        use ast::BinaryOperator as Op;
        let offset = binary.span.start;
        let left = self.expression_info(&binary.node.lhs)?;
        let right = self.expression_info(&binary.node.rhs)?;
        let left_value = self.converted_type(&left, offset)?;
        let right_value = self.converted_type(&right, offset)?;
        let operator = Binary::from_ast(&binary.node.operator.node);
        let assignment = matches!(
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
        );
        let mut left_destination = None;
        let mut right_destination = None;
        let mut computation = None;
        match binary.node.operator.node {
            Op::Assign => {
                right_destination =
                    Some((self.atomic_value_type(&left.ty)?, Conversion::Assignment))
            }
            Op::LogicalAnd | Op::LogicalOr | Op::Index => {}
            _ if matches!(left_value.kind, TypeKind::Vector { .. })
                || matches!(right_value.kind, TypeKind::Vector { .. }) =>
            {
                let shift = matches!(
                    binary.node.operator.node,
                    Op::ShiftLeft | Op::ShiftRight | Op::AssignShiftLeft | Op::AssignShiftRight
                );
                let common = self.vector_operands(
                    &left_value,
                    &right_value,
                    &binary.node.lhs,
                    &binary.node.rhs,
                    shift,
                )?;
                computation = Some(common.clone());
                if !matches!(left_value.kind, TypeKind::Vector { .. }) {
                    left_destination = Some((common.clone(), Conversion::VectorSplat));
                }
                if !shift {
                    if !matches!(right_value.kind, TypeKind::Vector { .. }) {
                        right_destination = Some((common, Conversion::VectorSplat));
                    } else if !self.compatible(&right_value, &common)? {
                        // Native NEON and GNU vectors have distinct C identities
                        // but convert lane values to the left operand's type.
                        right_destination = Some((common, Conversion::Arithmetic));
                    }
                }
            }
            Op::ShiftLeft | Op::ShiftRight | Op::AssignShiftLeft | Op::AssignShiftRight => {
                let left = integer_to_type(self.promoted_integer(&left, offset)?);
                let right = integer_to_type(self.promoted_integer(&right, offset)?);
                computation = Some(left.clone());
                left_destination = Some((left, Conversion::IntegerPromotion));
                right_destination = Some((right, Conversion::IntegerPromotion));
            }
            _ if self.is_arithmetic(&left_value)? && self.is_arithmetic(&right_value)? => {
                let common = self.arithmetic_type(&left, &right, offset)?;
                computation = Some(common.clone());
                left_destination = Some((common.clone(), Conversion::Arithmetic));
                right_destination = Some((common, Conversion::Arithmetic));
            }
            Op::Equals
            | Op::NotEquals
            | Op::Less
            | Op::LessOrEqual
            | Op::Greater
            | Op::GreaterOrEqual => {
                let common = match (&left_value.kind, &right_value.kind) {
                    (TypeKind::Pointer(left), TypeKind::Pointer(right)) => {
                        self.composite_pointer(left, right, offset)?.pointer()
                    }
                    (TypeKind::Pointer(_), _) => left_value.clone(),
                    (_, TypeKind::Pointer(_)) => right_value.clone(),
                    _ => {
                        return Err(Error::new(
                            offset,
                            "checked comparison has no common operand type",
                        ));
                    }
                };
                left_destination = Some((common.clone(), Conversion::Pointer));
                right_destination = Some((common, Conversion::Pointer));
            }
            _ => {
                computation = Some(result.ty.clone());
            }
        }
        let computation_type = computation
            .as_ref()
            .map(|ty| self.retained_type(ty, offset))
            .transpose()?;
        let write_back = if assignment {
            Some(self.retained_type(
                if self.unit.atomic_value(&left.ty)?.is_some() {
                    &left.ty
                } else {
                    &result.ty
                },
                offset,
            )?)
        } else {
            None
        };
        Ok(ExprKind::Binary {
            operator,
            left: self.retained_use(
                &binary.node.lhs,
                if operator == Binary::Assign {
                    UseContext::Place
                } else if assignment {
                    UseContext::ReadModifyWrite
                } else {
                    UseContext::Value
                },
                left_destination,
            )?,
            right: self.retained_use(&binary.node.rhs, UseContext::Value, right_destination)?,
            computation_type,
            write_back,
        })
    }

    fn retain_offset_of(
        &mut self,
        offset_of: &Node<ast::OffsetOfExpression>,
    ) -> Result<ExprKind, Error> {
        let offset = offset_of.span.start;
        let mut ty = self.type_name(&offset_of.node.type_name.node)?;
        let record = self.retained_type(&ty, offset)?;
        let first =
            self.retained_field_path(&ty, &offset_of.node.designator.node.base.node.name, offset)?;
        self.retain_member_reference(&ty, &first, &offset_of.node.designator.node.base)?;
        ty = self.subobject(
            &ty,
            &first.iter().map(|index| *index as u64).collect::<Vec<_>>(),
            offset,
        )?;
        let mut members = vec![OffsetMember::Field(first)];
        for member in &offset_of.node.designator.node.members {
            match &member.node {
                ast::OffsetMember::Member(name) => {
                    let path = self.retained_field_path(&ty, &name.node.name, offset)?;
                    self.retain_member_reference(&ty, &path, name)?;
                    ty = self.subobject(
                        &ty,
                        &path.iter().map(|index| *index as u64).collect::<Vec<_>>(),
                        offset,
                    )?;
                    members.push(OffsetMember::Field(path));
                }
                ast::OffsetMember::Index(index) => {
                    members.push(OffsetMember::Index(self.retained_expression_id(index)?));
                    let TypeKind::Array { element, .. } = &self.unit.resolve(&ty)?.kind else {
                        return Err(Error::new(
                            offset,
                            "retained offset index has no array type",
                        ));
                    };
                    ty = (**element).clone();
                }
                ast::OffsetMember::IndirectMember(_) => {
                    return Err(Error::new(
                        offset,
                        "indirect offsetof designator is unsupported",
                    ));
                }
            }
        }
        let field_count = members
            .iter()
            .map(|member| match member {
                OffsetMember::Field(path) => path.len(),
                OffsetMember::Index(_) => 0,
            })
            .sum::<usize>();
        self.code_builder().budget.charge(
            0,
            members.len() + field_count,
            members.len() * std::mem::size_of::<OffsetMember>()
                + field_count * std::mem::size_of::<usize>(),
            offset,
        )?;
        Ok(ExprKind::OffsetOf { record, members })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::analyze_inner;
    use crate::checked::{CheckedCode, Limits};
    use toucan_target::Target;

    fn checked(source: &str, target: Target) -> CheckedCode {
        let (unit, code) = analyze_inner(source, target, Some(Limits::default()))
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        let plain = crate::analyze(source, target).unwrap();
        assert_eq!(format!("{unit:?}"), format!("{plain:?}"));
        let code = code.unwrap();
        assert!(
            code.ambiguous_aliases.is_empty(),
            "{:#?}",
            code.ambiguous_aliases
        );
        assert!(
            !code
                .expression_coverage
                .iter()
                .any(|coverage| matches!(coverage.status, Coverage::Missing))
        );
        let mut seen = std::collections::HashSet::new();
        assert!(
            code.expressions
                .iter()
                .all(|expression| seen.insert(expression.occurrence))
        );
        code
    }

    fn ty(code: &CheckedCode, id: TypeId) -> &Type {
        &code.types[id.index()]
    }
    fn kinds(operand: &ExprUse) -> Vec<Conversion> {
        operand.conversions.iter().map(|step| step.kind).collect()
    }

    fn scalar(code: &CheckedCode, id: TypeId, kind: IntegerKind) {
        assert_eq!(ty(code, id).kind, TypeKind::Integer(kind));
    }

    #[test]
    fn byte_swaps_retain_converted_arguments_and_distinct_operations() {
        for target in Target::ALL {
            let code = checked(
                "enum { x = __builtin_bswap16(0x1234) }; void f(short value) { __builtin_bswap16(value); __builtin_bswap32(value); __builtin_bswap64(value); }",
                target,
            );
            let mut found = Vec::new();
            for expression in &code.expressions {
                let ExprKind::BuiltinCall {
                    builtin, arguments, ..
                } = &expression.kind
                else {
                    continue;
                };
                found.push(*builtin);
                assert_eq!(arguments.len(), 1);
                assert_eq!(arguments[0].context, UseContext::Value);
                assert_eq!(arguments[0].effective_type, expression.ty);
                assert_eq!(
                    arguments[0].conversions.last().unwrap().kind,
                    Conversion::Assignment
                );
                let kind = match builtin {
                    Builtin::ByteSwap16 => IntegerKind::UnsignedShort,
                    Builtin::ByteSwap32 => IntegerKind::UnsignedInt,
                    Builtin::ByteSwap64
                        if matches!(
                            target,
                            Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
                        ) =>
                    {
                        IntegerKind::UnsignedLong
                    }
                    Builtin::ByteSwap64 => IntegerKind::UnsignedLongLong,
                    _ => panic!("byte swap"),
                };
                scalar(&code, expression.ty, kind);
            }
            assert_eq!(
                found,
                [
                    Builtin::ByteSwap16,
                    Builtin::ByteSwap16,
                    Builtin::ByteSwap32,
                    Builtin::ByteSwap64
                ]
            );
        }
    }

    #[test]
    fn bit_counts_retain_target_parameter_conversions_and_int_results() {
        let operations = [
            (
                "__builtin_clz",
                Builtin::CountLeadingZeros,
                IntegerKind::UnsignedInt,
            ),
            (
                "__builtin_clzl",
                Builtin::CountLeadingZerosLong,
                IntegerKind::UnsignedLong,
            ),
            (
                "__builtin_clzll",
                Builtin::CountLeadingZerosLongLong,
                IntegerKind::UnsignedLongLong,
            ),
            (
                "__builtin_ctz",
                Builtin::CountTrailingZeros,
                IntegerKind::UnsignedInt,
            ),
            (
                "__builtin_ctzl",
                Builtin::CountTrailingZerosLong,
                IntegerKind::UnsignedLong,
            ),
            (
                "__builtin_ctzll",
                Builtin::CountTrailingZerosLongLong,
                IntegerKind::UnsignedLongLong,
            ),
        ];
        for target in Target::ALL {
            for (name, expected, parameter) in operations {
                let code = checked(
                    &format!("int f(short value) {{ return {name}(value); }}"),
                    target,
                );
                let mut found = 0;
                for expression in &code.expressions {
                    let ExprKind::BuiltinCall {
                        builtin, arguments, ..
                    } = &expression.kind
                    else {
                        continue;
                    };
                    found += 1;
                    assert_eq!(*builtin, expected);
                    scalar(&code, expression.ty, IntegerKind::Int);
                    assert_eq!(arguments.len(), 1);
                    assert_eq!(arguments[0].context, UseContext::Value);
                    scalar(&code, arguments[0].effective_type, parameter);
                    assert_eq!(
                        kinds(&arguments[0]),
                        [Conversion::Lvalue, Conversion::Assignment]
                    );
                }
                assert_eq!(found, 1);
            }
        }
    }

    #[test]
    fn memory_intrinsics_retain_argument_conversions_and_identities() {
        let source = "void f(void) { char destination[4]; const char source[4] = {1,2,3,4}; short size = 4; long byte = 255; __builtin_memset(destination, byte, size); __builtin_memcpy(destination, source, size); __builtin_memmove(destination, source, size); __builtin_memcmp(destination, source, size); }";
        for target in Target::ALL {
            let code = checked(source, target);
            let mut found = Vec::new();
            for expression in &code.expressions {
                let ExprKind::BuiltinCall {
                    builtin,
                    arguments,
                    callee_occurrence,
                    ..
                } = &expression.kind
                else {
                    continue;
                };
                found.push(*builtin);
                assert!(
                    source[code.occurrences[callee_occurrence.index()]
                        .source
                        .range
                        .clone()]
                    .starts_with("__builtin_mem")
                );
                assert_eq!(arguments.len(), 3);
                assert!(
                    arguments
                        .iter()
                        .all(|argument| argument.context == UseContext::Value)
                );
                assert_eq!(
                    kinds(&arguments[0]),
                    vec![Conversion::ArrayDecay, Conversion::Assignment]
                );
                assert_eq!(
                    kinds(&arguments[2]),
                    vec![Conversion::Lvalue, Conversion::Assignment]
                );
                scalar(
                    &code,
                    arguments[2].effective_type,
                    if target.long_width() == 64 {
                        IntegerKind::UnsignedLong
                    } else {
                        IntegerKind::UnsignedLongLong
                    },
                );
                let TypeKind::Pointer(destination) = &ty(&code, arguments[0].effective_type).kind
                else {
                    panic!("destination pointer")
                };
                assert_eq!(destination.kind, TypeKind::Void);
                assert_eq!(destination.qualifiers.is_const, *builtin == Builtin::Memcmp);
                if *builtin == Builtin::Memset {
                    scalar(&code, arguments[1].effective_type, IntegerKind::Int);
                    assert_eq!(
                        kinds(&arguments[1]),
                        vec![Conversion::Lvalue, Conversion::Assignment]
                    );
                } else {
                    let TypeKind::Pointer(source) = &ty(&code, arguments[1].effective_type).kind
                    else {
                        panic!("source pointer")
                    };
                    assert_eq!(source.kind, TypeKind::Void);
                    assert!(source.qualifiers.is_const);
                }
                assert_eq!(expression.category, ValueCategory::Value);
                if *builtin == Builtin::Memcmp {
                    scalar(&code, expression.ty, IntegerKind::Int);
                } else {
                    assert!(
                        matches!(&ty(&code, expression.ty).kind, TypeKind::Pointer(pointee) if pointee.kind == TypeKind::Void && !pointee.qualifiers.is_const)
                    );
                }
            }
            assert_eq!(
                found,
                [
                    Builtin::Memset,
                    Builtin::Memcpy,
                    Builtin::Memmove,
                    Builtin::Memcmp
                ]
            );
        }
    }

    #[test]
    fn original_string_escapes_and_prior_array_bounds_are_retained() {
        for target in Target::ALL {
            let code = checked(
                r#"extern int values[5]; int values[] = {1,2}; unsigned short emoji[] = u"\U0001F600"; char mixed[] = "\x00e9" "\u00e9";"#,
                target,
            );
            let strings: Vec<_> = code
                .expressions
                .iter()
                .filter_map(|expression| match &expression.kind {
                    ExprKind::String(value) => Some(value.code_units.as_slice()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                strings,
                [&[0xd83d, 0xde00, 0][..], &[0xe9, 0xc3, 0xa9, 0][..]]
            );
            for site in &code.declarations {
                if code.entities[site.entity.index()].name.as_deref() == Some("values") {
                    assert!(matches!(
                        ty(&code, site.ty).kind,
                        TypeKind::Array {
                            length: Some(5),
                            ..
                        }
                    ));
                }
            }
        }
    }

    #[test]
    fn call_uses_preserve_decay_prototypes_and_default_promotions() {
        for target in Target::ALL {
            let code = checked(
                "int sink(const int *, double, ...); int f(short n, float value) {int a[2]; return sink(a, value, n, value);}",
                target,
            );
            let (callee, direct, arguments) = code
                .expressions
                .iter()
                .find_map(|expression| match &expression.kind {
                    ExprKind::Call {
                        callee,
                        direct_callee,
                        arguments,
                    } => Some((callee, direct_callee, arguments)),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                code.entities[direct.unwrap().index()].name.as_deref(),
                Some("sink")
            );
            assert_eq!(kinds(callee), [Conversion::FunctionDecay]);
            let array = &code.expressions[arguments[0].expression.index()];
            assert_eq!(array.category, ValueCategory::ObjectLvalue);
            assert!(matches!(
                ty(&code, array.ty).kind,
                TypeKind::Array {
                    length: Some(2),
                    ..
                }
            ));
            assert_eq!(
                kinds(&arguments[0]),
                [Conversion::ArrayDecay, Conversion::Assignment]
            );
            let TypeKind::Pointer(pointee) = &ty(&code, arguments[0].effective_type).kind else {
                panic!("pointer")
            };
            assert!(pointee.qualifiers.is_const);
            assert_eq!(
                ty(&code, arguments[1].effective_type).kind,
                TypeKind::Float(FloatKind::Double)
            );
            assert_eq!(
                kinds(&arguments[1]),
                [Conversion::Lvalue, Conversion::Assignment]
            );
            scalar(&code, arguments[2].effective_type, IntegerKind::Int);
            assert_eq!(
                kinds(&arguments[2]),
                [Conversion::Lvalue, Conversion::DefaultArgument]
            );
            assert_eq!(
                ty(&code, arguments[3].effective_type).kind,
                TypeKind::Float(FloatKind::Double)
            );
            assert_eq!(
                kinds(&arguments[3]),
                [Conversion::Lvalue, Conversion::DefaultArgument]
            );
        }
        let code = checked(
            "int (*pointer)(int); int f(void) {return pointer(1);}",
            Target::X86_64UnknownLinuxGnu,
        );
        assert!(code.expressions.iter().any(|expression| matches!(
            expression.kind,
            ExprKind::Call {
                direct_callee: None,
                ..
            }
        )));
    }

    #[test]
    fn identifiers_resolve_before_their_own_initializer_and_after_shadowing() {
        let code = checked(
            "int x; int f(void) {int x=x; {long x=x;} return x;}",
            Target::X86_64UnknownLinuxGnu,
        );
        let references: Vec<_> = code
            .expressions
            .iter()
            .filter_map(|expression| {
                if let ExprKind::Name(entity) = expression.kind
                    && code.entities[entity.index()].name.as_deref() == Some("x")
                {
                    return Some((expression, entity));
                }
                None
            })
            .collect();
        assert_eq!(references.len(), 3);
        assert_eq!(references[0].1, references[2].1);
        assert_ne!(references[0].1, references[1].1);
        for (expression, entity) in references {
            let declaration = code
                .declarations
                .iter()
                .find(|site| site.entity == entity)
                .unwrap();
            assert_eq!(expression.scope, declaration.scope);
            assert_eq!(expression.ty, declaration.ty);
        }
        let code = checked(
            "int value; int *address = &value; void f(void) {extern int value; int *local = &value;}",
            Target::X86_64UnknownLinuxGnu,
        );
        let entities: std::collections::HashSet<_> = code
            .expressions
            .iter()
            .filter_map(|expression| match expression.kind {
                ExprKind::Name(entity)
                    if code.entities[entity.index()].name.as_deref() == Some("value") =>
                {
                    Some(entity)
                }
                _ => None,
            })
            .collect();
        assert_eq!(entities.len(), 1);
    }

    #[test]
    fn arithmetic_and_assignment_uses_keep_operation_and_storage_types() {
        let code = checked(
            "int f(short left, unsigned int right) {left += right; return left < right;}",
            Target::X86_64UnknownLinuxGnu,
        );
        for expression in &code.expressions {
            if let ExprKind::Binary {
                operator: Binary::AssignPlus,
                left,
                right,
                computation_type,
                write_back,
            } = &expression.kind
            {
                scalar(&code, left.effective_type, IntegerKind::UnsignedInt);
                scalar(&code, right.effective_type, IntegerKind::UnsignedInt);
                scalar(&code, computation_type.unwrap(), IntegerKind::UnsignedInt);
                scalar(&code, write_back.unwrap(), IntegerKind::Short);
                assert_eq!(left.context, UseContext::ReadModifyWrite);
                assert_eq!(
                    kinds(left),
                    [
                        Conversion::Lvalue,
                        Conversion::IntegerPromotion,
                        Conversion::Arithmetic
                    ]
                );
                scalar(&code, left.conversions[1].target_type, IntegerKind::Int);
            }
            if let ExprKind::Binary {
                operator: Binary::Less,
                left,
                right,
                ..
            } = &expression.kind
            {
                scalar(&code, left.effective_type, IntegerKind::UnsignedInt);
                scalar(&code, right.effective_type, IntegerKind::UnsignedInt);
                scalar(&code, expression.ty, IntegerKind::Int);
            }
        }
        let code = checked(
            "long f(int input) {long initialized = input; return input;}",
            Target::X86_64UnknownLinuxGnu,
        );
        assert_eq!(code.assignment_conversions.len(), 2);
        for assignment in &code.assignment_conversions {
            scalar(&code, assignment.effective_type, IntegerKind::Long);
            assert!(kinds(assignment).contains(&Conversion::Assignment));
        }
    }

    #[test]
    fn unevaluated_and_generic_operands_remain_typed_without_decay() {
        let code = checked(
            "int live(void); int ignored(void); int f(int condition) {int a[3]; return sizeof a + _Generic(a, int*: live(), default: ignored());}",
            Target::X86_64UnknownLinuxGnu,
        );
        let operand = code
            .expressions
            .iter()
            .find_map(|expression| match &expression.kind {
                ExprKind::SizeOfValue { operand, .. } => Some(operand),
                _ => None,
            })
            .unwrap();
        assert_eq!(operand.context, UseContext::Unevaluated);
        assert!(operand.conversions.is_empty());
        assert!(matches!(
            ty(&code, operand.effective_type).kind,
            TypeKind::Array {
                length: Some(3),
                ..
            }
        ));
        let (control, arms, selected) = code
            .expressions
            .iter()
            .find_map(|expression| match &expression.kind {
                ExprKind::Generic {
                    control,
                    arms,
                    selected,
                } => Some((control, arms, selected)),
                _ => None,
            })
            .unwrap();
        assert_eq!(*selected, 0);
        assert_eq!(arms.len(), 2);
        assert_eq!(control.context, UseContext::UnevaluatedValue);
        assert!(matches!(
            ty(&code, control.effective_type).kind,
            TypeKind::Pointer(_)
        ));
        assert_eq!(kinds(control), [Conversion::ArrayDecay]);
        for arm in arms {
            assert!(matches!(
                code.expressions[arm.expression.index()].kind,
                ExprKind::Call { .. }
            ));
        }
    }

    #[test]
    fn repeated_type_queries_keep_compound_statement_and_constant_identities() {
        let source = "enum E { value = sizeof(struct S {int field;}) }; int f(void) { return sizeof(({ label: ; struct S object = (struct S){1}; object; })); }";
        let code = checked(source, Target::X86_64UnknownLinuxGnu);
        assert_eq!(
            code.entities
                .iter()
                .filter(|entity| entity.name.as_deref() == Some("object"))
                .count(),
            1
        );
        assert_eq!(
            code.expressions
                .iter()
                .filter(|expression| matches!(
                    expression.kind,
                    ExprKind::StatementExpression { .. }
                ))
                .count(),
            1
        );
        assert_eq!(
            code.expressions
                .iter()
                .filter(|expression| matches!(expression.kind, ExprKind::CompoundLiteral { .. }))
                .count(),
            1
        );
        let code = checked(
            "struct S {int a[2];}; enum { offset = __builtin_offsetof(struct S, a[1]) }; void *f(void *p) { return &*p; }",
            Target::X86_64UnknownLinuxGnu,
        );
        assert!(
            code.expressions
                .iter()
                .any(|expression| matches!(expression.kind, ExprKind::OffsetOf { .. }))
        );
        assert!(
            code.expressions
                .iter()
                .any(|expression| matches!(expression.kind, ExprKind::AddressIndirection { .. }))
        );
    }

    #[test]
    fn literals_builtins_bitfields_and_register_objects_keep_their_facts() {
        for target in Target::ALL {
            let code = checked(
                "int f(int last, ...) {__builtin_va_list list; __builtin_va_start(list, last); int x = __builtin_va_arg(list, int); __builtin_va_end(list); return x;}",
                target,
            );
            assert!(code.expressions.iter().any(|expression| matches!(
                expression.kind,
                ExprKind::BuiltinCall {
                    builtin: Builtin::VaStart,
                    ..
                }
            )));
            assert!(
                code.expressions
                    .iter()
                    .any(|expression| matches!(expression.kind, ExprKind::VaArg { .. }))
            );
        }
        let code = checked(
            "enum {hint=__builtin_expect(3, 1)}; struct S {unsigned int bit:3;}; int f(struct S s, register int r) { const char *text=\"\\xFF\"; double x=0x1.8p1; return s.bit+r; }",
            Target::X86_64UnknownLinuxGnu,
        );
        assert!(
            code.expressions
                .iter()
                .any(|expression| expression.bitfield == Some(3))
        );
        assert!(
            code.expressions
                .iter()
                .any(|expression| expression.register)
        );
        assert!(code.expressions.iter().any(|expression| matches!(&expression.kind, ExprKind::String(string) if string.code_units == [255,0])));
        assert!(code.expressions.iter().any(|expression| matches!(
            &expression.kind,
            ExprKind::Float {
                hexadecimal: true,
                ..
            }
        )));
    }
    #[test]
    fn updates_keep_promoted_reads_and_attribute_metadata_is_explicit() {
        let code = checked(
            "int f(short value) {return value++;}",
            Target::X86_64UnknownLinuxGnu,
        );
        let expression = code
            .expressions
            .iter()
            .find(|expression| {
                matches!(
                    expression.kind,
                    ExprKind::Unary {
                        operator: Unary::PostIncrement,
                        ..
                    }
                )
            })
            .unwrap();
        let ExprKind::Unary {
            operand,
            computation_type,
            write_back,
            ..
        } = &expression.kind
        else {
            unreachable!()
        };
        scalar(&code, expression.ty, IntegerKind::Short);
        scalar(&code, computation_type.unwrap(), IntegerKind::Int);
        scalar(&code, write_back.unwrap(), IntegerKind::Short);
        assert_eq!(
            kinds(operand),
            [Conversion::Lvalue, Conversion::IntegerPromotion]
        );
        let code = checked(
            "int log_message(const char *, ...) __attribute__((format(printf,1,2),nonnull(1)));",
            Target::X86_64UnknownLinuxGnu,
        );
        assert!(!code.expression_coverage.is_empty());
        assert!(
            code.expression_coverage
                .iter()
                .all(|coverage| matches!(coverage.status, Coverage::AttributeArgument))
        );
    }
    #[test]
    fn folded_and_initializer_expressions_have_coverage_and_insertions_are_explicit() {
        let code = checked(
            "enum { A = 1 ? 2 : 3, B = 0 && 4 }; int a[4] = {[2] = 5}; char s[] = \"abc\"; struct S { int x; }; struct S object = (struct S){}; int f(void) { return sizeof(_Generic(a, int *: a[0], default: s[0])); }",
            Target::X86_64UnknownLinuxGnu,
        );
        // The empty initializer adapter inserts list syntax, not a fabricated
        // value expression. Its synthetic occurrence remains distinguishable.
        assert!(
            code.occurrences
                .iter()
                .any(|occurrence| occurrence.source.synthetic)
        );
        assert_eq!(
            code.expression_coverage.len(),
            code.occurrences
                .iter()
                .filter(|occurrence| occurrence.kind == OccurrenceKind::Expression)
                .count()
        );
    }

    #[test]
    fn decoded_expression_payload_is_charged_to_retention_budget() {
        let source = format!("int f(void) {{ return \"{}\"[0]; }}", "a".repeat(512));
        checked(&source, Target::X86_64UnknownLinuxGnu);
        let error = analyze_inner(
            &source,
            Target::X86_64UnknownLinuxGnu,
            Some(Limits {
                payload_bytes: 1_024,
                ..Limits::default()
            }),
        )
        .unwrap_err();
        assert_eq!(error.offset, source.find('"').unwrap());
        assert!(error.message.contains("retention payload byte limit"));
    }

    #[test]
    fn statement_results_keep_the_final_expression_and_profile_conversions() {
        for target in Target::ALL {
            let gnu = matches!(
                target,
                Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
            );
            let code = checked(
                "int f(int x) { int a[1]; ({ x; }); ({ x; _Static_assert(1, \"ok\"); }); ({ a; }); return ({ int x = 2; x; }); }",
                target,
            );
            let results: Vec<_> = code
                .expressions
                .iter()
                .filter_map(|expression| match &expression.kind {
                    ExprKind::StatementExpression { result, .. } => Some((expression, result)),
                    _ => None,
                })
                .collect();
            assert_eq!(results.len(), 4);
            let first = results[0].1.as_ref().unwrap();
            assert_eq!(
                first.context,
                if gnu {
                    UseContext::Place
                } else {
                    UseContext::Value
                }
            );
            assert_eq!(
                kinds(first),
                if gnu {
                    vec![]
                } else {
                    vec![Conversion::Lvalue]
                }
            );
            assert_eq!(results[1].1.is_some(), gnu);
            assert_eq!(
                kinds(results[2].1.as_ref().unwrap()),
                [Conversion::ArrayDecay]
            );
            let last = results[3].1.as_ref().unwrap();
            assert_eq!(kinds(last), [Conversion::Lvalue]);
            let local = &code.expressions[last.expression.index()];
            assert_ne!(local.scope, results[3].0.scope);
            let ExprKind::Name(entity) = local.kind else {
                panic!("expected the block-local x");
            };
            assert_eq!(code.entities[entity.index()].kind, EntityKind::Variable);
        }
    }

    #[test]
    fn pointer_comparisons_keep_pointer_and_null_conversions() {
        let code = checked(
            "int f(int *p, const int *q) { return p == q || p == 0; }",
            Target::X86_64UnknownLinuxGnu,
        );
        let comparisons: Vec<_> = code
            .expressions
            .iter()
            .filter_map(|expression| match &expression.kind {
                ExprKind::Binary {
                    operator: Binary::Equals,
                    left,
                    right,
                    ..
                } => Some((left, right)),
                _ => None,
            })
            .collect();
        assert_eq!(comparisons.len(), 2);
        assert_eq!(
            kinds(comparisons[0].0),
            [Conversion::Lvalue, Conversion::Pointer]
        );
        assert_eq!(kinds(comparisons[0].1), [Conversion::Lvalue]);
        assert_eq!(kinds(comparisons[1].1), [Conversion::Pointer]);
        assert_eq!(
            comparisons[0].0.effective_type,
            comparisons[0].1.effective_type
        );
    }

    #[test]
    #[ignore = "requires native GCC and Clang; run with --include-ignored"]
    fn retained_promotions_match_native_c_calls() {
        let source = r#"
            _Static_assert(__builtin_types_compatible_p(__typeof__((short)0 + (unsigned int)0), unsigned int), "arithmetic conversion");
            _Static_assert(__builtin_types_compatible_p(__typeof__((short)0 << (long long)0), int), "shift promotion");
            int sink(const int *pointer, double fixed, ...) {
                __builtin_va_list arguments;
                __builtin_va_start(arguments, fixed);
                int narrow = __builtin_va_arg(arguments, int);
                double widened = __builtin_va_arg(arguments, double);
                __builtin_va_end(arguments);
                return *pointer == 7 && fixed == 1.25 && narrow == -4 && widened == 1.25;
            }
            int main(void) {
                int array[1] = {7};
                short narrow = -4;
                float value = 1.25f;
                return !sink(array, value, narrow, value);
            }
        "#;
        // The __builtin_types_compatible_p oracle is not part of the current
        // frontend syntax; the checked call uses the same C body below it.
        let body = &source[source.find("int sink").unwrap()..];
        let code = checked(body, Target::X86_64UnknownLinuxGnu);
        let arguments = code
            .expressions
            .iter()
            .find_map(|expression| match &expression.kind {
                ExprKind::Call {
                    arguments,
                    direct_callee: Some(entity),
                    ..
                } if code.entities[entity.index()].name.as_deref() == Some("sink") => {
                    Some(arguments)
                }
                _ => None,
            })
            .unwrap();
        scalar(&code, arguments[2].effective_type, IntegerKind::Int);
        assert_eq!(
            ty(&code, arguments[3].effective_type).kind,
            TypeKind::Float(FloatKind::Double)
        );
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("promotions.c");
        std::fs::write(&input, format!("{source}\n")).unwrap();
        for compiler in ["gcc", "clang"] {
            let executable = directory.path().join(format!("{compiler}.exe"));
            let output = std::process::Command::new(compiler)
                .args(["-std=c11", "-pedantic-errors"])
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

//! Checked integer arithmetic, before destination-width truncation.

use lang_c::{ast, span::Node};
use serde::Serialize;

use crate::analyze::Analyzer;
use crate::checked::Conversion;
use crate::{Error, IntegerKind, IntegerValue, Type, TypeKind};

/// The mathematical operation performed before overflow detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum OverflowOperation {
    Add,
    Subtract,
    Multiply,
}

/// Source-level overflow intrinsic families.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum OverflowForm {
    /// Preserve each input's integer type; argument two points to the result.
    GenericStore,
    /// Convert inputs and the result pointer to this explicit prototype type.
    TypedStore(IntegerKind),
    /// GNU `_p`: discard argument two's value while preserving its effects.
    /// Its written bitfield precision, when present, replaces the type's width.
    Predicate,
}

/// Overflow checks use infinite signed precision, then truncate to the result
/// type. Generic Clang Boolean results use one-bit precision, independently of
/// their eight-bit object representation. Every operation returns C `_Bool`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct OverflowIntrinsic {
    operation: OverflowOperation,
    form: OverflowForm,
}
impl OverflowIntrinsic {
    /// Mathematical operation before conversion to the result type.
    pub fn operation(self) -> OverflowOperation {
        self.operation
    }
    /// Whether the intrinsic stores a result or only checks representability.
    pub fn form(self) -> OverflowForm {
        self.form
    }
    /// Predicate forms evaluate effects in argument two without using its value.
    pub fn is_predicate(self) -> bool {
        self.form == OverflowForm::Predicate
    }
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        let name = name.strip_prefix("__builtin_")?;
        let (name, predicate) = if let Some(name) = name.strip_suffix("_overflow_p") {
            (name, true)
        } else {
            (name.strip_suffix("_overflow")?, false)
        };
        let parse = |name| {
            Some(match name {
                "add" => OverflowOperation::Add,
                "sub" => OverflowOperation::Subtract,
                "mul" => OverflowOperation::Multiply,
                _ => return None,
            })
        };
        if let Some(operation) = parse(name) {
            return Some(Self {
                operation,
                form: if predicate {
                    OverflowForm::Predicate
                } else {
                    OverflowForm::GenericStore
                },
            });
        }
        if predicate {
            return None;
        }
        let (signed, name) = if let Some(name) = name.strip_prefix('s') {
            (true, name)
        } else {
            (false, name.strip_prefix('u')?)
        };
        let (operation, suffix) = if let Some(suffix) = name.strip_prefix("add") {
            (OverflowOperation::Add, suffix)
        } else if let Some(suffix) = name.strip_prefix("sub") {
            (OverflowOperation::Subtract, suffix)
        } else {
            (OverflowOperation::Multiply, name.strip_prefix("mul")?)
        };
        let kind = match (signed, suffix) {
            (true, "") => IntegerKind::Int,
            (false, "") => IntegerKind::UnsignedInt,
            (true, "l") => IntegerKind::Long,
            (false, "l") => IntegerKind::UnsignedLong,
            (true, "ll") => IntegerKind::LongLong,
            (false, "ll") => IntegerKind::UnsignedLongLong,
            _ => return None,
        };
        Some(Self {
            operation,
            form: OverflowForm::TypedStore(kind),
        })
    }
}

pub(crate) struct OverflowSignature {
    pub(crate) parameters: [Option<Type>; 3],
    pub(crate) conversions: [Conversion; 3],
    sources: [Type; 3],
    precision: u8,
    signed: bool,
}

impl Analyzer {
    /// Shared by source checking and retained argument conversions.
    pub(crate) fn overflow_signature(
        &mut self,
        intrinsic: OverflowIntrinsic,
        call: &Node<ast::CallExpression>,
    ) -> Result<OverflowSignature, Error> {
        let offset = call.span.start;
        if call.node.arguments.len() != 3 {
            return Err(Error::new(
                offset,
                "overflow intrinsic requires three arguments",
            ));
        }
        if intrinsic.is_predicate() && !self.gnu_sync_profile() {
            return Err(Error::new(
                offset,
                "GNU overflow predicates are unsupported by this Clang profile",
            ));
        }
        let left = self.value_expression_type(&call.node.arguments[0])?;
        let right = self.value_expression_type(&call.node.arguments[1])?;
        let result_info = self.expression_info(&call.node.arguments[2])?;
        let result = self.converted_type(&result_info, call.node.arguments[2].span.start)?;
        let mut signature = OverflowSignature {
            parameters: std::array::from_fn(|_| None),
            conversions: [Conversion::Assignment; 3],
            sources: [left, right, result],
            precision: 0,
            signed: false,
        };
        let destination = match intrinsic.form {
            OverflowForm::TypedStore(kind) => {
                let ty = Type::new(TypeKind::Integer(kind));
                signature.parameters = [
                    Some(ty.clone()),
                    Some(ty.clone()),
                    Some(ty.clone().pointer()),
                ];
                if self.gnu_sync_profile() {
                    signature.conversions = [Conversion::IntrinsicArgument; 3];
                }
                ty
            }
            OverflowForm::GenericStore | OverflowForm::Predicate => {
                for source in &signature.sources[..2] {
                    self.integer_type(source, offset).map_err(|_| {
                        Error::new(offset, "generic overflow operands must have integer types")
                    })?;
                }
                if intrinsic.is_predicate() {
                    let ty = &signature.sources[2];
                    if matches!(ty.kind, TypeKind::Bool)
                        || matches!(ty.kind, TypeKind::Enum(_)) && result_info.bitfield.is_none()
                    {
                        return Err(Error::new(
                            offset,
                            "overflow predicate result expression cannot have Boolean or ordinary enum type",
                        ));
                    }
                    self.integer_type(ty, offset)?;
                    ty.clone()
                } else {
                    let TypeKind::Pointer(pointee) = &signature.sources[2].kind else {
                        return Err(Error::new(
                            offset,
                            "overflow result must be a pointer to an integer object",
                        ));
                    };
                    if self.unit.qualifiers(pointee)?.is_const {
                        return Err(Error::new(
                            offset,
                            "generic overflow result cannot be const-qualified",
                        ));
                    }
                    let ty = self.unqualified(pointee)?;
                    if self.gnu_sync_profile()
                        && matches!(ty.kind, TypeKind::Bool | TypeKind::Enum(_))
                    {
                        return Err(Error::new(
                            offset,
                            "GCC generic overflow results cannot have Boolean or enum type",
                        ));
                    }
                    self.integer_type(&ty, offset).map_err(|_| {
                        Error::new(offset, "overflow result must point to an integer object")
                    })?;
                    ty
                }
            }
        };
        let value = self.integer_type(&destination, offset)?;
        signature.precision = if intrinsic.is_predicate() {
            result_info
                .bitfield
                .map(|width| {
                    u8::try_from(width)
                        .map_err(|_| Error::new(offset, "overflow bitfield width exceeds 128 bits"))
                })
                .transpose()?
                .unwrap_or(value.bits)
        } else if matches!(destination.kind, TypeKind::Bool) {
            1
        } else {
            value.bits
        };
        if signature.precision == 0 || signature.precision > value.bits {
            return Err(Error::new(
                offset,
                "overflow result precision is outside its integer type",
            ));
        }
        signature.signed = value.signed;
        Ok(signature)
    }

    pub(crate) fn overflow_call_type(
        &mut self,
        intrinsic: OverflowIntrinsic,
        call: &Node<ast::CallExpression>,
    ) -> Result<Type, Error> {
        let key = (call.span.start, call.span.end);
        if intrinsic.is_predicate() && self.checked_overflow_predicates.contains_key(&key) {
            return Ok(Type::new(TypeKind::Bool));
        }
        let signature = self.overflow_signature(intrinsic, call)?;
        for (index, argument) in call.node.arguments.iter().enumerate() {
            if let Some(destination) = &signature.parameters[index] {
                let source = &signature.sources[index];
                if signature.conversions[index] == Conversion::IntrinsicArgument {
                    if !(self.is_arithmetic(source)? || matches!(source.kind, TypeKind::Pointer(_)))
                        || matches!(destination.kind, TypeKind::Pointer(_))
                            && matches!(source.kind, TypeKind::Float(_))
                    {
                        return Err(Error::new(
                            argument.span.start,
                            "incompatible typed overflow intrinsic argument",
                        ));
                    }
                } else {
                    self.check_assignment_type(destination, source, argument)?;
                    if self.checked.is_some() {
                        self.retain_assignment(argument, destination)?;
                    }
                }
            }
        }
        if intrinsic.is_predicate() {
            if self.checked_overflow_predicates.len() >= 65_536 {
                return Err(Error::new(
                    call.span.start,
                    "overflow predicate count exceeds the 65536-entry limit",
                ));
            }
            self.checked_overflow_predicates
                .insert(key, (signature.precision, signature.signed));
        }
        Ok(Type::new(TypeKind::Bool))
    }

    pub(crate) fn eval_overflow_predicate(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<IntegerValue, Error> {
        let intrinsic = self
            .builtin_name(call)
            .and_then(OverflowIntrinsic::from_name)
            .expect("overflow predicate");
        self.overflow_call_type(intrinsic, call)?;
        let (precision, signed) =
            self.checked_overflow_predicates[&(call.span.start, call.span.end)];
        if self.overflow_discarded_effects(&call.node.arguments[2], 0)? != Some(false) {
            return Err(Error::new(
                call.span.start,
                "overflow predicate result expression has runtime or unresolved effects",
            ));
        }
        let left = self.eval(&call.node.arguments[0])?;
        let right = self.eval(&call.node.arguments[1])?;
        let (_, overflow) = calculate(intrinsic.operation, left, right, precision, signed);
        Ok(IntegerValue::new(u128::from(overflow), 8, false, 0))
    }

    /// Classify only effects, without evaluating ignored arithmetic or plain
    /// loads. Runtime-sized type operands and unknown calls remain explicit.
    fn overflow_discarded_effects(
        &mut self,
        expression: &Node<ast::Expression>,
        depth: usize,
    ) -> Result<Option<bool>, Error> {
        use ast::{BinaryOperator as B, UnaryOperator as U};
        if depth >= 128 {
            return Ok(None);
        }
        let recurse = |this: &mut Self, value: &Node<ast::Expression>| {
            this.overflow_discarded_effects(value, depth + 1)
        };
        Ok(match &expression.node {
            ast::Expression::Constant(_)
            | ast::Expression::StringLiteral(_)
            | ast::Expression::AlignOf(_) => Some(false),
            ast::Expression::Identifier(_) => {
                let ty = self.expression_type(expression)?;
                Some(
                    !matches!(
                        self.unit.resolve(&ty)?.kind,
                        TypeKind::Array { .. }
                            | TypeKind::VariableArray { .. }
                            | TypeKind::Function(_)
                    ) && (self.unit.qualifiers(&ty)?.is_volatile
                        || self.unit.atomic_value(&ty)?.is_some()),
                )
            }
            ast::Expression::Cast(cast) => {
                let ty = self.type_name(&cast.node.type_name.node)?;
                if self.unit.is_variably_modified(&ty)? {
                    None
                } else {
                    recurse(self, &cast.node.expression)?
                }
            }
            ast::Expression::SizeOfTy(size) => {
                let ty = self.type_name(&size.node.0.node)?;
                if self.unit.is_variable_length_array(&ty)? {
                    None
                } else {
                    Some(false)
                }
            }
            ast::Expression::SizeOfVal(size) => {
                let ty = self.expression_type(&size.node.0)?;
                if self.unit.is_variable_length_array(&ty)? {
                    None
                } else {
                    Some(false)
                }
            }
            ast::Expression::UnaryOperator(unary) => match unary.node.operator.node {
                U::PreIncrement | U::PreDecrement | U::PostIncrement | U::PostDecrement => {
                    Some(true)
                }
                U::Indirection => {
                    let ty = self.expression_type(expression)?;
                    combine(
                        Some(
                            self.unit.qualifiers(&ty)?.is_volatile
                                || self.unit.atomic_value(&ty)?.is_some(),
                        ),
                        recurse(self, &unary.node.operand)?,
                    )
                }
                U::Address => self.overflow_place_effects(&unary.node.operand, depth + 1)?,
                _ => recurse(self, &unary.node.operand)?,
            },
            ast::Expression::Member(member) => {
                let ty = self.expression_type(expression)?;
                combine(
                    Some(
                        self.unit.qualifiers(&ty)?.is_volatile
                            || self.unit.atomic_value(&ty)?.is_some(),
                    ),
                    if member.node.operator.node == ast::MemberOperator::Indirect {
                        recurse(self, &member.node.expression)?
                    } else {
                        self.overflow_place_effects(&member.node.expression, depth + 1)?
                    },
                )
            }
            ast::Expression::BinaryOperator(binary) => {
                let operator = &binary.node.operator.node;
                if matches!(
                    operator,
                    B::Assign
                        | B::AssignPlus
                        | B::AssignMinus
                        | B::AssignMultiply
                        | B::AssignDivide
                        | B::AssignModulo
                        | B::AssignBitwiseAnd
                        | B::AssignBitwiseOr
                        | B::AssignBitwiseXor
                        | B::AssignShiftLeft
                        | B::AssignShiftRight
                ) {
                    Some(true)
                } else {
                    let left = recurse(self, &binary.node.lhs)?;
                    let short = matches!(operator, B::LogicalAnd | B::LogicalOr)
                        && self.eval_arithmetic(&binary.node.lhs).is_ok_and(|value| {
                            (*operator == B::LogicalAnd && !value.truth())
                                || (*operator == B::LogicalOr && value.truth())
                        });
                    if short {
                        left
                    } else {
                        let effects = combine(left, recurse(self, &binary.node.rhs)?);
                        if *operator == B::Index {
                            let ty = self.expression_type(expression)?;
                            combine(
                                effects,
                                Some(
                                    self.unit.qualifiers(&ty)?.is_volatile
                                        || self.unit.atomic_value(&ty)?.is_some(),
                                ),
                            )
                        } else {
                            effects
                        }
                    }
                }
            }
            ast::Expression::Conditional(conditional) => {
                let condition = recurse(self, &conditional.node.condition)?;
                let branches = if let Ok(value) = self.eval_arithmetic(&conditional.node.condition)
                {
                    if value.truth() {
                        match &conditional.node.then_expression {
                            Some(value) => recurse(self, value)?,
                            None => Some(false),
                        }
                    } else {
                        recurse(self, &conditional.node.else_expression)?
                    }
                } else {
                    combine(
                        match &conditional.node.then_expression {
                            Some(value) => recurse(self, value)?,
                            None => Some(false),
                        },
                        recurse(self, &conditional.node.else_expression)?,
                    )
                };
                combine(condition, branches)
            }
            ast::Expression::Choose(selection) => {
                let selected = self.checked_choose_expression(selection)?;
                recurse(self, selected)?
            }
            ast::Expression::ConvertVector(conversion) => {
                recurse(self, &conversion.node.expression)?
            }
            ast::Expression::TypesCompatible(_) => Some(false),
            ast::Expression::GenericSelection(selection) => {
                let index = self
                    .generic_selections
                    .get(&(selection.span.start, selection.span.end))
                    .copied()
                    .ok_or_else(|| {
                        Error::new(
                            selection.span.start,
                            "overflow predicate generic selection was not checked",
                        )
                    })?;
                let selected = match &selection.node.associations[index].node {
                    ast::GenericAssociation::Type(ty) => &ty.node.expression,
                    ast::GenericAssociation::Default(value) => value,
                };
                recurse(self, selected)?
            }
            ast::Expression::Comma(expressions) => {
                let mut effects = Some(false);
                for value in expressions.iter() {
                    effects = combine(effects, recurse(self, value)?);
                }
                effects
            }
            ast::Expression::Call(call) => {
                let name = self.builtin_name(call);
                if matches!(
                    name,
                    Some(
                        "__builtin_constant_p"
                            | "__builtin_object_size"
                            | "__atomic_always_lock_free"
                    )
                ) {
                    Some(false)
                } else if name.is_some_and(|name| {
                    crate::elementwise::ElementwiseOperation::from_name(name).is_some()
                        || self.byte_swap_type(name).is_some()
                        || self.bit_count_type(name).is_some()
                        || self.infinity_builtin_kind(name).is_some()
                        || self.nan_builtin(name).is_some()
                        || name == "__builtin_complex"
                        || name == "__builtin_expect"
                        || name == "__atomic_is_lock_free"
                        || name == "__c11_atomic_is_lock_free"
                        || OverflowIntrinsic::from_name(name)
                            .is_some_and(OverflowIntrinsic::is_predicate)
                }) {
                    let mut effects = Some(false);
                    for argument in &call.node.arguments {
                        effects = combine(effects, recurse(self, argument)?);
                    }
                    effects
                } else {
                    None
                }
            }
            _ => None,
        })
    }

    fn overflow_place_effects(
        &mut self,
        expression: &Node<ast::Expression>,
        depth: usize,
    ) -> Result<Option<bool>, Error> {
        if depth >= 128 {
            return Ok(None);
        }
        match &expression.node {
            ast::Expression::Identifier(_) => Ok(Some(false)),
            ast::Expression::Member(member) => {
                if member.node.operator.node == ast::MemberOperator::Indirect {
                    self.overflow_discarded_effects(&member.node.expression, depth + 1)
                } else {
                    self.overflow_place_effects(&member.node.expression, depth + 1)
                }
            }
            ast::Expression::BinaryOperator(binary)
                if binary.node.operator.node == ast::BinaryOperator::Index =>
            {
                Ok(combine(
                    self.overflow_discarded_effects(&binary.node.lhs, depth + 1)?,
                    self.overflow_discarded_effects(&binary.node.rhs, depth + 1)?,
                ))
            }
            ast::Expression::UnaryOperator(unary)
                if unary.node.operator.node == ast::UnaryOperator::Indirection =>
            {
                self.overflow_discarded_effects(&unary.node.operand, depth + 1)
            }
            _ => self.overflow_discarded_effects(expression, depth + 1),
        }
    }
}

fn combine(left: Option<bool>, right: Option<bool>) -> Option<bool> {
    match (left, right) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (None, _) | (_, None) => None,
        _ => Some(false),
    }
}

/// A sign and 128-bit magnitude plus a high-result flag are enough: every
/// destination has at most 128 value bits. No full 256-bit product is needed.
fn calculate(
    operation: OverflowOperation,
    left: IntegerValue,
    right: IntegerValue,
    precision: u8,
    signed: bool,
) -> (u128, bool) {
    let magnitude = |value: IntegerValue| {
        if value.signed && value.signed_value() < 0 {
            (true, value.signed_value().unsigned_abs())
        } else {
            (false, value.value)
        }
    };
    let (left_negative, left) = magnitude(left);
    let (mut right_negative, right) = magnitude(right);
    if operation == OverflowOperation::Subtract && right != 0 {
        right_negative = !right_negative;
    }
    let (negative, low, high) = if operation == OverflowOperation::Multiply {
        let (low, high) = left.overflowing_mul(right);
        (left_negative != right_negative, low, high)
    } else if left_negative == right_negative {
        let (low, high) = left.overflowing_add(right);
        (left_negative, low, high)
    } else if left >= right {
        (left_negative, left - right, false)
    } else {
        (right_negative, right - left, false)
    };
    let negative = negative && (low != 0 || high);
    let limit = if signed {
        (1u128 << (precision - 1)) - u128::from(!negative)
    } else if precision == 128 {
        u128::MAX
    } else {
        (1u128 << precision) - 1
    };
    let overflow = high || (negative && !signed) || low > limit;
    let stored = if negative { low.wrapping_neg() } else { low };
    let stored = if precision == 128 {
        stored
    } else {
        stored & ((1 << precision) - 1)
    };
    (stored, overflow)
}

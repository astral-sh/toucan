//! Bounded fixed-vector folds, separate from static-initializer admissibility.

use lang_c::{ast, span::Node};
use serde::Serialize;

use crate::analyze::Analyzer;
use crate::floating::ArithmeticValue;
use crate::integer::signed_result;
use crate::{
    ArithmeticConstant, Error, FloatKind, IntegerKind, IntegerValue, Qualifiers, TranslationUnit,
    Type, TypeKind, VectorKind,
};

/// The resolved scalar kind of a vector lane. Its encoded width and floating
/// format are carried by the corresponding [`ArithmeticConstant`] values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum VectorElement {
    Integer(IntegerKind),
    Floating(FloatKind),
}

/// An owner-independent vector value/storage shape. This snapshot preserves
/// resolved kinds, qualifiers and effective alignment, without exporting IDs or
/// typedef ancestry from the evaluator's temporary translation unit. Complete
/// source type identity belongs to the original analysis and retained graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct VectorConstantType {
    kind: VectorKind,
    element: VectorElement,
    qualifiers: Qualifiers,
    atomic: bool,
    alignment_bytes: u64,
    size_bytes: u64,
    lane_count: u64,
}

impl VectorConstantType {
    /// Returns the GNU or nominal NEON vector flavor.
    pub fn kind(self) -> VectorKind {
        self.kind
    }
    /// Returns the resolved scalar lane kind.
    pub fn element(self) -> VectorElement {
        self.element
    }
    /// Returns the expression type's outer C qualifiers.
    pub fn qualifiers(self) -> Qualifiers {
        self.qualifiers
    }
    /// Reports an atomic-qualified result before value conversion.
    pub fn is_atomic(self) -> bool {
        self.atomic
    }
    /// Returns the effective C type alignment, including typedef attributes.
    pub fn alignment_bytes(self) -> u64 {
        self.alignment_bytes
    }
    /// Returns the target storage size of the complete vector.
    pub fn size_bytes(self) -> u64 {
        self.size_bytes
    }
    /// Returns the number of scalar lanes.
    pub fn lane_count(self) -> u64 {
        self.lane_count
    }
}

/// A folded fixed-size vector, with lanes in source order. Its shape and target
/// encodings remain usable after the evaluation environment is dropped. These
/// values do not establish a Rust ABI or static-initializer admissibility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VectorConstant {
    ty: VectorConstantType,
    lanes: Vec<ArithmeticConstant>,
}

impl VectorConstant {
    /// Returns the resolved value/storage shape, before outer value conversion.
    pub fn ty(&self) -> &VectorConstantType {
        &self.ty
    }

    /// Returns each lane's target-encoded arithmetic value in source order.
    pub fn lanes(&self) -> &[ArithmeticConstant] {
        &self.lanes
    }
}

pub(crate) struct VectorValue {
    ty: Type,
    lanes: Vec<ArithmeticValue>,
}

impl VectorValue {
    pub(crate) fn into_constant(
        self,
        unit: &TranslationUnit,
        offset: usize,
    ) -> Result<VectorConstant, Error> {
        let value_type = unit.atomic_value(&self.ty)?.unwrap_or(&self.ty);
        let TypeKind::Vector {
            kind,
            element,
            lanes,
        } = &unit.resolve(value_type)?.kind
        else {
            return Err(Error::new(offset, "constant result is not a fixed vector"));
        };
        let element = match unit.resolve(element)?.kind {
            TypeKind::Integer(kind) => VectorElement::Integer(kind),
            TypeKind::Float(kind) => VectorElement::Floating(kind),
            _ => {
                return Err(Error::new(
                    offset,
                    "constant vector has an invalid scalar lane type",
                ));
            }
        };
        let ty = VectorConstantType {
            kind: *kind,
            element,
            qualifiers: unit.qualifiers(&self.ty)?,
            atomic: unit.atomic_value(&self.ty)?.is_some(),
            alignment_bytes: unit.alignment(&self.ty)?,
            size_bytes: unit.layout(&self.ty)?.size_bytes(),
            lane_count: *lanes,
        };
        Ok(VectorConstant {
            ty,
            lanes: self
                .lanes
                .into_iter()
                .map(|lane| lane.into_constant(unit.target, offset))
                .collect::<Result<_, _>>()?,
        })
    }
}

/// One fold visits at most 65,536 vector expressions and lanes. Every vector is also
/// bounded to the frontend's 16-byte fixed-vector limit before allocating lanes.
struct Budget(usize);

impl Budget {
    fn charge(&mut self, amount: usize, offset: usize) -> Result<(), Error> {
        self.0 = self.0.checked_sub(amount).ok_or_else(|| {
            Error::new(
                offset,
                "vector constant evaluation exceeds the 65536-step limit",
            )
        })?;
        Ok(())
    }
}

impl Analyzer {
    pub(crate) fn eval_vector(
        &mut self,
        expression: &Node<ast::Expression>,
        initializer: bool,
    ) -> Result<VectorValue, Error> {
        self.vector_constant(expression, initializer, &mut Budget(65_536))
    }

    fn vector_constant(
        &mut self,
        expression: &Node<ast::Expression>,
        initializer: bool,
        budget: &mut Budget,
    ) -> Result<VectorValue, Error> {
        budget.charge(1, expression.span.start)?;
        self.enter_expression(expression.span.start)?;
        let result = self.vector_constant_inner(expression, initializer, budget);
        self.leave_expression();
        result
    }

    fn vector_constant_inner(
        &mut self,
        expression: &Node<ast::Expression>,
        initializer: bool,
        budget: &mut Budget,
    ) -> Result<VectorValue, Error> {
        use ast::{BinaryOperator as Binary, UnaryOperator as Unary};
        let offset = expression.span.start;
        // This checks both conditional operands and every type operand before
        // selecting values. Retention reuses the original checked expression.
        let ty = self.expression_type(expression)?;
        let (element, count) = self.vector_constant_shape(&ty, offset)?;
        budget.charge(count, offset)?;
        let unsupported = || Error::new(offset, "expression is not a supported vector constant");
        let lanes = match &expression.node {
            ast::Expression::CompoundLiteral(literal) => {
                // Initializer checking has already rejected designators, nested
                // braces, excess lanes, and invalid scalar conversions.
                self.check_initializer_list(
                    &ty,
                    &literal.node.initializer_list,
                    expression,
                    literal.span,
                    true,
                )?;
                let zero = self.convert_arithmetic(
                    ArithmeticValue::Integer(IntegerValue::int(0)),
                    &element,
                    offset,
                )?;
                let mut lanes = vec![zero; count];
                for (index, item) in literal.node.initializer_list.iter().enumerate() {
                    if self.empty_initializers.contains(&item.span.start) {
                        continue;
                    }
                    let ast::Initializer::Expression(value) = &item.node.initializer.node else {
                        return Err(unsupported());
                    };
                    let value = self.eval_arithmetic(value)?;
                    lanes[index] = self.convert_arithmetic(value, &element, item.span.start)?;
                }
                lanes
            }
            ast::Expression::Conditional(conditional) => {
                if initializer {
                    self.check_static_arithmetic(&conditional.node.condition)?;
                }
                let condition = self.eval_arithmetic(&conditional.node.condition)?;
                self.vector_constant(
                    if condition.truth() {
                        &conditional.node.then_expression
                    } else {
                        &conditional.node.else_expression
                    },
                    initializer,
                    budget,
                )?
                .lanes
            }
            ast::Expression::Choose(selection) => {
                let selected = self.choose_expression(selection)?;
                self.vector_constant(selected, initializer, budget)?.lanes
            }
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                self.vector_constant(selected, initializer, budget)?.lanes
            }
            ast::Expression::UnaryOperator(unary)
                if matches!(
                    unary.node.operator.node,
                    Unary::Plus | Unary::Minus | Unary::Complement
                ) =>
            {
                if initializer
                    && self.gnu_vector_profile()
                    && unary.node.operator.node != Unary::Plus
                {
                    return Err(Error::new(
                        offset,
                        "GNU static vector arithmetic is not a constant initializer",
                    ));
                }
                let mut lanes = self
                    .vector_constant(&unary.node.operand, initializer, budget)?
                    .lanes;
                for lane in &mut lanes {
                    *lane = match (&*lane, &unary.node.operator.node) {
                        (value, Unary::Plus) => *value,
                        (ArithmeticValue::Integer(value), operator) => {
                            let result = match operator {
                                Unary::Minus if value.signed => signed_result(
                                    value.signed_value().checked_neg(),
                                    *value,
                                    offset,
                                )?,
                                Unary::Minus => IntegerValue::new(
                                    value.value.wrapping_neg(),
                                    value.bits,
                                    value.signed,
                                    value.rank,
                                ),
                                Unary::Complement => IntegerValue::new(
                                    !value.value,
                                    value.bits,
                                    value.signed,
                                    value.rank,
                                ),
                                _ => return Err(unsupported()),
                            };
                            ArithmeticValue::Integer(result)
                        }
                        (
                            ArithmeticValue::Floating {
                                value,
                                kind,
                                signaling,
                            },
                            Unary::Minus,
                        ) => ArithmeticValue::Floating {
                            value: -*value,
                            kind: *kind,
                            signaling: *signaling,
                        },
                        _ => return Err(unsupported()),
                    };
                }
                lanes
            }
            ast::Expression::BinaryOperator(binary)
                if matches!(
                    binary.node.operator.node,
                    Binary::Plus
                        | Binary::Minus
                        | Binary::Multiply
                        | Binary::Divide
                        | Binary::Modulo
                        | Binary::BitwiseAnd
                        | Binary::BitwiseOr
                        | Binary::BitwiseXor
                        | Binary::ShiftLeft
                        | Binary::ShiftRight
                        | Binary::Equals
                        | Binary::NotEquals
                        | Binary::Less
                        | Binary::LessOrEqual
                        | Binary::Greater
                        | Binary::GreaterOrEqual
                ) =>
            {
                if initializer && self.gnu_vector_profile() {
                    return Err(Error::new(
                        offset,
                        "GNU static vector arithmetic is not a constant initializer",
                    ));
                }
                let left_ty = self.value_expression_type(&binary.node.lhs)?;
                let right_ty = self.value_expression_type(&binary.node.rhs)?;
                let source_ty = if matches!(left_ty.kind, TypeKind::Vector { .. }) {
                    &left_ty
                } else {
                    &right_ty
                };
                let (source_element, _) = self.vector_constant_shape(source_ty, offset)?;
                let mut left = self.vector_constant_operand(
                    &binary.node.lhs,
                    &left_ty,
                    (&source_element, count),
                    initializer,
                    budget,
                )?;
                // Shift counts retain their scalar type: narrowing 256 to an
                // unsigned-char lane must not silently turn it into a zero shift.
                let shift = matches!(
                    binary.node.operator.node,
                    Binary::ShiftLeft | Binary::ShiftRight
                );
                let right_element = if shift { &right_ty } else { &source_element };
                let right = self.vector_constant_operand(
                    &binary.node.rhs,
                    &right_ty,
                    (right_element, count),
                    initializer,
                    budget,
                )?;
                let comparison = matches!(
                    binary.node.operator.node,
                    Binary::Equals
                        | Binary::NotEquals
                        | Binary::Less
                        | Binary::LessOrEqual
                        | Binary::Greater
                        | Binary::GreaterOrEqual
                );
                for (left, right) in left.iter_mut().zip(right) {
                    let value = if let (ArithmeticValue::Integer(a), ArithmeticValue::Integer(b)) =
                        (*left, right)
                    {
                        ArithmeticValue::Integer(self.vector_integer_binary(
                            &binary.node.operator.node,
                            a,
                            b,
                            offset,
                        )?)
                    } else {
                        self.arithmetic_binary(&binary.node.operator.node, *left, right, offset)?
                    };
                    // A comparison produces all-one or all-zero lanes, rather
                    // than the scalar C comparison's one or zero.
                    *left = self.convert_arithmetic(
                        if comparison {
                            ArithmeticValue::Integer(IntegerValue::int(if value.truth() {
                                -1
                            } else {
                                0
                            }))
                        } else {
                            value
                        },
                        &element,
                        offset,
                    )?;
                }
                left
            }
            ast::Expression::ConvertVector(conversion) => {
                let source =
                    self.vector_constant(&conversion.node.expression, initializer, budget)?;
                let (source_element, _) = self.vector_constant_shape(&source.ty, offset)?;
                if initializer
                    && (!self.gnu_vector_profile()
                        || !self.compatible(&source_element, &element)?)
                {
                    return Err(Error::new(
                        offset,
                        "numeric vector conversion is not a static initializer in this compiler profile",
                    ));
                }
                source
                    .lanes
                    .into_iter()
                    .map(|lane| self.convert_arithmetic(lane, &element, offset))
                    .collect::<Result<_, _>>()?
            }
            _ => return Err(unsupported()),
        };
        Ok(VectorValue { ty, lanes })
    }

    fn vector_constant_shape(&self, ty: &Type, offset: usize) -> Result<(Type, usize), Error> {
        let ty = self.unit.atomic_value(ty)?.unwrap_or(ty);
        let TypeKind::Vector { element, lanes, .. } = &self.unit.resolve(ty)?.kind else {
            return Err(Error::new(
                offset,
                "vector constant evaluation requires a fixed vector type",
            ));
        };
        if *lanes == 0 || *lanes > 16 || self.unit.layout(ty)?.size_bytes() > 16 {
            return Err(Error::new(
                offset,
                "vector constant evaluation exceeds the 16-byte vector limit",
            ));
        }
        Ok(((**element).clone(), *lanes as usize))
    }

    fn vector_constant_operand(
        &mut self,
        expression: &Node<ast::Expression>,
        ty: &Type,
        shape: (&Type, usize),
        initializer: bool,
        budget: &mut Budget,
    ) -> Result<Vec<ArithmeticValue>, Error> {
        if matches!(ty.kind, TypeKind::Vector { .. }) {
            return Ok(self.vector_constant(expression, initializer, budget)?.lanes);
        }
        let (element, lanes) = shape;
        budget.charge(lanes, expression.span.start)?;
        if initializer {
            self.check_static_arithmetic(expression)?;
        }
        let value = self.eval_arithmetic(expression)?;
        let value = self.convert_arithmetic(value, element, expression.span.start)?;
        Ok(vec![value; lanes])
    }

    /// Vector integer operators keep the lane width; scalar integer promotions
    /// would incorrectly change narrow shifts and signed overflow decisions.
    fn vector_integer_binary(
        &self,
        operator: &ast::BinaryOperator,
        left: IntegerValue,
        right: IntegerValue,
        offset: usize,
    ) -> Result<IntegerValue, Error> {
        use ast::BinaryOperator as Op;
        let result = match operator {
            Op::Plus | Op::Minus | Op::Multiply if left.signed => {
                return signed_result(
                    match operator {
                        Op::Plus => left.signed_value().checked_add(right.signed_value()),
                        Op::Minus => left.signed_value().checked_sub(right.signed_value()),
                        _ => left.signed_value().checked_mul(right.signed_value()),
                    },
                    left,
                    offset,
                );
            }
            Op::Plus => left.value.wrapping_add(right.value),
            Op::Minus => left.value.wrapping_sub(right.value),
            Op::Multiply => left.value.wrapping_mul(right.value),
            Op::BitwiseAnd => left.value & right.value,
            Op::BitwiseOr => left.value | right.value,
            Op::BitwiseXor => left.value ^ right.value,
            Op::ShiftLeft | Op::ShiftRight => {
                let count = right
                    .as_u64()
                    .map_err(|_| Error::new(offset, "negative vector shift count"))?;
                if count >= u64::from(left.bits) {
                    return Err(Error::new(offset, "vector shift count exceeds lane width"));
                }
                if *operator == Op::ShiftRight {
                    if left.signed {
                        (left.signed_value() >> count) as u128
                    } else {
                        left.value >> count
                    }
                } else {
                    if left.signed
                        && (left.signed_value() < 0
                            || left.value > ((IntegerValue::mask(left.bits) >> 1) >> count))
                    {
                        return Err(Error::new(
                            offset,
                            "signed vector left shift overflows or has a negative operand",
                        ));
                    }
                    left.value << count
                }
            }
            Op::Divide | Op::Modulo if left.signed => {
                if right.value == 0 {
                    return Err(Error::new(offset, "division by zero in vector constant"));
                }
                let value = if *operator == Op::Divide {
                    left.signed_value().checked_div(right.signed_value())
                } else {
                    // Remainder has the same signed-overflow restriction.
                    left.signed_value()
                        .checked_div(right.signed_value())
                        .and_then(|_| left.signed_value().checked_rem(right.signed_value()))
                };
                if right.signed_value() == -1 && left.value == (1_u128 << (left.bits - 1)) {
                    return Err(Error::new(offset, "signed vector division overflows"));
                }
                return signed_result(value, left, offset);
            }
            Op::Divide | Op::Modulo => {
                if right.value == 0 {
                    return Err(Error::new(offset, "division by zero in vector constant"));
                }
                if *operator == Op::Divide {
                    left.value / right.value
                } else {
                    left.value % right.value
                }
            }
            _ => return self.binary(operator, left, right, offset),
        };
        Ok(IntegerValue::new(result, left.bits, left.signed, left.rank))
    }
}

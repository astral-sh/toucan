//! Target floating-point constant evaluation, without host floating-point arithmetic.

use lang_c::{ast, span::Node};
use rustc_apfloat::ieee::{Double, Quad, Single, X87DoubleExtended};
use rustc_apfloat::{Float, FloatConvert, Round, Status, StatusAnd};
use toucan_target::Target;

use crate::analyze::Analyzer;
use crate::integer::{convert, promote, signed_result};
use crate::{
    ArithmeticConstant, Error, FloatKind, FloatingFormat, FloatingValue, IntegerValue, Type,
    TypeKind,
};

/// Binary128 exactly stores every finite value in the supported C formats. Each
/// conversion and operation still rounds in its own target format, not binary128.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ArithmeticValue {
    Integer(IntegerValue),
    Floating { value: Quad, kind: FloatKind },
}

impl ArithmeticValue {
    pub(crate) fn into_constant(
        self,
        target: Target,
        offset: usize,
    ) -> Result<ArithmeticConstant, Error> {
        Ok(match self {
            Self::Integer(value) => ArithmeticConstant::Integer(value),
            Self::Floating { value, kind } => {
                let format = Format::for_type(kind, target, offset)?;
                let bits = match format {
                    Format::Binary32 => encode_float::<Single>(value),
                    Format::Binary64 => encode_float::<Double>(value),
                    Format::X87 => encode_float::<X87DoubleExtended>(value),
                    Format::Binary128 => encode_float::<Quad>(value),
                };
                let format = match format {
                    Format::Binary32 => FloatingFormat::Binary32,
                    Format::Binary64 => FloatingFormat::Binary64,
                    Format::X87 => FloatingFormat::X87,
                    Format::Binary128 => FloatingFormat::Binary128,
                };
                ArithmeticConstant::Floating(FloatingValue { kind, format, bits })
            }
        })
    }

    pub(crate) fn truth(self) -> bool {
        match self {
            Self::Integer(value) => value.truth(),
            Self::Floating { value, .. } => !value.is_zero(),
        }
    }

    pub(crate) fn integer(self, offset: usize) -> Result<IntegerValue, Error> {
        match self {
            Self::Integer(value) => Ok(value),
            Self::Floating { .. } => Err(Error::new(offset, "expected an integer constant")),
        }
    }
}

#[derive(Clone, Copy)]
enum Format {
    Binary32,
    Binary64,
    X87,
    Binary128,
}

impl Format {
    fn for_type(kind: FloatKind, target: Target, offset: usize) -> Result<Self, Error> {
        Ok(match kind {
            FloatKind::Float => Self::Binary32,
            FloatKind::Double => Self::Binary64,
            FloatKind::LongDouble => match target {
                Target::X86_64UnknownLinuxGnu | Target::X86_64AppleDarwin => Self::X87,
                Target::Aarch64UnknownLinuxGnu => Self::Binary128,
                Target::Aarch64AppleDarwin | Target::X86_64PcWindowsMsvc => Self::Binary64,
            },
            FloatKind::Extended { .. } => {
                return Err(Error::new(
                    offset,
                    "extended floating-point evaluation is unsupported",
                ));
            }
        })
    }
}

macro_rules! dispatch {
    ($format:expr, $function:ident($($argument:expr),* $(,)?)) => {
        match $format {
            Format::Binary32 => $function::<Single>($($argument),*),
            Format::Binary64 => $function::<Double>($($argument),*),
            Format::X87 => $function::<X87DoubleExtended>($($argument),*),
            Format::Binary128 => $function::<Quad>($($argument),*),
        }
    };
}

impl Analyzer {
    pub(crate) fn floating_literal(
        &self,
        literal: &ast::Float,
        offset: usize,
    ) -> Result<ArithmeticValue, Error> {
        if literal.suffix.imaginary {
            return Err(Error::new(offset, "complex constants are unsupported"));
        }
        if literal.number.len() > 4096 {
            return Err(Error::new(
                offset,
                "floating literal exceeds the 4096-byte limit",
            ));
        }
        let kind = match literal.suffix.format {
            ast::FloatFormat::Float => FloatKind::Float,
            ast::FloatFormat::Double => FloatKind::Double,
            ast::FloatFormat::LongDouble => FloatKind::LongDouble,
            _ => {
                return Err(Error::new(
                    offset,
                    "extended floating constants are unsupported",
                ));
            }
        };
        let format = Format::for_type(kind, self.unit.target, offset)?;
        // lang-c stores hexadecimal digits after the `0x` prefix.
        let number = if literal.base == ast::FloatBase::Hexadecimal {
            std::borrow::Cow::Owned(format!("0x{}", literal.number))
        } else {
            std::borrow::Cow::Borrowed(literal.number.as_ref())
        };
        let value = dispatch!(format, parse_literal(&number, offset))?;
        Ok(ArithmeticValue::Floating { value, kind })
    }

    /// Evaluates the arithmetic constant expressions permitted in static
    /// initializers, including floating operands that are not C11 integer ICEs.
    pub(crate) fn eval_arithmetic(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ArithmeticValue, Error> {
        self.enter_expression(expression.span.start)?;
        let result = self.eval_arithmetic_inner(expression);
        self.leave_expression();
        result
    }

    fn eval_arithmetic_inner(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<ArithmeticValue, Error> {
        use ast::BinaryOperator as Binary;
        use ast::UnaryOperator as Unary;
        let offset = expression.span.start;
        match &expression.node {
            ast::Expression::Call(call)
                if matches!(&call.node.callee.node, ast::Expression::Identifier(identifier)
                if matches!(identifier.node.name.as_str(), "__builtin_inf" | "__builtin_inff" | "__builtin_infl"
                    | "__builtin_huge_val" | "__builtin_huge_valf" | "__builtin_huge_vall"
                    | "__builtin_nan" | "__builtin_nanf" | "__builtin_nanl"
                    | "__builtin_nans" | "__builtin_nansf" | "__builtin_nansl")) =>
            {
                Err(Error::new(
                    offset,
                    "non-finite floating builtin constants are unsupported",
                ))
            }
            ast::Expression::Constant(constant) => {
                if let ast::Constant::Float(literal) = &constant.node {
                    self.floating_literal(literal, offset)
                } else {
                    self.eval(expression).map(ArithmeticValue::Integer)
                }
            }
            ast::Expression::Cast(cast) => {
                let ty = self.type_name(&cast.node.type_name.node)?;
                let value = self.eval_arithmetic(&cast.node.expression)?;
                self.convert_arithmetic(value, &ty, offset)
            }
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                self.eval_arithmetic(selected)
            }
            ast::Expression::CompoundLiteral(literal) => {
                let ty = self.type_name(&literal.node.type_name.node)?;
                let [item] = literal.node.initializer_list.as_slice() else {
                    return Err(Error::new(
                        offset,
                        "scalar constant initializer requires one value",
                    ));
                };
                if !item.node.designation.is_empty() {
                    return Err(Error::new(
                        offset,
                        "scalar constant initializer cannot have a designator",
                    ));
                }
                let mut initializer = &item.node.initializer;
                for _ in 0..128 {
                    match &initializer.node {
                        ast::Initializer::Expression(expression) => {
                            let value = self.eval_arithmetic(expression)?;
                            return self.convert_arithmetic(value, &ty, offset);
                        }
                        ast::Initializer::List(items) => {
                            let [item] = items.as_slice() else {
                                return Err(Error::new(
                                    offset,
                                    "scalar constant initializer requires one value",
                                ));
                            };
                            if !item.node.designation.is_empty() {
                                return Err(Error::new(
                                    offset,
                                    "scalar constant initializer cannot have a designator",
                                ));
                            }
                            initializer = &item.node.initializer;
                        }
                    }
                }
                Err(Error::new(
                    offset,
                    "constant initializer nesting exceeds the 128-level limit",
                ))
            }
            ast::Expression::UnaryOperator(unary) => {
                let value = self.eval_arithmetic(&unary.node.operand)?;
                if unary.node.operator.node == Unary::Negate {
                    return Ok(ArithmeticValue::Integer(IntegerValue::int(i128::from(
                        !value.truth(),
                    ))));
                }
                match value {
                    ArithmeticValue::Floating { value, kind } => {
                        let value = match unary.node.operator.node {
                            Unary::Plus => value,
                            Unary::Minus => -value,
                            _ => {
                                return Err(Error::new(
                                    offset,
                                    "operator requires integer operands",
                                ));
                            }
                        };
                        Ok(ArithmeticValue::Floating { value, kind })
                    }
                    ArithmeticValue::Integer(value) => {
                        let value = promote(value);
                        let result = match unary.node.operator.node {
                            Unary::Plus => value,
                            Unary::Minus if value.signed => {
                                signed_result(value.signed_value().checked_neg(), value, offset)?
                            }
                            Unary::Minus => IntegerValue::new(
                                value.value.wrapping_neg(),
                                value.bits,
                                false,
                                value.rank,
                            ),
                            Unary::Complement => IntegerValue::new(
                                !value.value,
                                value.bits,
                                value.signed,
                                value.rank,
                            ),
                            _ => {
                                return Err(Error::new(
                                    offset,
                                    "operator is not permitted in an arithmetic constant expression",
                                ));
                            }
                        };
                        Ok(ArithmeticValue::Integer(result))
                    }
                }
            }
            ast::Expression::BinaryOperator(binary) => {
                let left = self.eval_arithmetic(&binary.node.lhs)?;
                let operator = &binary.node.operator.node;
                if matches!(operator, Binary::LogicalAnd | Binary::LogicalOr) {
                    let right = self.expression_type(&binary.node.rhs)?;
                    self.require_scalar(&right, offset)?;
                    if (*operator == Binary::LogicalAnd && !left.truth())
                        || (*operator == Binary::LogicalOr && left.truth())
                    {
                        return Ok(ArithmeticValue::Integer(IntegerValue::int(i128::from(
                            left.truth(),
                        ))));
                    }
                    let right = self.eval_arithmetic(&binary.node.rhs)?;
                    return Ok(ArithmeticValue::Integer(IntegerValue::int(i128::from(
                        right.truth(),
                    ))));
                }
                let right = self.eval_arithmetic(&binary.node.rhs)?;
                self.arithmetic_binary(operator, left, right, offset)
            }
            ast::Expression::Conditional(conditional) => {
                // The unselected operand still determines the common type and
                // must satisfy C expression constraints.
                let ty = self.expression_type(expression)?;
                let condition = self.eval_arithmetic(&conditional.node.condition)?;
                let selected = if condition.truth() {
                    &conditional.node.then_expression
                } else {
                    &conditional.node.else_expression
                };
                let value = self.eval_arithmetic(selected)?;
                self.convert_arithmetic(value, &ty, offset)
            }
            _ => self.eval(expression).map(ArithmeticValue::Integer),
        }
    }

    /// Applies the arithmetic conversion for a cast or scalar initializer.
    pub(crate) fn convert_arithmetic(
        &self,
        value: ArithmeticValue,
        destination: &Type,
        offset: usize,
    ) -> Result<ArithmeticValue, Error> {
        let ty = self.unit.resolve(destination)?;
        if matches!(ty.kind, TypeKind::Bool) {
            return Ok(ArithmeticValue::Integer(IntegerValue::new(
                u128::from(value.truth()),
                8,
                false,
                0,
            )));
        }
        if let TypeKind::Float(kind) = ty.kind {
            let format = Format::for_type(kind, self.unit.target, offset)?;
            return Ok(ArithmeticValue::Floating {
                value: dispatch!(format, convert_float(value, offset))?,
                kind,
            });
        }
        let destination = self.integer_type(destination, offset)?;
        let result = match value {
            ArithmeticValue::Integer(value) => convert(value, destination),
            ArithmeticValue::Floating { value, .. } => {
                // C floating-to-integer conversion truncates toward zero. It
                // does not wrap out-of-range values as integer casts do.
                let converted = if destination.signed {
                    value
                        .to_i128(destination.bits.into())
                        .map(|value| value as u128)
                } else {
                    value.to_u128(destination.bits.into())
                };
                if converted.status.contains(Status::INVALID_OP) {
                    return Err(Error::new(
                        offset,
                        "floating constant is outside the destination integer range",
                    ));
                }
                IntegerValue::new(
                    converted.value,
                    destination.bits,
                    destination.signed,
                    destination.rank,
                )
            }
        };
        Ok(ArithmeticValue::Integer(result))
    }

    fn arithmetic_binary(
        &self,
        operator: &ast::BinaryOperator,
        left: ArithmeticValue,
        right: ArithmeticValue,
        offset: usize,
    ) -> Result<ArithmeticValue, Error> {
        use ast::BinaryOperator as Op;
        if let (ArithmeticValue::Integer(left), ArithmeticValue::Integer(right)) = (left, right) {
            return self
                .binary(operator, left, right, offset)
                .map(ArithmeticValue::Integer);
        }
        let kind = [FloatKind::LongDouble, FloatKind::Double, FloatKind::Float]
            .into_iter()
            .find(|kind| matches!(left, ArithmeticValue::Floating { kind: actual, .. } if actual == *kind)
                || matches!(right, ArithmeticValue::Floating { kind: actual, .. } if actual == *kind))
            .expect("a floating operand is present");
        let format = Format::for_type(kind, self.unit.target, offset)?;
        let left = dispatch!(format, convert_float(left, offset))?;
        let right = dispatch!(format, convert_float(right, offset))?;
        let comparison = match operator {
            Op::Less => Some(left < right),
            Op::LessOrEqual => Some(left <= right),
            Op::Greater => Some(left > right),
            Op::GreaterOrEqual => Some(left >= right),
            Op::Equals => Some(left == right),
            Op::NotEquals => Some(left != right),
            _ => None,
        };
        if let Some(value) = comparison {
            return Ok(ArithmeticValue::Integer(IntegerValue::int(i128::from(
                value,
            ))));
        }
        Ok(ArithmeticValue::Floating {
            value: dispatch!(format, binary_float(operator, left, right, offset))?,
            kind,
        })
    }
}

fn parse_literal<F>(source: &str, offset: usize) -> Result<Quad, Error>
where
    F: Float + FloatConvert<Quad>,
{
    let parsed = F::from_str_r(source, Round::NearestTiesToEven)
        .map_err(|_| Error::new(offset, "invalid floating constant"))?;
    let value = finite_result(parsed, offset)?;
    Ok(value.convert(&mut false).value)
}

fn encode_float<F: Float>(value: Quad) -> u128
where
    Quad: FloatConvert<F>,
{
    let value: F = value.convert(&mut false).value;
    value.to_bits()
}

fn convert_float<F>(value: ArithmeticValue, offset: usize) -> Result<Quad, Error>
where
    F: Float + FloatConvert<Quad>,
    Quad: FloatConvert<F>,
{
    let converted = match value {
        ArithmeticValue::Integer(value) if value.signed => F::from_i128(value.signed_value()),
        ArithmeticValue::Integer(value) => F::from_u128(value.value),
        ArithmeticValue::Floating { value, .. } => value.convert(&mut false),
    };
    let value = finite_result(converted, offset)?;
    Ok(value.convert(&mut false).value)
}

fn binary_float<F>(
    operator: &ast::BinaryOperator,
    left: Quad,
    right: Quad,
    offset: usize,
) -> Result<Quad, Error>
where
    F: Float + FloatConvert<Quad>,
    Quad: FloatConvert<F>,
{
    let left: F = left.convert(&mut false).value;
    let right: F = right.convert(&mut false).value;
    let value = match operator {
        ast::BinaryOperator::Plus => left.add_r(right, Round::NearestTiesToEven),
        ast::BinaryOperator::Minus => left.sub_r(right, Round::NearestTiesToEven),
        ast::BinaryOperator::Multiply => left.mul_r(right, Round::NearestTiesToEven),
        ast::BinaryOperator::Divide => left.div_r(right, Round::NearestTiesToEven),
        _ => return Err(Error::new(offset, "operator requires integer operands")),
    };
    let value = finite_result(value, offset)?;
    Ok(value.convert(&mut false).value)
}

/// Inexact rounding and gradual underflow are defined. Reject operations whose
/// mathematical result cannot be represented as a finite target value.
fn finite_result<F: Float>(result: StatusAnd<F>, offset: usize) -> Result<F, Error> {
    if result.status.contains(Status::DIV_BY_ZERO) {
        return Err(Error::new(offset, "floating-point division by zero"));
    }
    if result
        .status
        .intersects(Status::INVALID_OP | Status::OVERFLOW)
        || !result.value.is_finite()
    {
        return Err(Error::new(
            offset,
            "floating-point constant overflow or invalid operation",
        ));
    }
    Ok(result.value)
}

#[cfg(test)]
#[path = "floating_tests.rs"]
mod tests;

//! Target floating-point constant evaluation, without host floating-point arithmetic.

use lang_c::{ast, span::Node};
use rustc_apfloat::ieee::{BFloat, Double, Half, Quad, Single, X87DoubleExtended};
use rustc_apfloat::{Float, FloatConvert, Round, Status, StatusAnd};
use toucan_target::Target;

use crate::analyze::Analyzer;
use crate::integer::{convert, promote, signed_result};
use crate::{
    ArithmeticConstant, ComplexValue, Error, FloatKind, FloatingFormat, FloatingValue,
    IntegerValue, Type, TypeKind,
};

/// Binary128 exactly stores every finite value in the supported C formats. Each
/// conversion and operation still rounds in its own target format, not binary128.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ArithmeticValue {
    Integer(IntegerValue),
    Floating {
        value: Quad,
        kind: FloatKind,
        // Quad conversions quiet signaling NaNs. Keep this bit separately while
        // storing their payload in a quiet NaN; representation changes are not
        // C arithmetic operations.
        signaling: bool,
    },
    Complex {
        real: Quad,
        imaginary: Quad,
        kind: FloatKind,
        signaling: [bool; 2],
    },
}

impl ArithmeticValue {
    pub(crate) fn into_constant(
        self,
        target: Target,
        offset: usize,
    ) -> Result<ArithmeticConstant, Error> {
        Ok(match self {
            Self::Integer(value) => ArithmeticConstant::Integer(value),
            Self::Floating {
                value,
                kind,
                signaling,
            } => ArithmeticConstant::Floating(floating_constant(
                value, kind, signaling, target, offset,
            )?),
            Self::Complex {
                real,
                imaginary,
                kind,
                signaling,
            } => {
                let real = floating_constant(real, kind, signaling[0], target, offset)?;
                let imaginary = floating_constant(imaginary, kind, signaling[1], target, offset)?;
                ArithmeticConstant::Complex(ComplexValue {
                    kind,
                    format: real.format,
                    real_bits: real.bits,
                    imaginary_bits: imaginary.bits,
                })
            }
        })
    }

    pub(crate) fn truth(self) -> bool {
        match self {
            Self::Integer(value) => value.truth(),
            Self::Floating { value, .. } => !value.is_zero(),
            Self::Complex {
                real, imaginary, ..
            } => !real.is_zero() || !imaginary.is_zero(),
        }
    }

    pub(crate) fn integer(self, offset: usize) -> Result<IntegerValue, Error> {
        match self {
            Self::Integer(value) => Ok(value),
            Self::Floating { .. } | Self::Complex { .. } => {
                Err(Error::new(offset, "expected an integer constant"))
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Binary16,
    BFloat16,
    Binary32,
    Binary64,
    X87,
    Binary128,
}

impl Format {
    fn for_type(kind: FloatKind, target: Target, offset: usize) -> Result<Self, Error> {
        Ok(match kind {
            FloatKind::FLOAT16 => Self::Binary16,
            FloatKind::BFloat16 => Self::BFloat16,
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
            Format::Binary16 => $function::<Half>($($argument),*),
            Format::BFloat16 => $function::<BFloat>($($argument),*),
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
        if literal.number.len() > 4096 {
            return Err(Error::new(
                offset,
                "floating literal exceeds the 4096-byte limit",
            ));
        }
        let kind = crate::narrow_float::literal_kind(&literal.suffix.format, offset)?;
        let format = Format::for_type(kind, self.unit.target, offset)?;
        // lang-c stores hexadecimal digits after the `0x` prefix.
        let number = if literal.base == ast::FloatBase::Hexadecimal {
            std::borrow::Cow::Owned(format!("0x{}", literal.number))
        } else {
            std::borrow::Cow::Borrowed(literal.number.as_ref())
        };
        let value = dispatch!(
            format,
            parse_literal(&number, offset, kind.is_narrow() && self.gnu_sync_profile())
        )?;
        Ok(if literal.suffix.imaginary {
            self.require_complex_kind(kind, offset)?;
            ArithmeticValue::Complex {
                real: Quad::ZERO,
                imaginary: value,
                kind,
                signaling: [false; 2],
            }
        } else {
            ArithmeticValue::Floating {
                value,
                kind,
                signaling: false,
            }
        })
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
            ast::Expression::Call(call) => {
                let name = self.builtin_name(call);
                if let Some(kind) = name.and_then(|name| self.infinity_builtin_kind(name)) {
                    self.builtin_call_type(call)?;
                    Ok(ArithmeticValue::Floating {
                        value: Quad::INFINITY,
                        kind,
                        signaling: false,
                    })
                } else if let Some((kind, signaling)) = name.and_then(|name| self.nan_builtin(name))
                {
                    self.builtin_call_type(call)?;
                    let payload = self.nan_payload(&call.node.arguments[0])?;
                    let format = Format::for_type(kind, self.unit.target, offset)?;
                    Ok(ArithmeticValue::Floating {
                        value: dispatch!(format, nan_storage(payload, signaling)),
                        kind,
                        signaling,
                    })
                } else if name == Some("__builtin_complex") {
                    let ty = self.complex_constructor_type(call)?;
                    let TypeKind::Complex(kind) = ty.kind else {
                        unreachable!("complex constructor type")
                    };
                    let real = self.eval_arithmetic(&call.node.arguments[0])?;
                    let imaginary = self.eval_arithmetic(&call.node.arguments[1])?;
                    let (
                        ArithmeticValue::Floating {
                            value: real,
                            signaling: real_signaling,
                            ..
                        },
                        ArithmeticValue::Floating {
                            value: imaginary,
                            signaling: imaginary_signaling,
                            ..
                        },
                    ) = (real, imaginary)
                    else {
                        return Err(Error::new(
                            offset,
                            "__builtin_complex constant arguments must be real floating values",
                        ));
                    };
                    Ok(ArithmeticValue::Complex {
                        real,
                        imaginary,
                        kind,
                        signaling: [real_signaling, imaginary_signaling],
                    })
                } else {
                    self.eval(expression).map(ArithmeticValue::Integer)
                }
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
            ast::Expression::Choose(selection) => {
                let selected = self.choose_expression(selection)?;
                self.eval_arithmetic(selected)
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
                    ArithmeticValue::Complex {
                        real,
                        imaginary,
                        kind,
                        signaling,
                    } => {
                        let (real, imaginary) = match unary.node.operator.node {
                            Unary::Plus => (real, imaginary),
                            Unary::Minus => (-real, -imaginary),
                            _ => {
                                return Err(Error::new(
                                    offset,
                                    "operator requires integer operands",
                                ));
                            }
                        };
                        Ok(ArithmeticValue::Complex {
                            real,
                            imaginary,
                            kind,
                            signaling,
                        })
                    }
                    ArithmeticValue::Floating {
                        value,
                        kind,
                        signaling,
                    } => {
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
                        Ok(ArithmeticValue::Floating {
                            value,
                            kind,
                            signaling,
                        })
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
                if let TypeKind::Float(kind) = self.unit.resolve(&ty)?.kind {
                    self.require_narrow_constant_precision(kind, offset)?;
                }
                self.convert_arithmetic(value, &ty, offset)
            }
            _ => self.eval(expression).map(ArithmeticValue::Integer),
        }
    }

    /// Recognize the literal payload forms both GCC and Clang can fold.
    fn nan_payload(&mut self, mut expression: &Node<ast::Expression>) -> Result<u128, Error> {
        let offset = expression.span.start;
        for _ in 0..128 {
            match &expression.node {
                ast::Expression::Cast(cast) => {
                    let ty = self.type_name(&cast.node.type_name.node)?;
                    let TypeKind::Pointer(pointee) = &self.unit.resolve(&ty)?.kind else {
                        break;
                    };
                    if !matches!(
                        self.unit.resolve(pointee)?.kind,
                        TypeKind::Void | TypeKind::Integer(crate::IntegerKind::Char)
                    ) {
                        break;
                    }
                    expression = &cast.node.expression;
                }
                ast::Expression::StringLiteral(literal) => {
                    let decoded = self.decode_string_literal(literal, offset)?;
                    let units = &decoded.code_units[..decoded.code_units.len() - 1];
                    if units.contains(&0) {
                        return Err(Error::new(
                            offset,
                            "constant NaN payloads with embedded NULs are unsupported",
                        ));
                    }
                    if units.len() > 4096 {
                        return Err(Error::new(
                            offset,
                            "NaN payload exceeds the 4096-byte limit",
                        ));
                    }
                    return parse_nan_payload(units, offset);
                }
                _ => break,
            }
        }
        Err(Error::new(
            offset,
            "constant NaN payload requires a string literal or a char/void pointer cast around one",
        ))
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
        if let TypeKind::Complex(kind) = ty.kind {
            self.require_complex_kind(kind, offset)?;
            let (real, imaginary) = match value {
                ArithmeticValue::Complex {
                    real,
                    imaginary,
                    kind,
                    signaling,
                } => (
                    ArithmeticValue::Floating {
                        value: real,
                        kind,
                        signaling: signaling[0],
                    },
                    ArithmeticValue::Floating {
                        value: imaginary,
                        kind,
                        signaling: signaling[1],
                    },
                ),
                value => (
                    value,
                    ArithmeticValue::Floating {
                        value: Quad::ZERO,
                        kind,
                        signaling: false,
                    },
                ),
            };
            let real_ty = Type::new(TypeKind::Float(kind));
            let real = self.convert_arithmetic(real, &real_ty, offset)?;
            let imaginary = self.convert_arithmetic(imaginary, &real_ty, offset)?;
            let (
                ArithmeticValue::Floating {
                    value: real,
                    signaling: real_signaling,
                    ..
                },
                ArithmeticValue::Floating {
                    value: imaginary,
                    signaling: imaginary_signaling,
                    ..
                },
            ) = (real, imaginary)
            else {
                unreachable!("real component conversion")
            };
            return Ok(ArithmeticValue::Complex {
                real,
                imaginary,
                kind,
                signaling: [real_signaling, imaginary_signaling],
            });
        }
        let value = match value {
            ArithmeticValue::Complex {
                real,
                kind,
                signaling,
                ..
            } => ArithmeticValue::Floating {
                value: real,
                kind,
                signaling: signaling[0],
            },
            value => value,
        };
        if let TypeKind::Float(kind) = ty.kind {
            let format = Format::for_type(kind, self.unit.target, offset)?;
            let signaling = if let ArithmeticValue::Floating {
                kind: source,
                signaling,
                ..
            } = value
            {
                signaling && Format::for_type(source, self.unit.target, offset)? == format
            } else {
                false
            };
            return Ok(ArithmeticValue::Floating {
                value: dispatch!(format, convert_float(value, offset))?,
                kind,
                signaling,
            });
        }
        let destination = self.integer_type(destination, offset)?;
        let result = match value {
            ArithmeticValue::Integer(value) => convert(value, destination),
            ArithmeticValue::Complex { .. } => unreachable!("complex real component extracted"),
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

    fn require_narrow_constant_precision(
        &self,
        kind: FloatKind,
        offset: usize,
    ) -> Result<(), Error> {
        if kind.is_narrow() && self.gnu_sync_profile() {
            return Err(Error::new(
                offset,
                "GNU half/bfloat arithmetic constant evaluation with excess precision is unsupported",
            ));
        }
        Ok(())
    }

    fn complex_binary(
        &self,
        operator: &ast::BinaryOperator,
        left: ArithmeticValue,
        right: ArithmeticValue,
        kind: FloatKind,
        format: Format,
        offset: usize,
    ) -> Result<ArithmeticValue, Error> {
        let real_domains = [
            !matches!(left, ArithmeticValue::Complex { .. }),
            !matches!(right, ArithmeticValue::Complex { .. }),
        ];
        let ty = Type::new(TypeKind::Complex(kind));
        let left = self.convert_arithmetic(left, &ty, offset)?;
        let right = self.convert_arithmetic(right, &ty, offset)?;
        let (
            ArithmeticValue::Complex {
                real: a,
                imaginary: b,
                signaling: left_signaling,
                ..
            },
            ArithmeticValue::Complex {
                real: c,
                imaginary: d,
                signaling: right_signaling,
                ..
            },
        ) = (left, right)
        else {
            unreachable!("converted complex operands")
        };
        if matches!(
            operator,
            ast::BinaryOperator::Equals | ast::BinaryOperator::NotEquals
        ) {
            let equal = a == c && b == d;
            return Ok(ArithmeticValue::Integer(IntegerValue::int(i128::from(
                if *operator == ast::BinaryOperator::Equals {
                    equal
                } else {
                    !equal
                },
            ))));
        }
        if left_signaling
            .into_iter()
            .chain(right_signaling)
            .any(|value| value)
        {
            return Err(Error::new(
                offset,
                "arithmetic on signaling NaN constants is unsupported",
            ));
        }
        use crate::complex::binary as complex_float;
        let [real, imaginary] = dispatch!(
            format,
            complex_float(
                operator,
                [a, b, c, d],
                real_domains,
                self.gnu_sync_profile(),
                offset
            )
        )?;
        Ok(ArithmeticValue::Complex {
            real,
            imaginary,
            kind,
            signaling: [false; 2],
        })
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
        let float_kind = |value| {
            if let ArithmeticValue::Floating { kind, .. } | ArithmeticValue::Complex { kind, .. } =
                value
            {
                Some(kind)
            } else {
                None
            }
        };
        let kind = crate::narrow_float::common_kind(float_kind(left), float_kind(right), offset)?
            .expect("a floating operand is present");
        self.require_narrow_constant_precision(kind, offset)?;
        let format = Format::for_type(kind, self.unit.target, offset)?;
        if matches!(left, ArithmeticValue::Complex { .. })
            || matches!(right, ArithmeticValue::Complex { .. })
        {
            return self.complex_binary(operator, left, right, kind, format, offset);
        }
        let signaling = matches!(
            left,
            ArithmeticValue::Floating {
                signaling: true,
                ..
            }
        ) || matches!(
            right,
            ArithmeticValue::Floating {
                signaling: true,
                ..
            }
        );
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
        if signaling {
            return Err(Error::new(
                offset,
                "arithmetic on signaling NaN constants is unsupported",
            ));
        }
        Ok(ArithmeticValue::Floating {
            value: dispatch!(format, binary_float(operator, left, right, offset))?,
            kind,
            signaling: false,
        })
    }
}

fn floating_constant(
    value: Quad,
    kind: FloatKind,
    signaling: bool,
    target: Target,
    offset: usize,
) -> Result<FloatingValue, Error> {
    let format = Format::for_type(kind, target, offset)?;
    let bits = match format {
        Format::Binary16 => encode_float::<Half>(value, signaling),
        Format::BFloat16 => encode_float::<BFloat>(value, signaling),
        Format::Binary32 => encode_float::<Single>(value, signaling),
        Format::Binary64 => encode_float::<Double>(value, signaling),
        Format::X87 => encode_float::<X87DoubleExtended>(value, signaling),
        Format::Binary128 => encode_float::<Quad>(value, signaling),
    };
    let format = match format {
        Format::Binary16 => FloatingFormat::Binary16,
        Format::BFloat16 => FloatingFormat::BFloat16,
        Format::Binary32 => FloatingFormat::Binary32,
        Format::Binary64 => FloatingFormat::Binary64,
        Format::X87 => FloatingFormat::X87,
        Format::Binary128 => FloatingFormat::Binary128,
    };
    Ok(FloatingValue { kind, format, bits })
}

/// GNU payload digits are accumulated modulo 2^128; every supported significand
/// is narrower, so truncating during parsing preserves all representable bits.
fn parse_nan_payload(units: &[u32], offset: usize) -> Result<u128, Error> {
    if units.is_empty() {
        return Ok(0);
    }
    let (radix, digits) = if units.starts_with(&[u32::from(b'0'), u32::from(b'x')])
        || units.starts_with(&[u32::from(b'0'), u32::from(b'X')])
    {
        (16, &units[2..])
    } else if units[0] == u32::from(b'0') {
        (8, units)
    } else {
        (10, units)
    };
    let invalid = || {
        Error::new(
            offset,
            "constant NaN payload syntax is unsupported; expected unsigned decimal, octal, or hexadecimal digits",
        )
    };
    if digits.is_empty() {
        return Err(invalid());
    }
    let mut value = 0u128;
    for unit in digits {
        let digit = char::from_u32(*unit)
            .and_then(|c| c.to_digit(radix))
            .ok_or_else(invalid)?;
        value = value
            .wrapping_mul(u128::from(radix))
            .wrapping_add(u128::from(digit));
    }
    Ok(value)
}

fn parse_literal<F>(source: &str, offset: usize, require_exact: bool) -> Result<Quad, Error>
where
    F: Float + FloatConvert<Quad>,
{
    let parsed = F::from_str_r(source, Round::NearestTiesToEven)
        .map_err(|_| Error::new(offset, "invalid floating constant"))?;
    if require_exact && parsed.status.contains(Status::INEXACT) {
        return Err(Error::new(
            offset,
            "GNU half literal evaluation with excess precision is unsupported",
        ));
    }
    let value = checked_result(parsed, offset)?;
    Ok(value.convert(&mut false).value)
}

fn encode_float<F: Float>(value: Quad, signaling: bool) -> u128
where
    Quad: FloatConvert<F>,
{
    let value: F = value.convert(&mut false).value;
    if signaling {
        F::snan(Some(value.to_bits())).copy_sign(value).to_bits()
    } else {
        value.to_bits()
    }
}

/// Select the target default payload before widening its quiet representation.
/// The separate signaling flag restores the original form on encoding.
fn nan_storage<F: Float + FloatConvert<Quad>>(payload: u128, signaling: bool) -> Quad {
    let value = if signaling {
        let signaling = F::snan(Some(payload));
        F::qnan(Some(signaling.to_bits()))
    } else {
        F::qnan(Some(payload))
    };
    value.convert(&mut false).value
}

fn convert_float<F>(value: ArithmeticValue, offset: usize) -> Result<Quad, Error>
where
    F: Float + FloatConvert<Quad>,
    Quad: FloatConvert<F>,
{
    let converted = match value {
        ArithmeticValue::Integer(value) if value.signed => F::from_i128(value.signed_value()),
        ArithmeticValue::Integer(value) => F::from_u128(value.value),
        ArithmeticValue::Floating { value, .. } | ArithmeticValue::Complex { real: value, .. } => {
            value.convert(&mut false)
        }
    };
    let value = checked_result(converted, offset)?;
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
    let value = checked_result(value, offset)?;
    Ok(value.convert(&mut false).value)
}

/// Allow inexact rounding, gradual underflow, and existing infinities. Overflow
/// from finite operands and invalid operations still fail constant evaluation.
fn checked_result<F: Float>(result: StatusAnd<F>, offset: usize) -> Result<F, Error> {
    if result.status.contains(Status::DIV_BY_ZERO) {
        return Err(Error::new(offset, "floating-point division by zero"));
    }
    if result
        .status
        .intersects(Status::INVALID_OP | Status::OVERFLOW)
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

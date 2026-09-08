use lang_c::{ast, span::Node};

use crate::{Error, IntegerKind, IntegerValue, Type, TypeKind, analyze::Analyzer};

impl Analyzer {
    pub(crate) fn eval(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<IntegerValue, Error> {
        self.enter_expression(expression.span.start)?;
        let result = self.eval_inner(expression);
        self.leave_expression();
        result
    }

    fn eval_inner(&mut self, expression: &Node<ast::Expression>) -> Result<IntegerValue, Error> {
        let offset = expression.span.start;
        match &expression.node {
            ast::Expression::Constant(constant) => match &constant.node {
                ast::Constant::Integer(integer) => self.literal(integer, offset),
                ast::Constant::Character(character) => character_value(character, offset),
                ast::Constant::Float(_) => Err(Error::new(
                    offset,
                    "floating-point expression is not an integer constant expression",
                )),
            },
            ast::Expression::Identifier(identifier) => self
                .unit
                .constants
                .get(&identifier.node.name)
                .copied()
                .ok_or_else(|| {
                    Error::new(
                        offset,
                        format!("`{}` is not an integer constant", identifier.node.name),
                    )
                }),
            ast::Expression::Cast(cast) => {
                let ty = self.type_name(&cast.node.type_name.node)?;
                let destination = self.integer_type(&ty, offset)?;
                let value = self.eval(&cast.node.expression)?;
                Ok(if matches!(self.unit.resolve(&ty)?.kind, TypeKind::Bool) {
                    IntegerValue::new(u128::from(value.truth()), 8, false, 0)
                } else {
                    convert(value, destination)
                })
            }
            ast::Expression::UnaryOperator(unary) => {
                let value = promote(self.eval(&unary.node.operand)?);
                match unary.node.operator.node {
                    ast::UnaryOperator::Plus => Ok(value),
                    ast::UnaryOperator::Minus => {
                        if value.signed {
                            signed_result(value.signed_value().checked_neg(), value, offset)
                        } else {
                            Ok(IntegerValue::new(
                                value.value.wrapping_neg(),
                                value.bits,
                                false,
                                value.rank,
                            ))
                        }
                    }
                    ast::UnaryOperator::Complement => Ok(IntegerValue::new(
                        !value.value,
                        value.bits,
                        value.signed,
                        value.rank,
                    )),
                    ast::UnaryOperator::Negate => Ok(IntegerValue::int(i128::from(!value.truth()))),
                    _ => Err(Error::new(
                        offset,
                        "operator is not permitted in an integer constant expression",
                    )),
                }
            }
            ast::Expression::BinaryOperator(binary) => {
                let left = self.eval(&binary.node.lhs)?;
                if matches!(
                    binary.node.operator.node,
                    ast::BinaryOperator::LogicalAnd | ast::BinaryOperator::LogicalOr
                ) {
                    // Short-circuiting suppresses evaluation, not operand validation.
                    let right = self.expression_type(&binary.node.rhs)?;
                    self.require_scalar(&right, offset)?;
                }
                if binary.node.operator.node == ast::BinaryOperator::LogicalAnd && !left.truth() {
                    return Ok(IntegerValue::int(0));
                }
                if binary.node.operator.node == ast::BinaryOperator::LogicalOr && left.truth() {
                    return Ok(IntegerValue::int(1));
                }
                let right = self.eval(&binary.node.rhs)?;
                self.binary(&binary.node.operator.node, left, right, offset)
            }
            ast::Expression::Conditional(conditional) => {
                let left_ty = self.expression_type(&conditional.node.then_expression)?;
                let right_ty = self.expression_type(&conditional.node.else_expression)?;
                let destination = common(
                    self.integer_type(&left_ty, offset)?,
                    self.integer_type(&right_ty, offset)?,
                );
                let selected = if self.eval(&conditional.node.condition)?.truth() {
                    &conditional.node.then_expression
                } else {
                    &conditional.node.else_expression
                };
                Ok(convert(self.eval(selected)?, destination))
            }
            ast::Expression::SizeOfTy(size) => {
                let ty = self.type_name(&size.node.0.node)?;
                self.size_of(&ty, offset)
            }
            ast::Expression::SizeOfVal(size) => {
                let ty = self.expression_type(&size.node.0)?;
                self.size_of(&ty, offset)
            }
            ast::Expression::AlignOf(alignment) => {
                let ty = self.type_name(&alignment.node.0.node)?;
                let layout = self.unit.layout(&ty)?;
                Ok(self.size_value(layout.alignment_bytes()))
            }
            ast::Expression::OffsetOf(expression) => {
                let mut ty = self.type_name(&expression.node.type_name.node)?;
                let (mut offset_bytes, field_type) = self.field_offset(
                    &ty,
                    &expression.node.designator.node.base.node.name,
                    offset,
                )?;
                ty = field_type;
                for member in &expression.node.designator.node.members {
                    match &member.node {
                        ast::OffsetMember::Member(name) => {
                            let (delta, field_type) =
                                self.field_offset(&ty, &name.node.name, offset)?;
                            offset_bytes = offset_bytes
                                .checked_add(delta)
                                .ok_or_else(|| Error::new(offset, "offsetof overflow"))?;
                            ty = field_type;
                        }
                        ast::OffsetMember::Index(index) => {
                            let resolved = self.unit.resolve(&ty)?.clone();
                            let TypeKind::Array { element, .. } = resolved.kind else {
                                return Err(Error::new(offset, "offsetof index requires an array"));
                            };
                            let index = self.eval(index)?.as_u64()?;
                            let size = self.unit.layout(&element)?.size_bytes();
                            offset_bytes = offset_bytes
                                .checked_add(
                                    index
                                        .checked_mul(size)
                                        .ok_or_else(|| Error::new(offset, "offsetof overflow"))?,
                                )
                                .ok_or_else(|| Error::new(offset, "offsetof overflow"))?;
                            ty = *element;
                        }
                        ast::OffsetMember::IndirectMember(_) => {
                            return Err(Error::new(
                                offset,
                                "indirect offsetof designator is unsupported",
                            ));
                        }
                    }
                }
                Ok(self.size_value(offset_bytes))
            }
            _ => Err(Error::new(
                offset,
                "expression is not a supported integer constant expression",
            )),
        }
    }

    fn size_of(&self, ty: &Type, offset: usize) -> Result<IntegerValue, Error> {
        if matches!(
            self.unit.resolve(ty)?.kind,
            TypeKind::Array { length: None, .. } | TypeKind::Void | TypeKind::Function(_)
        ) {
            return Err(Error::new(offset, "sizeof requires a complete object type"));
        }
        Ok(self.size_value(self.unit.layout(ty)?.size_bytes()))
    }

    fn size_value(&self, value: u64) -> IntegerValue {
        let bits = self.unit.target.pointer_width() as u8;
        IntegerValue::new(
            u128::from(value),
            bits,
            false,
            if self.unit.target.long_width() == u64::from(bits) {
                4
            } else {
                5
            },
        )
    }

    fn field_offset(&self, ty: &Type, name: &str, offset: usize) -> Result<(u64, Type), Error> {
        let TypeKind::Record(id) = self.unit.resolve(ty)?.kind else {
            return Err(Error::new(offset, "member access requires a record"));
        };
        let record = &self.unit.records[id];
        let fields = record
            .fields
            .as_ref()
            .ok_or_else(|| Error::new(offset, "member of incomplete record"))?;
        let layout = self.unit.layout(ty)?;
        for (index, field) in fields.iter().enumerate() {
            let field_layout = layout.fields[index].as_ref();
            if field.name.as_deref() == Some(name) {
                if field.bit_width.is_some() {
                    return Err(Error::new(offset, "offsetof cannot address a bitfield"));
                }
                return Ok((
                    field_layout
                        .ok_or_else(|| Error::new(offset, "missing field layout"))?
                        .offset_bits
                        / 8,
                    field.ty.clone(),
                ));
            }
            if field.name.is_none()
                && field.bit_width.is_none()
                && let Ok((nested, ty)) = self.field_offset(&field.ty, name, offset)
            {
                return Ok((
                    field_layout
                        .ok_or_else(|| Error::new(offset, "missing anonymous field layout"))?
                        .offset_bits
                        / 8
                        + nested,
                    ty,
                ));
            }
        }
        Err(Error::new(offset, format!("unknown field `{name}`")))
    }

    pub(crate) fn expression_type(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<Type, Error> {
        self.enter_expression(expression.span.start)?;
        let result = self.expression_type_inner(expression);
        self.leave_expression();
        result
    }

    fn expression_type_inner(&mut self, expression: &Node<ast::Expression>) -> Result<Type, Error> {
        let offset = expression.span.start;
        let integer = match &expression.node {
            ast::Expression::Constant(constant) => match &constant.node {
                ast::Constant::Integer(integer) => self.literal(integer, offset)?,
                ast::Constant::Character(_) => IntegerValue::int(0),
                ast::Constant::Float(float) => {
                    return Ok(Type::new(TypeKind::Float(match float.suffix.format {
                        ast::FloatFormat::Float => crate::FloatKind::Float,
                        ast::FloatFormat::Double => crate::FloatKind::Double,
                        ast::FloatFormat::LongDouble => crate::FloatKind::LongDouble,
                        _ => return Err(Error::new(offset, "unsupported floating-point type")),
                    })));
                }
            },
            ast::Expression::Identifier(identifier) => {
                if let Some(value) = self.unit.constants.get(&identifier.node.name) {
                    *value
                } else if let Some(declaration) = self
                    .unit
                    .declarations
                    .iter()
                    .find(|declaration| declaration.name == identifier.node.name)
                {
                    return Ok(declaration.ty.clone());
                } else {
                    return Err(Error::new(
                        offset,
                        format!("unknown identifier `{}`", identifier.node.name),
                    ));
                }
            }
            ast::Expression::Cast(cast) => {
                self.expression_type(&cast.node.expression)?;
                return self.type_name(&cast.node.type_name.node);
            }
            ast::Expression::UnaryOperator(unary) => {
                let ty = self.expression_type(&unary.node.operand)?;
                match unary.node.operator.node {
                    ast::UnaryOperator::Address => return Ok(ty.pointer()),
                    ast::UnaryOperator::Indirection => {
                        let TypeKind::Pointer(pointee) = &self.unit.resolve(&ty)?.kind else {
                            return Err(Error::new(offset, "indirection requires a pointer"));
                        };
                        return Ok((**pointee).clone());
                    }
                    ast::UnaryOperator::Negate => {
                        self.require_scalar(&ty, offset)?;
                        IntegerValue::int(0)
                    }
                    _ => promote(self.integer_type(&ty, offset)?),
                }
            }
            ast::Expression::BinaryOperator(binary) => {
                let left = self.expression_type(&binary.node.lhs)?;
                let right = self.expression_type(&binary.node.rhs)?;
                if matches!(
                    binary.node.operator.node,
                    ast::BinaryOperator::Equals
                        | ast::BinaryOperator::NotEquals
                        | ast::BinaryOperator::Less
                        | ast::BinaryOperator::LessOrEqual
                        | ast::BinaryOperator::Greater
                        | ast::BinaryOperator::GreaterOrEqual
                        | ast::BinaryOperator::LogicalAnd
                        | ast::BinaryOperator::LogicalOr
                ) {
                    self.require_scalar(&left, offset)?;
                    self.require_scalar(&right, offset)?;
                    IntegerValue::int(0)
                } else {
                    if binary.node.operator.node == ast::BinaryOperator::Index {
                        return match &self.unit.resolve(&left)?.kind {
                            TypeKind::Array { element, .. } | TypeKind::Pointer(element) => {
                                Ok((**element).clone())
                            }
                            _ => Err(Error::new(offset, "index requires pointer or array")),
                        };
                    }
                    let left = self.integer_type(&left, offset)?;
                    if matches!(
                        binary.node.operator.node,
                        ast::BinaryOperator::ShiftLeft | ast::BinaryOperator::ShiftRight
                    ) {
                        self.integer_type(&right, offset)?;
                        promote(left)
                    } else {
                        common(left, self.integer_type(&right, offset)?)
                    }
                }
            }
            ast::Expression::Conditional(conditional) => {
                let condition = self.expression_type(&conditional.node.condition)?;
                self.require_scalar(&condition, offset)?;
                let left = self.expression_type(&conditional.node.then_expression)?;
                let right = self.expression_type(&conditional.node.else_expression)?;
                common(
                    self.integer_type(&left, offset)?,
                    self.integer_type(&right, offset)?,
                )
            }
            ast::Expression::SizeOfTy(size) => {
                let ty = self.type_name(&size.node.0.node)?;
                self.size_of(&ty, offset)?;
                self.size_value(0)
            }
            ast::Expression::SizeOfVal(size) => {
                let ty = self.expression_type(&size.node.0)?;
                self.size_of(&ty, offset)?;
                self.size_value(0)
            }
            ast::Expression::AlignOf(alignment) => {
                let ty = self.type_name(&alignment.node.0.node)?;
                self.unit.layout(&ty)?;
                self.size_value(0)
            }
            ast::Expression::OffsetOf(_) => {
                self.eval(expression)?;
                self.size_value(0)
            }
            ast::Expression::StringLiteral(strings) => {
                let decoded = crate::analyze::decode_strings(&strings.node, offset)?;
                return Ok(Type::new(TypeKind::Array {
                    element: Box::new(Type::new(TypeKind::Integer(IntegerKind::Char))),
                    length: Some(decoded.len() as u64 + 1),
                }));
            }
            ast::Expression::Member(member) => {
                let mut ty = self.expression_type(&member.node.expression)?;
                if member.node.operator.node == ast::MemberOperator::Indirect {
                    let TypeKind::Pointer(pointee) = &self.unit.resolve(&ty)?.kind else {
                        return Err(Error::new(offset, "indirect member requires a pointer"));
                    };
                    ty = (**pointee).clone();
                }
                return Ok(self
                    .field_offset(&ty, &member.node.identifier.node.name, offset)?
                    .1);
            }
            ast::Expression::Call(call) => {
                for argument in &call.node.arguments {
                    self.expression_type(argument)?;
                }
                let ty = self.expression_type(&call.node.callee)?;
                let mut ty = self.unit.resolve(&ty)?;
                if let TypeKind::Pointer(pointee) = &ty.kind {
                    ty = self.unit.resolve(pointee)?;
                }
                let TypeKind::Function(function) = &ty.kind else {
                    return Err(Error::new(offset, "callee is not a function"));
                };
                return Ok(function.return_type.clone());
            }
            _ => {
                return Err(Error::new(
                    offset,
                    "expression type inference is unsupported for this expression",
                ));
            }
        };
        Ok(integer_to_type(integer))
    }

    /// Validates an operand's type without evaluating its value. Arrays and
    /// function designators undergo their usual conversion to pointers.
    fn require_scalar(&self, ty: &Type, offset: usize) -> Result<(), Error> {
        if matches!(
            self.unit.resolve(ty)?.kind,
            TypeKind::Void | TypeKind::Record(_)
        ) {
            return Err(Error::new(offset, "operator requires a scalar operand"));
        }
        Ok(())
    }

    pub(crate) fn integer_type(&self, ty: &Type, offset: usize) -> Result<IntegerValue, Error> {
        let ty = self.unit.resolve(ty)?;
        let (bits, signed, rank) = match ty.kind {
            TypeKind::Bool => (8, false, 0),
            TypeKind::Integer(kind) => match kind {
                IntegerKind::Char => (8, self.unit.target.char_is_signed(), 1),
                IntegerKind::SignedChar => (8, true, 1),
                IntegerKind::UnsignedChar => (8, false, 1),
                IntegerKind::Short => (16, true, 2),
                IntegerKind::UnsignedShort => (16, false, 2),
                IntegerKind::Int => (32, true, 3),
                IntegerKind::UnsignedInt => (32, false, 3),
                IntegerKind::Long => (self.unit.target.long_width() as u8, true, 4),
                IntegerKind::UnsignedLong => (self.unit.target.long_width() as u8, false, 4),
                IntegerKind::LongLong => (64, true, 5),
                IntegerKind::UnsignedLongLong => (64, false, 5),
                IntegerKind::Int128 => (128, true, 6),
                IntegerKind::UnsignedInt128 => (128, false, 6),
            },
            TypeKind::Enum(_) => {
                let layout = self.unit.layout(ty)?;
                let TypeKind::Enum(id) = ty.kind else {
                    unreachable!()
                };
                let signed = self.unit.target == toucan_target::Target::X86_64PcWindowsMsvc
                    || self.unit.enums[id]
                        .variants
                        .iter()
                        .any(|variant| variant.value.signed && variant.value.signed_value() < 0);
                let bits = layout.size_bits as u8;
                (bits, signed, if bits <= 32 { 3 } else { 4 })
            }
            _ => return Err(Error::new(offset, "expected an integer type")),
        };
        Ok(IntegerValue::new(0, bits, signed, rank))
    }

    fn literal(&self, literal: &ast::Integer, offset: usize) -> Result<IntegerValue, Error> {
        if literal.suffix.imaginary {
            return Err(Error::new(
                offset,
                "imaginary integer constants are unsupported",
            ));
        }
        let radix = match literal.base {
            ast::IntegerBase::Decimal => 10,
            ast::IntegerBase::Octal => 8,
            ast::IntegerBase::Hexadecimal => 16,
            ast::IntegerBase::Binary => 2,
        };
        let digits = literal.number.as_ref();
        let digits = if radix == 16 {
            digits
                .strip_prefix("0x")
                .or_else(|| digits.strip_prefix("0X"))
                .unwrap_or(digits)
        } else if radix == 2 {
            digits
                .strip_prefix("0b")
                .or_else(|| digits.strip_prefix("0B"))
                .unwrap_or(digits)
        } else {
            digits
        };
        let value = u128::from_str_radix(digits, radix)
            .map_err(|_| Error::new(offset, "invalid or oversized integer literal"))?;
        let minimum_rank = match literal.suffix.size {
            ast::IntegerSize::Int => 3,
            ast::IntegerSize::Long => 4,
            ast::IntegerSize::LongLong => 5,
        };
        for rank in minimum_rank..=5 {
            let bits = match rank {
                3 => 32,
                4 => self.unit.target.long_width() as u8,
                _ => 64,
            };
            if !literal.suffix.unsigned && value < (1u128 << (bits - 1)) {
                return Ok(IntegerValue::new(value, bits, true, rank));
            }
            if (literal.suffix.unsigned || radix != 10) && value <= IntegerValue::mask(bits) {
                return Ok(IntegerValue::new(value, bits, false, rank));
            }
        }
        Err(Error::new(
            offset,
            "integer literal exceeds supported C integer types",
        ))
    }

    pub(crate) fn integer_add_one(
        &self,
        value: IntegerValue,
        offset: usize,
    ) -> Result<IntegerValue, Error> {
        if value.signed {
            signed_result(value.signed_value().checked_add(1), value, offset)
        } else if value.value == IntegerValue::mask(value.bits) {
            Err(Error::new(offset, "enumerator increment overflows"))
        } else {
            Ok(IntegerValue::new(
                value.value + 1,
                value.bits,
                false,
                value.rank,
            ))
        }
    }

    fn binary(
        &self,
        operator: &ast::BinaryOperator,
        left: IntegerValue,
        right: IntegerValue,
        offset: usize,
    ) -> Result<IntegerValue, Error> {
        use ast::BinaryOperator as Op;
        if matches!(operator, Op::LogicalAnd | Op::LogicalOr) {
            return Ok(IntegerValue::int(i128::from(
                if *operator == Op::LogicalAnd {
                    left.truth() && right.truth()
                } else {
                    left.truth() || right.truth()
                },
            )));
        }
        if matches!(operator, Op::ShiftLeft | Op::ShiftRight) {
            let left = promote(left);
            let right = promote(right);
            let count = right
                .as_u64()
                .map_err(|_| Error::new(offset, "negative shift count"))?;
            if count >= u64::from(left.bits) {
                return Err(Error::new(offset, "shift count exceeds integer width"));
            }
            if *operator == Op::ShiftRight {
                return Ok(IntegerValue::new(
                    if left.signed {
                        (left.signed_value() >> count) as u128
                    } else {
                        left.value >> count
                    },
                    left.bits,
                    left.signed,
                    left.rank,
                ));
            }
            if left.signed {
                if left.signed_value() < 0 {
                    return Err(Error::new(
                        offset,
                        "left shift of a negative signed integer",
                    ));
                }
                let maximum = IntegerValue::mask(left.bits) >> 1;
                if left.value > (maximum >> count) {
                    return Err(Error::new(offset, "signed left shift overflows"));
                }
                return signed_result(left.signed_value().checked_shl(count as u32), left, offset);
            }
            return Ok(IntegerValue::new(
                left.value << count,
                left.bits,
                false,
                left.rank,
            ));
        }
        let ty = common(left, right);
        let left = convert(left, ty);
        let right = convert(right, ty);
        let comparison = match operator {
            Op::Equals => Some(left.value == right.value),
            Op::NotEquals => Some(left.value != right.value),
            Op::Less => Some(if ty.signed {
                left.signed_value() < right.signed_value()
            } else {
                left.value < right.value
            }),
            Op::LessOrEqual => Some(if ty.signed {
                left.signed_value() <= right.signed_value()
            } else {
                left.value <= right.value
            }),
            Op::Greater => Some(if ty.signed {
                left.signed_value() > right.signed_value()
            } else {
                left.value > right.value
            }),
            Op::GreaterOrEqual => Some(if ty.signed {
                left.signed_value() >= right.signed_value()
            } else {
                left.value >= right.value
            }),
            _ => None,
        };
        if let Some(result) = comparison {
            return Ok(IntegerValue::int(i128::from(result)));
        }
        if matches!(operator, Op::Divide | Op::Modulo) && right.value == 0 {
            return Err(Error::new(
                offset,
                "division by zero in constant expression",
            ));
        }
        if ty.signed
            && matches!(
                operator,
                Op::Plus | Op::Minus | Op::Multiply | Op::Divide | Op::Modulo
            )
        {
            let a = left.signed_value();
            let b = right.signed_value();
            if matches!(operator, Op::Divide | Op::Modulo)
                && a == signed_minimum(ty.bits)
                && b == -1
            {
                return Err(Error::new(offset, "signed integer division overflows"));
            }
            return signed_result(
                match operator {
                    Op::Plus => a.checked_add(b),
                    Op::Minus => a.checked_sub(b),
                    Op::Multiply => a.checked_mul(b),
                    Op::Divide => a.checked_div(b),
                    Op::Modulo => a.checked_rem(b),
                    _ => unreachable!(),
                },
                ty,
                offset,
            );
        }
        let value = match operator {
            Op::Plus => left.value.wrapping_add(right.value),
            Op::Minus => left.value.wrapping_sub(right.value),
            Op::Multiply => left.value.wrapping_mul(right.value),
            Op::Divide => left.value / right.value,
            Op::Modulo => left.value % right.value,
            Op::BitwiseAnd => left.value & right.value,
            Op::BitwiseOr => left.value | right.value,
            Op::BitwiseXor => left.value ^ right.value,
            _ => {
                return Err(Error::new(
                    offset,
                    "operator is not permitted in an integer constant expression",
                ));
            }
        };
        Ok(IntegerValue::new(value, ty.bits, ty.signed, ty.rank))
    }
}

fn promote(value: IntegerValue) -> IntegerValue {
    if value.rank < 3 {
        convert(value, IntegerValue::int(0))
    } else {
        value
    }
}

fn convert(value: IntegerValue, destination: IntegerValue) -> IntegerValue {
    IntegerValue::new(
        if value.signed {
            value.signed_value() as u128
        } else {
            value.value
        },
        destination.bits,
        destination.signed,
        destination.rank,
    )
}

fn common(left: IntegerValue, right: IntegerValue) -> IntegerValue {
    let left = promote(left);
    let right = promote(right);
    if left.signed == right.signed {
        return if left.rank >= right.rank { left } else { right };
    }
    let (signed, unsigned) = if left.signed {
        (left, right)
    } else {
        (right, left)
    };
    if unsigned.rank >= signed.rank {
        unsigned
    } else if signed.bits > unsigned.bits {
        signed
    } else {
        IntegerValue::new(0, signed.bits, false, signed.rank)
    }
}

fn signed_result(
    value: Option<i128>,
    ty: IntegerValue,
    offset: usize,
) -> Result<IntegerValue, Error> {
    let value = value.ok_or_else(|| Error::new(offset, "signed integer overflow"))?;
    let maximum = if ty.bits == 128 {
        i128::MAX
    } else {
        (1i128 << (ty.bits - 1)) - 1
    };
    let minimum = signed_minimum(ty.bits);
    if value < minimum || value > maximum {
        return Err(Error::new(offset, "signed integer overflow"));
    }
    Ok(IntegerValue::new(value as u128, ty.bits, true, ty.rank))
}

fn integer_to_type(value: IntegerValue) -> Type {
    Type::new(TypeKind::Integer(match (value.rank, value.signed) {
        (0 | 1, true) => IntegerKind::SignedChar,
        (0 | 1, false) => IntegerKind::UnsignedChar,
        (2, true) => IntegerKind::Short,
        (2, false) => IntegerKind::UnsignedShort,
        (3, true) => IntegerKind::Int,
        (3, false) => IntegerKind::UnsignedInt,
        (4, true) => IntegerKind::Long,
        (4, false) => IntegerKind::UnsignedLong,
        (6, true) => IntegerKind::Int128,
        (6, false) => IntegerKind::UnsignedInt128,
        (_, true) => IntegerKind::LongLong,
        (_, false) => IntegerKind::UnsignedLongLong,
    }))
}

fn signed_minimum(bits: u8) -> i128 {
    if bits == 128 {
        i128::MIN
    } else {
        -(1i128 << (bits - 1))
    }
}

fn character_value(character: &str, offset: usize) -> Result<IntegerValue, Error> {
    let Some(body) = character
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    else {
        return Err(Error::new(
            offset,
            "wide character constants are unsupported",
        ));
    };
    let value = if let Some(escape) = body.strip_prefix('\\') {
        match escape {
            "n" => 10,
            "r" => 13,
            "t" => 9,
            "a" => 7,
            "b" => 8,
            "f" => 12,
            "v" => 11,
            "\\" => 92,
            "'" => 39,
            "\"" => 34,
            "?" => 63,
            value if value.starts_with('x') => u32::from_str_radix(&value[1..], 16)
                .map_err(|_| Error::new(offset, "invalid hexadecimal character escape"))?,
            value if value.len() <= 3 && value.chars().all(|ch| matches!(ch, '0'..='7')) => {
                u32::from_str_radix(value, 8)
                    .map_err(|_| Error::new(offset, "invalid octal character escape"))?
            }
            _ => return Err(Error::new(offset, "unsupported character escape")),
        }
    } else if body.len() == 1 && body.is_ascii() {
        u32::from(body.as_bytes()[0])
    } else {
        return Err(Error::new(
            offset,
            "multicharacter and non-ASCII constants depend on unsupported execution character sets",
        ));
    };
    if value > 127 {
        return Err(Error::new(
            offset,
            "character constant depends on the execution character set",
        ));
    }
    Ok(IntegerValue::int(i128::from(value)))
}

use lang_c::{ast, span::Node};

use crate::{Error, IntegerKind, IntegerValue, Type, TypeKind, analyze::Analyzer};

impl Analyzer {
    /// Distinguishes integer constant expressions from runtime bounds, including
    /// expressions that a compiler could fold but which are not C11 ICEs.
    pub(crate) fn is_integer_constant_expression(
        &mut self,
        expression: &Node<ast::Expression>,
        depth: usize,
    ) -> Result<bool, Error> {
        let previous = self
            .parameter_type_dependencies
            .as_mut()
            .map(|dependencies| dependencies.suspend());
        let result = self.is_integer_constant_expression_inner(expression, depth);
        if let (Some(dependencies), Some(previous)) =
            (&mut self.parameter_type_dependencies, previous)
        {
            dependencies.restore_suspension(previous);
        }
        result
    }

    fn is_integer_constant_expression_inner(
        &mut self,
        expression: &Node<ast::Expression>,
        depth: usize,
    ) -> Result<bool, Error> {
        if depth >= 128 {
            return Err(Error::new(
                expression.span.start,
                "constant expression nesting exceeds the 128-level limit",
            ));
        }
        Ok(match &expression.node {
            ast::Expression::Constant(constant) => {
                !matches!(constant.node, ast::Constant::Float(_))
            }
            ast::Expression::Identifier(identifier) => {
                self.unit.constants.contains_key(&identifier.node.name)
            }
            ast::Expression::Cast(cast) => {
                let ty = self.type_name(&cast.node.type_name.node)?;
                let integer = matches!(
                    self.unit.resolve(&ty)?.kind,
                    TypeKind::Integer(_) | TypeKind::Bool | TypeKind::Enum(_)
                );
                integer
                    && (self
                        .null_base_member_offset(&cast.node.expression)?
                        .is_some()
                        || matches!(&cast.node.expression.node, ast::Expression::Constant(constant) if matches!(&constant.node, ast::Constant::Float(literal) if !literal.suffix.imaginary))
                        || self.is_integer_constant_expression(&cast.node.expression, depth + 1)?)
            }
            ast::Expression::UnaryOperator(unary) => {
                matches!(
                    unary.node.operator.node,
                    ast::UnaryOperator::Plus
                        | ast::UnaryOperator::Minus
                        | ast::UnaryOperator::Complement
                        | ast::UnaryOperator::Negate
                        | ast::UnaryOperator::Real
                        | ast::UnaryOperator::Imaginary
                ) && self.is_integer_constant_expression(&unary.node.operand, depth + 1)?
            }
            ast::Expression::BinaryOperator(binary) => {
                use ast::BinaryOperator as Op;
                matches!(
                    binary.node.operator.node,
                    Op::Multiply
                        | Op::Divide
                        | Op::Modulo
                        | Op::Plus
                        | Op::Minus
                        | Op::ShiftLeft
                        | Op::ShiftRight
                        | Op::Less
                        | Op::Greater
                        | Op::LessOrEqual
                        | Op::GreaterOrEqual
                        | Op::Equals
                        | Op::NotEquals
                        | Op::BitwiseAnd
                        | Op::BitwiseOr
                        | Op::BitwiseXor
                        | Op::LogicalAnd
                        | Op::LogicalOr
                ) && self.is_integer_constant_expression(&binary.node.lhs, depth + 1)?
                    && self.is_integer_constant_expression(&binary.node.rhs, depth + 1)?
            }
            ast::Expression::Conditional(conditional) => {
                self.is_integer_constant_expression(&conditional.node.condition, depth + 1)?
                    && match &conditional.node.then_expression {
                        Some(value) => self.is_integer_constant_expression(value, depth + 1)?,
                        None => true,
                    }
                    && self.is_integer_constant_expression(
                        &conditional.node.else_expression,
                        depth + 1,
                    )?
            }
            ast::Expression::SizeOfTy(size) => {
                let ty = self.sizeof_type_name(&size.node.0.node)?;
                !self.unit.is_variable_length_array(&ty)?
            }
            ast::Expression::SizeOfVal(size) => {
                let checkpoint = self.sve_feature_checkpoint();
                let allocation_context = self.allocation_context(false);
                let ty = self.expression_type(&size.node.0);
                let ty =
                    self.finish_allocation_operand(allocation_context, ty, |analyzer, ty| {
                        analyzer.unit.is_variable_length_array(ty)
                    })?;
                if !self.unit.is_variable_length_array(&ty)? {
                    self.discard_sve_feature_uses(checkpoint);
                }
                !self.unit.is_variable_length_array(&ty)?
            }
            ast::Expression::AlignOf(_) => true,
            ast::Expression::OffsetOf(offset) => {
                for member in &offset.node.designator.node.members {
                    if let ast::OffsetMember::Index(index) = &member.node
                        && !self.is_integer_constant_expression(index, depth + 1)?
                    {
                        return Ok(false);
                    }
                }
                true
            }
            ast::Expression::TypesCompatible(query) => {
                self.eval_types_compatible(query)?;
                true
            }
            ast::Expression::Choose(selection) => {
                let selected = self.choose_expression(selection)?;
                self.is_integer_constant_expression(selected, depth + 1)?
            }
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                self.is_integer_constant_expression(selected, depth + 1)?
            }
            ast::Expression::Call(call)
                if self.builtin_name(call) == Some("__c11_atomic_is_lock_free") =>
            {
                self.eval_c11_atomic_lock_free(call).is_ok()
            }
            ast::Expression::Call(call)
                if self
                    .builtin_name(call)
                    .and_then(crate::atomic::AtomicOperation::from_name)
                    .is_some_and(crate::atomic::AtomicOperation::is_lock_free_query) =>
            {
                self.eval_atomic_lock_free(call).is_ok()
            }
            ast::Expression::Call(call)
                if self.builtin_name(call).is_some_and(|name| {
                    self.byte_swap_type(name).is_some() || self.bit_count_type(name).is_some()
                }) =>
            {
                self.eval(expression).is_ok()
            }
            ast::Expression::Call(call)
                if self
                    .builtin_name(call)
                    .and_then(crate::overflow::OverflowIntrinsic::from_name)
                    .is_some_and(crate::overflow::OverflowIntrinsic::is_predicate) =>
            {
                self.eval_overflow_predicate(call).is_ok()
            }
            ast::Expression::Call(call) if self.builtin_name(call) == Some("__builtin_expect") => {
                self.eval_expect(call).is_ok()
            }
            ast::Expression::Call(call)
                if self
                    .builtin_name(call)
                    .and_then(|name| self.object_size_signature(name))
                    .is_some() =>
            {
                self.infer_object_size(call)?.frontend_fold()
            }
            ast::Expression::Call(call)
                if self.builtin_name(call) == Some("__builtin_constant_p") =>
            {
                self.builtin_call_type(call)?;
                true
            }
            _ => false,
        })
    }

    pub(crate) fn eval(
        &mut self,
        expression: &Node<ast::Expression>,
    ) -> Result<IntegerValue, Error> {
        let checkpoint = self
            .checked
            .as_ref()
            .map(|checked| checked.evaluation_checkpoint());
        self.enter_expression(expression.span.start)?;
        let result = self.eval_inner(expression);
        self.leave_expression();
        let value = result?;
        // Preserve the ordinary evaluator's first diagnostic. Type names may
        // already have created bounds or typeof operands; keep their enclosing
        // context when recording a successful expression through the cache.
        if let (Some(checked), Some(checkpoint)) = (&mut self.checked, checkpoint)
            && checked.prepare_evaluated_expression(expression, checkpoint)?
        {
            self.expression_info(expression)?;
        }
        Ok(value)
    }

    fn eval_inner(&mut self, expression: &Node<ast::Expression>) -> Result<IntegerValue, Error> {
        let offset = expression.span.start;
        match &expression.node {
            ast::Expression::Constant(constant) => match &constant.node {
                ast::Constant::Integer(integer) => self.literal(integer, offset),
                ast::Constant::Character(character) => {
                    crate::decode_character_literal_with_profile(
                        character,
                        self.unit.profile()?,
                        offset,
                    )
                }
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
            ast::Expression::TypesCompatible(query) => self.eval_types_compatible(query),
            ast::Expression::Choose(selection) => {
                let selected = self.choose_expression(selection)?;
                self.eval(selected)
            }
            ast::Expression::GenericSelection(selection) => {
                let selected = self.generic_expression(selection)?;
                self.eval(selected)
            }
            ast::Expression::Call(call)
                if self.builtin_name(call) == Some("__c11_atomic_is_lock_free") =>
            {
                self.eval_c11_atomic_lock_free(call)
            }
            ast::Expression::Call(call)
                if self
                    .builtin_name(call)
                    .and_then(crate::atomic::AtomicOperation::from_name)
                    .is_some_and(crate::atomic::AtomicOperation::is_lock_free_query) =>
            {
                self.eval_atomic_lock_free(call)
            }
            ast::Expression::Call(call)
                if self
                    .builtin_name(call)
                    .and_then(crate::overflow::OverflowIntrinsic::from_name)
                    .is_some_and(crate::overflow::OverflowIntrinsic::is_predicate) =>
            {
                self.eval_overflow_predicate(call)
            }
            ast::Expression::Call(call) if self.builtin_name(call) == Some("__builtin_expect") => {
                self.eval_expect(call)
            }
            ast::Expression::Call(call)
                if self.builtin_name(call) == Some("__builtin_constant_p") =>
            {
                self.eval_constant_query(call)
            }
            ast::Expression::Call(call)
                if self
                    .builtin_name(call)
                    .and_then(|name| self.object_size_signature(name))
                    .is_some() =>
            {
                self.eval_object_size(call)
            }
            ast::Expression::Call(call)
                if self
                    .builtin_name(call)
                    .and_then(|name| self.byte_swap_type(name))
                    .is_some() =>
            {
                self.eval_byte_swap(call)
            }
            ast::Expression::Call(call)
                if self
                    .builtin_name(call)
                    .and_then(|name| self.bit_count_type(name))
                    .is_some() =>
            {
                self.eval_bit_count(call)
            }
            ast::Expression::Cast(cast) => {
                let ty = self.type_name(&cast.node.type_name.node)?;
                let destination = self.integer_type(&ty, offset)?;
                // C11 6.6 permits a floating constant as an immediate operand
                // of a cast to integer type in an integer constant expression.
                if let ast::Expression::Constant(constant) = &cast.node.expression.node
                    && let ast::Constant::Float(literal) = &constant.node
                    && !literal.suffix.imaginary
                {
                    let value = self.floating_literal(literal, offset)?;
                    return self.convert_arithmetic(value, &ty, offset)?.integer(offset);
                }
                let value =
                    if let Some(bytes) = self.null_base_member_offset(&cast.node.expression)? {
                        self.expression_type(&cast.node.expression)?;
                        self.size_value(bytes)
                    } else {
                        self.eval(&cast.node.expression)?
                    };
                Ok(if matches!(self.unit.resolve(&ty)?.kind, TypeKind::Bool) {
                    IntegerValue::new(u128::from(value.truth()), 8, false, 0)
                } else {
                    convert(value, destination)
                })
            }
            ast::Expression::UnaryOperator(unary) => {
                let value = self.eval(&unary.node.operand)?;
                if unary.node.operator.node == ast::UnaryOperator::Real {
                    return Ok(value);
                }
                if unary.node.operator.node == ast::UnaryOperator::Imaginary {
                    return Ok(IntegerValue::new(0, value.bits, value.signed, value.rank));
                }
                let value = promote(value);
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
                let left_ty = self.expression_type(conditional.node.nonzero_expression())?;
                let right_ty = self.expression_type(&conditional.node.else_expression)?;
                let destination = common(
                    self.integer_type(&left_ty, offset)?,
                    self.integer_type(&right_ty, offset)?,
                );
                let condition = self.eval(&conditional.node.condition)?;
                let value = if condition.truth() {
                    match &conditional.node.then_expression {
                        Some(value) => self.eval(value)?,
                        None => condition,
                    }
                } else {
                    self.eval(&conditional.node.else_expression)?
                };
                Ok(convert(value, destination))
            }
            ast::Expression::SizeOfTy(size) => {
                let ty = self.sizeof_type_name(&size.node.0.node)?;
                self.size_of(&ty, offset)
            }
            ast::Expression::SizeOfVal(size) => self.sizeof_expression(&size.node.0),
            ast::Expression::AlignOf(alignment) => {
                let bytes = self.alignment_query(alignment)?;
                Ok(self.size_value(bytes))
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

    pub(crate) fn size_of(&self, ty: &Type, offset: usize) -> Result<IntegerValue, Error> {
        if self.unit.is_variable_length_array(ty)? {
            return Err(Error::new(
                offset,
                "sizeof a variable-length array is not an integer constant expression",
            ));
        }
        match self.unit.resolve(ty)?.kind {
            TypeKind::Void | TypeKind::Function(_) => return Ok(self.size_value(1)),
            TypeKind::Array { length: None, .. } => {
                return Err(Error::new(offset, "sizeof requires a complete object type"));
            }
            _ => {}
        }
        Ok(self.size_value(self.unit.layout(ty)?.size_bytes()))
    }

    pub(crate) fn size_value(&self, value: u64) -> IntegerValue {
        let bits = self.unit.target.pointer_width() as u8;
        IntegerValue::new(
            u128::from(value),
            bits,
            false,
            if bits == 32 {
                3
            } else if self.unit.target.long_width() == u64::from(bits) {
                4
            } else {
                5
            },
        )
    }

    pub(crate) fn field_offset(
        &self,
        ty: &Type,
        name: &str,
        offset: usize,
    ) -> Result<(u64, Type), Error> {
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

    /// Fold only the offset-macro spelling `(integer)&(((record*)0)->field)`.
    /// This is a compiler extension, so an arbitrary pointer address or dereference
    /// must never become an integer constant through this path.
    pub(crate) fn null_base_member_offset(
        &mut self,
        address: &Node<ast::Expression>,
    ) -> Result<Option<u64>, Error> {
        use ast::{BinaryOperator as Binary, Expression as E, MemberOperator as Member};

        let E::UnaryOperator(address) = &address.node else {
            return Ok(None);
        };
        if address.node.operator.node != ast::UnaryOperator::Address {
            return Ok(None);
        }
        let mut path = Vec::new();
        let mut current = address.node.operand.as_ref();
        loop {
            if path.len() >= 128 {
                return Err(Error::new(
                    current.span.start,
                    "offsetof nesting limit exceeded",
                ));
            }
            match &current.node {
                E::Member(member) => {
                    path.push(current);
                    current = member.node.expression.as_ref();
                    if member.node.operator.node == Member::Indirect {
                        break;
                    }
                }
                E::BinaryOperator(binary) if binary.node.operator.node == Binary::Index => {
                    path.push(current);
                    current = binary.node.lhs.as_ref();
                }
                _ => return Ok(None),
            }
        }
        let E::Cast(base) = &current.node else {
            return Ok(None);
        };
        let E::Constant(zero) = &base.node.expression.node else {
            return Ok(None);
        };
        let ast::Constant::Integer(literal) = &zero.node else {
            return Ok(None);
        };
        if self.literal(literal, zero.span.start)?.value != 0 {
            return Ok(None);
        }
        let pointer = self.type_name(&base.node.type_name.node)?;
        let TypeKind::Pointer(pointee) = &self.unit.resolve(&pointer)?.kind else {
            return Ok(None);
        };
        let mut ty = (**pointee).clone();
        if !matches!(self.unit.resolve(&ty)?.kind, TypeKind::Record(_)) {
            return Ok(None);
        }
        let mut bytes = 0u64;
        for (index, step) in path.into_iter().rev().enumerate() {
            let delta = match &step.node {
                E::Member(member)
                    if (index == 0 && member.node.operator.node == Member::Indirect)
                        || (index > 0 && member.node.operator.node == Member::Direct) =>
                {
                    let (delta, field) =
                        self.field_offset(&ty, &member.node.identifier.node.name, step.span.start)?;
                    ty = field;
                    delta
                }
                E::BinaryOperator(binary) if binary.node.operator.node == Binary::Index => {
                    let TypeKind::Array { element, .. } = &self.unit.resolve(&ty)?.kind else {
                        return Ok(None);
                    };
                    let element = (**element).clone();
                    if !self.is_integer_constant_expression(&binary.node.rhs, 0)? {
                        return Ok(None);
                    }
                    let at = self
                        .eval(&binary.node.rhs)?
                        .as_u64()
                        .map_err(|error| Error::new(binary.node.rhs.span.start, error.message))?;
                    let stride = self.unit.layout(&element)?.size_bytes();
                    ty = element;
                    at.checked_mul(stride)
                        .ok_or_else(|| Error::new(step.span.start, "offsetof overflow"))?
                }
                _ => return Ok(None),
            };
            bytes = bytes
                .checked_add(delta)
                .ok_or_else(|| Error::new(step.span.start, "offsetof overflow"))?;
        }
        Ok(Some(bytes))
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
            TypeKind::Enum(id) => {
                let kind = self.unit.enum_integer_kind(id)?;
                return self.integer_type(&Type::new(TypeKind::Integer(kind)), offset);
            }
            _ => return Err(Error::new(offset, "expected an integer type")),
        };
        Ok(IntegerValue::new(0, bits, signed, rank))
    }

    pub(crate) fn literal(
        &self,
        literal: &ast::Integer,
        offset: usize,
    ) -> Result<IntegerValue, Error> {
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
            ast::IntegerSize::Msvc(width) => {
                if !self.unit.target.is_windows() {
                    return Err(Error::new(
                        offset,
                        "Microsoft integer suffix requires a Windows profile",
                    ));
                }
                let kind = match width {
                    8 => IntegerKind::Char,
                    16 => IntegerKind::Short,
                    32 => IntegerKind::Int,
                    64 => IntegerKind::LongLong,
                    _ => {
                        return Err(Error::new(
                            offset,
                            "unsupported Microsoft integer suffix width",
                        ));
                    }
                };
                if value > u128::from(u64::MAX) {
                    return Err(Error::new(
                        offset,
                        "integer literal exceeds supported C integer types",
                    ));
                }
                let ty = self.integer_type(&Type::new(TypeKind::Integer(kind)), offset)?;
                return Ok(IntegerValue::new(
                    value,
                    ty.bits,
                    !literal.suffix.unsigned,
                    ty.rank,
                ));
            }
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
            let c90_unsigned = self.unit.language_mode.is_c90() && rank >= 4
                // Clang's MS mode recovers an oversized explicitly signed LL
                // literal by wrapping its value. Preserve the existing rejection
                // instead of giving that recovery an unsigned type.
                && !(self.unit.target.is_windows()
                    && minimum_rank == 5);
            if (literal.suffix.unsigned || radix != 10 || c90_unsigned)
                && value <= IntegerValue::mask(bits)
            {
                return Ok(IntegerValue::new(value, bits, false, rank));
            }
        }
        Err(Error::new(
            offset,
            "integer literal exceeds supported C integer types",
        ))
    }

    /// Applies the compiler's final enumerator types after every initializer has
    /// been evaluated with the types visible inside the definition.
    pub(crate) fn finish_enum(&mut self, id: usize, offset: usize) -> Result<(), Error> {
        let destination = self.integer_type(&Type::new(TypeKind::Enum(id)), offset)?;
        let gnu = self.unit.compiler == toucan_target::Compiler::Gnu;
        if !gnu && destination.bits > 64 {
            return Err(Error::new(
                offset,
                "enum values exceed the target's supported integer range",
            ));
        }
        let variants = &mut self.unit.enums[id].variants;
        let wider_than_int = variants.iter().any(|variant| !variant.value.fits_int());
        for variant in variants {
            let value = variant.value;
            // The Windows MSVC enum ABI uses signed int even for unsigned
            // 32-bit enumerators. Clang and MSVC reinterpret SDK sentinels
            // such as 0xffffffff as -1 when the definition closes. Preserve
            // this bounded conversion, but reject wider lossy recoveries.
            let converted = convert(value, destination);
            let representable = if value.signed && value.signed_value() < 0 {
                converted.signed && value.signed_value() == converted.signed_value()
            } else {
                (!converted.signed || converted.signed_value() >= 0)
                    && value.value == converted.value
            };
            let windows_u32_sentinel = self.unit.target.is_windows()
                && destination.bits == 32
                && destination.signed
                && !value.signed
                && value.value <= u128::from(u32::MAX);
            if !representable && !windows_u32_sentinel {
                return Err(Error::new(
                    offset,
                    "enum value is not representable in its compatible integer type",
                ));
            }
            // GCC applies its C23 rule in older language modes too: one value
            // outside int changes every enumerator's type after the closing
            // brace. Clang keeps the individually representable values as int.
            if (gnu && wider_than_int) || !value.fits_int() {
                variant.value = converted;
                self.unit.constants.insert(variant.name.clone(), converted);
            }
        }
        Ok(())
    }

    /// Adds one for an implicit enumerator, widening on overflow while preserving
    /// signedness. The wider type remains visible to subsequent initializers.
    pub(crate) fn integer_add_one(
        &self,
        mut value: IntegerValue,
        offset: usize,
    ) -> Result<IntegerValue, Error> {
        let maximum = IntegerValue::mask(value.bits) >> u32::from(value.signed);
        if value.value == maximum {
            let gnu = self.unit.compiler == toucan_target::Compiler::Gnu;
            let wider = [(self.unit.target.long_width() as u8, 4), (64, 5), (128, 6)]
                .into_iter()
                .find(|(bits, _)| *bits > value.bits && (*bits <= 64 || gnu))
                .ok_or_else(|| {
                    Error::new(
                        offset,
                        "enumerator increment exceeds supported integer types",
                    )
                })?;
            value = convert(value, IntegerValue::new(0, wider.0, value.signed, wider.1));
        }
        if value.signed {
            signed_result(value.signed_value().checked_add(1), value, offset)
        } else {
            Ok(IntegerValue::new(
                value.value + 1,
                value.bits,
                false,
                value.rank,
            ))
        }
    }

    pub(crate) fn binary(
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

pub(crate) fn promote(value: IntegerValue) -> IntegerValue {
    if value.rank < 3 {
        convert(value, IntegerValue::int(0))
    } else {
        value
    }
}

pub(crate) fn convert(value: IntegerValue, destination: IntegerValue) -> IntegerValue {
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

pub(crate) fn common(left: IntegerValue, right: IntegerValue) -> IntegerValue {
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

pub(crate) fn signed_result(
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

pub(crate) fn integer_to_type(value: IntegerValue) -> Type {
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

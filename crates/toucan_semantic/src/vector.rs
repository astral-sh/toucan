//! GNU fixed-size vector types and lane operations.

use lang_c::{ast, span::Node};
use toucan_target::Target;

use crate::analyze::Analyzer;
use crate::floating::ArithmeticValue;
use crate::{Error, FloatKind, IntegerKind, Qualifiers, Type, TypeKind};

impl<'ast> Analyzer<'ast> {
    /// Converts corresponding lanes numerically; the written destination keeps
    /// its qualifiers and alignment until an enclosing value conversion.
    pub(crate) fn convert_vector_type(
        &mut self,
        conversion: &Node<ast::ConvertVectorExpression>,
    ) -> Result<Type, Error> {
        let source = self.expression_type(&conversion.node.expression)?;
        let destination = self.type_name(&conversion.node.type_name.node)?;
        let source_shape = if self.gnu_vector_profile() {
            self.unit.atomic_value(&source)?.unwrap_or(&source)
        } else {
            &source
        };
        let source = self.unit.resolve(source_shape)?;
        let destination_shape = if self.gnu_vector_profile() {
            self.unit
                .atomic_value(&destination)?
                .unwrap_or(&destination)
        } else {
            &destination
        };
        let destination_shape = self.unit.resolve(destination_shape)?;
        let TypeKind::Vector {
            lanes: source_lanes,
            ..
        } = source.kind
        else {
            return Err(Error::new(
                conversion.node.expression.span.start,
                "first argument to __builtin_convertvector must have fixed vector type",
            ));
        };
        let TypeKind::Vector {
            lanes: destination_lanes,
            ..
        } = destination_shape.kind
        else {
            return Err(Error::new(
                conversion.node.type_name.span.start,
                "second argument to __builtin_convertvector must be a fixed vector type",
            ));
        };
        if source_lanes != destination_lanes {
            return Err(Error::new(
                conversion.span.start,
                "__builtin_convertvector requires equal source and destination lane counts",
            ));
        }
        Ok(destination)
    }

    /// Applies GNU vector_size to a scalar base, including GCC's derived-type spelling.
    pub(crate) fn vector_type(&self, ty: Type, bytes: u64, offset: usize) -> Result<Type, Error> {
        self.vector_type_at(ty, bytes, offset, 0)
    }

    fn vector_type_at(
        &self,
        ty: Type,
        bytes: u64,
        offset: usize,
        depth: usize,
    ) -> Result<Type, Error> {
        if depth >= 128 {
            return Err(Error::new(
                offset,
                "vector declarator nesting exceeds the 128-level limit",
            ));
        }
        if bytes == 0 {
            return Err(Error::new(
                offset,
                "vector_size requires a positive byte count",
            ));
        }
        if !bytes.is_power_of_two() {
            return Err(Error::new(
                offset,
                "non-power-of-two vector_size is unsupported",
            ));
        }
        if bytes > 16 {
            return Err(Error::new(
                offset,
                "vectors larger than 16 bytes require unsupported target-feature configuration",
            ));
        }
        let mut resolved = self.unit.resolve(&ty)?.clone();
        resolved.qualifiers = self.unit.qualifiers(&ty)?;
        resolved.alignment = crate::TypeAlignment::default();
        let scalar = match &mut resolved.kind {
            TypeKind::Pointer(element)
            | TypeKind::Array { element, .. }
            | TypeKind::VariableArray { element, .. } => Some(element.as_mut()),
            TypeKind::Function(function) => Some(&mut function.return_type),
            _ => None,
        };
        if let Some(scalar) = scalar {
            if !self.gnu_vector_profile() {
                return Err(Error::new(
                    offset,
                    "vector_size on a derived declarator is unsupported by this target's compiler profile; apply it to the scalar base",
                ));
            }
            *scalar = self.vector_type_at(scalar.clone(), bytes, offset, depth + 1)?;
            return Ok(resolved);
        }
        if matches!(resolved.kind, TypeKind::Enum(_)) {
            return Err(Error::new(
                offset,
                "GNU vectors with enum elements are unsupported",
            ));
        }
        if !matches!(resolved.kind, TypeKind::Integer(_) | TypeKind::Float(_)) {
            return Err(Error::new(
                offset,
                "vector_size requires an integer or floating scalar element type",
            ));
        }
        let qualifiers = resolved.qualifiers;
        resolved.qualifiers = Qualifiers::default();
        let element_bytes = self.unit.layout(&resolved)?.size_bytes();
        if bytes < element_bytes || !bytes.is_multiple_of(element_bytes) {
            return Err(Error::new(
                offset,
                "vector_size must be a multiple of the element size",
            ));
        }
        Ok(Type {
            kind: TypeKind::Vector {
                kind: crate::VectorKind::Gnu,
                element: Box::new(resolved),
                lanes: bytes / element_bytes,
            },
            qualifiers,
            alignment: crate::TypeAlignment::default(),
        })
    }

    pub(crate) fn gnu_vector_profile(&self) -> bool {
        self.unit.compiler == toucan_target::Compiler::Gnu
    }

    /// Vector casts reinterpret storage; they are not elementwise numeric conversions.
    pub(crate) fn check_vector_cast(
        &self,
        source: &Type,
        destination: &Type,
        offset: usize,
    ) -> Result<(), Error> {
        let allowed = |ty: &Type| matches!(ty.kind, TypeKind::Vector { .. } | TypeKind::Integer(_));
        if !allowed(source)
            || !allowed(destination)
            || self.unit.layout(source)?.size_bits != self.unit.layout(destination)?.size_bits
        {
            return Err(Error::new(
                offset,
                "vector casts require vector or integer operands with equal storage size",
            ));
        }
        Ok(())
    }

    /// Selects a vector operand type without applying scalar integer promotions.
    pub(crate) fn vector_operands(
        &mut self,
        left: &Type,
        right: &Type,
        lhs: &Node<ast::Expression>,
        rhs: &Node<ast::Expression>,
        shift: bool,
    ) -> Result<Type, Error> {
        match (&left.kind, &right.kind) {
            (TypeKind::Vector { .. }, TypeKind::Vector { .. }) => {
                if !self.compatible_vector_lanes(left, right)? {
                    return Err(Error::new(
                        lhs.span.start,
                        "operations between different vector element types are unsupported; use an explicit vector cast",
                    ));
                }
                let mut result = left.clone();
                if !self.gnu_vector_profile() && left.alignment != right.alignment {
                    result.alignment = crate::TypeAlignment::default();
                }
                Ok(result)
            }
            (TypeKind::Vector { element, .. }, _) => {
                if shift {
                    self.integer_type(right, rhs.span.start)?;
                } else {
                    self.vector_scalar(element, right, rhs)?;
                }
                Ok(left.clone())
            }
            (_, TypeKind::Vector { element, .. }) => {
                self.vector_scalar(element, left, lhs)?;
                Ok(right.clone())
            }
            _ => unreachable!("vector operation has a vector operand"),
        }
    }

    /// GNU allows lane-preserving value conversion between its ordinary vector
    /// extension and nominal NEON types; pointer/type compatibility stays strict.
    pub(crate) fn compatible_vector_lanes(&self, left: &Type, right: &Type) -> Result<bool, Error> {
        let (
            TypeKind::Vector {
                element: a,
                lanes: al,
                ..
            },
            TypeKind::Vector {
                element: b,
                lanes: bl,
                ..
            },
        ) = (&left.kind, &right.kind)
        else {
            return Ok(false);
        };
        Ok(al == bl && self.compatible(a, b)?)
    }

    fn vector_scalar(
        &mut self,
        element: &Type,
        scalar: &Type,
        expression: &Node<ast::Expression>,
    ) -> Result<(), Error> {
        let offset = expression.span.start;
        if self.compatible(element, scalar)? {
            return Ok(());
        }
        if let (Ok(to), Ok(from)) = (
            self.integer_type(element, offset),
            self.integer_type(scalar, offset),
        ) {
            let fits = (to.signed == from.signed && to.bits >= from.bits)
                || (to.signed && !from.signed && to.bits > from.bits);
            if fits {
                return Ok(());
            }
        }
        if let TypeKind::Float(to) = element.kind {
            let precision = self.vector_float_precision(to);
            if let TypeKind::Float(from) = scalar.kind {
                if self.vector_float_precision(from) <= precision {
                    return Ok(());
                }
            } else if let Ok(from) = self.integer_type(scalar, offset)
                && u32::from(from.bits) - u32::from(from.signed) <= precision
            {
                return Ok(());
            }
        }
        // Constant scalars may be broadcast when the conversion preserves their value.
        // Round through the target evaluator, never the host's floating representation.
        if matches!(element.kind, TypeKind::Integer(_) | TypeKind::Float(_))
            && self.is_arithmetic(scalar)?
            && let Ok(value) = self.eval_arithmetic(expression)
            && let Ok(converted) = self.convert_arithmetic(value, element, offset)
            && let Ok(roundtrip) = self.convert_arithmetic(converted, scalar, offset)
        {
            let same = match (value, converted, roundtrip) {
                (ArithmeticValue::Integer(a), ArithmeticValue::Integer(b), _) => {
                    if a.signed && a.signed_value() < 0 {
                        b.signed && b.signed_value() == a.signed_value()
                    } else {
                        (!b.signed || b.signed_value() >= 0) && a.value == b.value
                    }
                }
                (ArithmeticValue::Integer(a), _, ArithmeticValue::Integer(b)) => a == b,
                (
                    ArithmeticValue::Floating { value: a, .. },
                    _,
                    ArithmeticValue::Floating { value: b, .. },
                ) => a == b,
                _ => false,
            };
            // GCC does not admit floating-to-integer splats even when lossless.
            if same
                && !matches!(
                    (&element.kind, &scalar.kind),
                    (TypeKind::Integer(_), TypeKind::Float(_))
                )
            {
                return Ok(());
            }
        }
        Err(Error::new(
            offset,
            "scalar operand cannot be represented safely in the vector element type",
        ))
    }

    fn vector_float_precision(&self, kind: FloatKind) -> u32 {
        match kind {
            FloatKind::Float | FloatKind::FLOAT32 => 24,
            FloatKind::Double | FloatKind::FLOAT64 | FloatKind::FLOAT32X => 53,
            FloatKind::LongDouble | FloatKind::FLOAT64X => match self.unit.target {
                Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl => 113,
                Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::I686UnknownLinuxGnu
                | Target::X86_64AppleDarwin => 64,
                Target::Aarch64AppleDarwin
                | Target::Armv7UnknownLinuxGnueabihf
                | Target::X86_64PcWindowsMsvc
                | Target::Aarch64PcWindowsMsvc => 53,
            },
            FloatKind::FLOAT128 => 113,
            FloatKind::FLOAT16 => 11,
            FloatKind::BFloat16 => 8,
            FloatKind::Extended { .. } => 0,
        }
    }

    pub(crate) fn vector_mask(&self, vector: &Type, offset: usize) -> Result<Type, Error> {
        let TypeKind::Vector { element, lanes, .. } = &vector.kind else {
            unreachable!()
        };
        let bytes = self.unit.layout(element)?.size_bytes();
        let kind = match bytes {
            1 if self.gnu_vector_profile() => IntegerKind::SignedChar,
            1 => IntegerKind::Char,
            2 => IntegerKind::Short,
            4 => IntegerKind::Int,
            8 if self.gnu_vector_profile() && self.unit.target.long_width() == 64 => {
                IntegerKind::Long
            }
            8 => IntegerKind::LongLong,
            16 => IntegerKind::Int128,
            _ => {
                return Err(Error::new(
                    offset,
                    "unsupported vector comparison element width",
                ));
            }
        };
        Ok(Type::new(TypeKind::Vector {
            kind: crate::VectorKind::Gnu,
            element: Box::new(Type::new(TypeKind::Integer(kind))),
            lanes: *lanes,
        }))
    }
}

impl<'ast> Analyzer<'ast> {
    /// GNU shuffle masks select lanes modulo the concatenated input length.
    /// Every argument is evaluated once, with ordinary unspecified argument order.
    pub(crate) fn shuffle_call_type(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<Type, Error> {
        let offset = call.span.start;
        if !self.gnu_vector_profile() {
            return Err(Error::new(
                offset,
                "__builtin_shuffle requires a GNU compiler profile",
            ));
        }
        if !matches!(call.node.arguments.len(), 2 | 3) {
            return Err(Error::new(
                offset,
                "__builtin_shuffle requires one or two input vectors and a mask vector",
            ));
        }
        let mut types = Vec::with_capacity(call.node.arguments.len());
        for argument in &call.node.arguments {
            types.push(self.value_expression_type(argument)?);
        }
        let first = &types[0];
        let TypeKind::Vector { element, lanes, .. } = &first.kind else {
            return Err(Error::new(
                offset,
                "__builtin_shuffle input must be a vector",
            ));
        };
        if types.len() == 3 && !self.compatible(first, &types[1])? {
            return Err(Error::new(
                call.node.arguments[1].span.start,
                "__builtin_shuffle input vectors must have the same element type and lane count",
            ));
        }
        let mask = types.last().expect("shuffle mask");
        let valid_mask = if let TypeKind::Vector {
            element: mask_element,
            lanes: mask_lanes,
            ..
        } = &mask.kind
        {
            matches!(self.unit.resolve(mask_element)?.kind, TypeKind::Integer(_))
                && mask_lanes == lanes
                && self.unit.layout(mask_element)?.size_bits == self.unit.layout(element)?.size_bits
        } else {
            false
        };
        if !valid_mask {
            return Err(Error::new(
                call.node.arguments.last().expect("shuffle mask").span.start,
                "__builtin_shuffle mask must be an integer vector with the input's lane count and element size",
            ));
        }
        Ok(first.clone())
    }
}

/// Cached only for calls that use the shuffle intrinsic. Input expressions and
/// lane type operands are checked once while their lexical scopes are visible.
pub(crate) struct ShuffleVectorSignature {
    pub(crate) result: Type,
    pub(crate) indices: Option<Vec<crate::checked::ShuffleLane>>,
}

impl<'ast> Analyzer<'ast> {
    pub(crate) fn shuffle_vector_signature(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<&ShuffleVectorSignature, Error> {
        let key = (call.span.start, call.span.end);
        if !self.shuffle_vectors.contains_key(&key) {
            if self.shuffle_vectors.len() >= 65_536 {
                return Err(Error::new(
                    call.span.start,
                    "shuffle expression count exceeds the 65536-entry limit",
                ));
            }
            let signature = self.check_shuffle_vector(call)?;
            self.shuffle_vectors.insert(key, signature);
        }
        Ok(&self.shuffle_vectors[&key])
    }

    fn check_shuffle_vector(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<ShuffleVectorSignature, Error> {
        use crate::checked::ShuffleLane;
        let arguments = &call.node.arguments;
        let offset = call.span.start;
        if arguments.len() < 2 {
            return Err(Error::new(
                offset,
                "__builtin_shufflevector requires two vector operands",
            ));
        }
        let first = self.value_expression_type(&arguments[0])?;
        let second = self.value_expression_type(&arguments[1])?;
        let (
            TypeKind::Vector {
                element,
                lanes: first_lanes,
                ..
            },
            TypeKind::Vector {
                element: second_element,
                lanes: second_lanes,
                ..
            },
        ) = (&first.kind, &second.kind)
        else {
            return Err(Error::new(
                offset,
                "__builtin_shufflevector requires fixed-size vector operands",
            ));
        };
        for input in [&first, &second] {
            if self.unit.layout(input)?.size_bytes() > 16 {
                return Err(Error::new(
                    offset,
                    "vectors larger than 16 bytes require unsupported target-feature configuration",
                ));
            }
        }
        if arguments.len() == 2 {
            if self.gnu_vector_profile() {
                return Err(Error::new(
                    offset,
                    "GNU __builtin_shufflevector requires scalar lane indices",
                ));
            }
            if first_lanes != second_lanes
                || !matches!(
                    self.unit.resolve(second_element)?.kind,
                    TypeKind::Integer(_)
                )
            {
                return Err(Error::new(
                    arguments[1].span.start,
                    "shuffle mask must be an integer vector with the input's lane count",
                ));
            }
            return Ok(ShuffleVectorSignature {
                result: first,
                indices: None,
            });
        }
        if if self.gnu_vector_profile() {
            !self.same_type(element, second_element, 0)?
        } else {
            !self.same_type(&first, &second, 0)?
        } {
            return Err(Error::new(
                arguments[1].span.start,
                if self.gnu_vector_profile() {
                    "shuffle input vectors must have the same element type"
                } else {
                    "Clang shuffle input vectors must have the same type"
                },
            ));
        }
        let lanes = (arguments.len() - 2) as u64;
        if !lanes.is_power_of_two() {
            return Err(Error::new(
                offset,
                if self.gnu_vector_profile() {
                    "GNU shuffle results require a power-of-two lane count"
                } else {
                    "Clang shuffle results with non-power-of-two lane counts require unsupported padded vector layouts"
                },
            ));
        }
        let bytes = self
            .unit
            .layout(element)?
            .size_bytes()
            .checked_mul(lanes)
            .ok_or_else(|| Error::new(offset, "shuffle result size overflow"))?;
        let result = if !self.gnu_vector_profile() && lanes == *first_lanes {
            first.clone()
        } else {
            // GNU canonicalizes the vector type, including NEON identity and
            // typedef alignment. Clang preserves those only for unchanged width.
            self.vector_type((**element).clone(), bytes, offset)?
        };
        let input_lanes = first_lanes
            .checked_add(*second_lanes)
            .ok_or_else(|| Error::new(offset, "shuffle input lane count overflow"))?;
        let mut indices = Vec::with_capacity(arguments.len() - 2);
        for index in &arguments[2..] {
            let checkpoint = self.sve_feature_checkpoint();
            if !self.is_integer_constant_expression(index, 0)? {
                if self.gnu_vector_profile()
                    && matches!(
                        self.value_expression_type(index)?.kind,
                        TypeKind::Pointer(_)
                    )
                {
                    return Err(Error::new(
                        index.span.start,
                        "GNU pointer-valued shuffle lane indices are unsupported",
                    ));
                }
                return Err(Error::new(
                    index.span.start,
                    "shuffle lane index must be an integer constant expression",
                ));
            }
            let value = self.eval(index)?;
            self.discard_sve_feature_uses(checkpoint);
            let lane = if value.signed && value.signed_value() == -1 {
                ShuffleLane::Undefined
            } else if (value.signed && value.signed_value() < 0)
                || value.value >= u128::from(input_lanes)
            {
                return Err(Error::new(
                    index.span.start,
                    "shuffle lane index is outside the concatenated input vectors",
                ));
            } else {
                ShuffleLane::Index(value.value as u64)
            };
            indices.push(lane);
        }
        Ok(ShuffleVectorSignature {
            result,
            indices: Some(indices),
        })
    }
}

//! Compiler intrinsics used by C standard headers and inline functions.

use lang_c::{ast, span::Node};

use crate::analyze::Analyzer;
use crate::integer::integer_to_type;
use crate::{Error, FloatKind, IntegerKind, IntegerValue, Type, TypeKind};

pub(crate) struct MemorySignature {
    pub(crate) result: Type,
    pub(crate) parameters: [Type; 3],
}

impl Analyzer {
    /// Infinity and huge-value intrinsics share the target's three C float types.
    pub(crate) fn infinity_builtin_kind(&self, name: &str) -> Option<FloatKind> {
        Some(match name {
            "__builtin_inff" | "__builtin_huge_valf" => FloatKind::Float,
            "__builtin_inf" | "__builtin_huge_val" => FloatKind::Double,
            "__builtin_infl" | "__builtin_huge_vall" => FloatKind::LongDouble,
            _ => return None,
        })
    }

    /// NaN constructors take a string payload and return the selected C format.
    pub(crate) fn nan_builtin(&self, name: &str) -> Option<(FloatKind, bool)> {
        Some(match name {
            "__builtin_nanf" => (FloatKind::Float, false),
            "__builtin_nan" => (FloatKind::Double, false),
            "__builtin_nanl" => (FloatKind::LongDouble, false),
            "__builtin_nansf" => (FloatKind::Float, true),
            "__builtin_nans" => (FloatKind::Double, true),
            "__builtin_nansl" => (FloatKind::LongDouble, true),
            _ => return None,
        })
    }

    /// Keep the payload conversion shared by type checking and retained uses.
    pub(crate) fn nan_parameter_type(&self) -> Type {
        let mut character = Type::new(TypeKind::Integer(IntegerKind::Char));
        character.qualifiers.is_const = true;
        character.pointer()
    }

    /// Byte-swap prototypes use the compiler target's exact-width unsigned types.
    pub(crate) fn byte_swap_type(&self, name: &str) -> Option<Type> {
        let kind = match name {
            "__builtin_bswap16" => IntegerKind::UnsignedShort,
            "__builtin_bswap32" => IntegerKind::UnsignedInt,
            "__builtin_bswap64" => match self.unit.target {
                toucan_target::Target::X86_64UnknownLinuxGnu
                | toucan_target::Target::Aarch64UnknownLinuxGnu => IntegerKind::UnsignedLong,
                _ => IntegerKind::UnsignedLongLong,
            },
            _ => return None,
        };
        Some(Type::new(TypeKind::Integer(kind)))
    }

    /// Fixed-width bit-count builtins convert to their declared unsigned C parameter.
    /// The canonical long type supplies the target-dependent width in both checking
    /// and retained argument conversions.
    pub(crate) fn bit_count_type(&self, name: &str) -> Option<Type> {
        let kind = match name {
            "__builtin_clz" | "__builtin_ctz" => IntegerKind::UnsignedInt,
            "__builtin_clzl" | "__builtin_ctzl" => IntegerKind::UnsignedLong,
            "__builtin_clzll" | "__builtin_ctzll" => IntegerKind::UnsignedLongLong,
            _ => return None,
        };
        Some(Type::new(TypeKind::Integer(kind)))
    }

    /// Library builtins use the target's ordinary C parameter conversions.
    /// Keep this signature shared with retained argument-use construction.
    pub(crate) fn memory_builtin_signature(&self, name: &str) -> Option<MemorySignature> {
        if !matches!(
            name,
            "__builtin_memset" | "__builtin_memcpy" | "__builtin_memmove" | "__builtin_memcmp"
        ) {
            return None;
        }
        let pointer = Type::new(TypeKind::Void).pointer();
        let mut constant = Type::new(TypeKind::Void);
        constant.qualifiers.is_const = true;
        let constant = constant.pointer();
        let int = Type::new(TypeKind::Integer(IntegerKind::Int));
        Some(MemorySignature {
            result: if name == "__builtin_memcmp" {
                int.clone()
            } else {
                pointer.clone()
            },
            parameters: [
                if name == "__builtin_memcmp" {
                    constant.clone()
                } else {
                    pointer
                },
                if name == "__builtin_memset" {
                    int
                } else {
                    constant
                },
                integer_to_type(self.size_value(0)),
            ],
        })
    }

    /// Recognizes intrinsics only when an ordinary declaration has not shadowed
    /// their names. Intrinsics never become exported external declarations.
    pub(crate) fn builtin_name<'a>(&self, call: &'a Node<ast::CallExpression>) -> Option<&'a str> {
        let ast::Expression::Identifier(identifier) = &call.node.callee.node else {
            return None;
        };
        let name = identifier.node.name.as_str();
        if self
            .lexical_scopes
            .iter()
            .any(|scope| scope.names.contains_key(name))
            || self
                .unit
                .declarations
                .iter()
                .any(|declaration| declaration.name == name)
            || self.unit.constants.contains_key(name)
        {
            return None;
        }
        Some(name)
    }

    pub(crate) fn builtin_call_type(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<Option<Type>, Error> {
        let Some(name) = self.builtin_name(call) else {
            return Ok(None);
        };
        if name == "__builtin_shuffle" {
            return self.shuffle_call_type(call).map(Some);
        }
        if let Some(intrinsic) = crate::x86::X86Intrinsic::from_name(name) {
            return self.x86_call_type(intrinsic, call).map(Some);
        }
        if let Some(intrinsic) = crate::overflow::OverflowIntrinsic::from_name(name) {
            return self.overflow_call_type(intrinsic, call).map(Some);
        }
        if let Some(operation) = crate::c11_atomic::C11AtomicOperation::from_name(name) {
            return self.c11_atomic_call_type(operation, call).map(Some);
        }
        if let Some(operation) = crate::atomic::AtomicOperation::from_name(name) {
            return self.atomic_call_type(operation, call).map(Some);
        }
        if let Some(operation) = crate::sync::SyncOperation::from_name(name) {
            return self.sync_call_type(operation, call).map(Some);
        }
        if name.rsplit_once('_').is_some_and(|(base, suffix)| {
            crate::sync::SyncOperation::from_name(base).is_some()
                && !suffix.is_empty()
                && suffix.bytes().all(|byte| byte.is_ascii_digit())
        }) {
            return Err(Error::new(
                call.span.start,
                "size-suffixed __sync intrinsic aliases are unsupported",
            ));
        }
        if let Some(signature) = self.fortified_signature(name, call.span.start)? {
            let arguments = &call.node.arguments;
            if arguments.len() < signature.parameters.len()
                || (!signature.variadic && arguments.len() != signature.parameters.len())
            {
                return Err(Error::new(
                    call.span.start,
                    "argument count does not match fortified intrinsic prototype",
                ));
            }
            for (index, argument) in arguments.iter().enumerate() {
                if let Some(parameter) = signature.parameters.get(index) {
                    self.check_assignment(parameter, argument)?;
                } else {
                    let ty = self.value_expression_type(argument)?;
                    self.require_complete_object(&ty, argument.span.start)?;
                }
                self.check_argument_pack(
                    argument,
                    index,
                    arguments.len(),
                    signature.parameters.len(),
                    signature.variadic,
                )?;
            }
            return Ok(Some(signature.result));
        }
        if matches!(name, "__builtin_va_arg_pack" | "__builtin_va_arg_pack_len") {
            if !matches!(
                self.unit.target,
                toucan_target::Target::X86_64UnknownLinuxGnu
                    | toucan_target::Target::Aarch64UnknownLinuxGnu
            ) {
                return Err(Error::new(
                    call.span.start,
                    "variadic argument packs require a GNU target profile",
                ));
            }
            if !call.node.arguments.is_empty() {
                return Err(Error::new(
                    call.span.start,
                    format!("{name} requires zero arguments"),
                ));
            }
            self.has_variadic_packs |= name == "__builtin_va_arg_pack";
            return Ok(Some(Type::new(TypeKind::Integer(IntegerKind::Int))));
        }
        let memory = self.memory_builtin_signature(name);
        let byte_swap = self.byte_swap_type(name);
        let infinity = self.infinity_builtin_kind(name);
        let nan = self.nan_builtin(name);
        let bit_count = self.bit_count_type(name);
        let object_size = self.object_size_signature(name);
        let arity = match name {
            "__builtin_va_start" | "__builtin_va_copy" | "__builtin_expect" => 2,
            "__builtin_va_end" | "__builtin_constant_p" => 1,
            "__builtin_unreachable" | "__builtin_trap" => 0,
            _ if memory.is_some() => 3,
            _ if byte_swap.is_some() || bit_count.is_some() => 1,
            _ if object_size.is_some() => 2,
            _ if infinity.is_some() => 0,
            _ if nan.is_some() => 1,
            _ => return Ok(None),
        };
        let arguments = &call.node.arguments;
        let offset = call.span.start;
        if arguments.len() != arity {
            return Err(Error::new(
                offset,
                format!("{name} requires {arity} arguments"),
            ));
        }
        if let Some(kind) = infinity {
            return Ok(Some(Type::new(TypeKind::Float(kind))));
        }
        if let Some((kind, _)) = nan {
            self.check_assignment(&self.nan_parameter_type(), &arguments[0])?;
            return Ok(Some(Type::new(TypeKind::Float(kind))));
        }
        if let Some(signature) = memory {
            for (argument, parameter) in arguments.iter().zip(&signature.parameters) {
                self.check_assignment(parameter, argument)?;
            }
            return Ok(Some(signature.result));
        }
        if let Some(ty) = byte_swap {
            self.check_assignment(&ty, &arguments[0])?;
            return Ok(Some(ty));
        }
        if let Some(parameter) = bit_count {
            self.check_assignment(&parameter, &arguments[0])?;
            return Ok(Some(Type::new(TypeKind::Integer(IntegerKind::Int))));
        }
        if let Some(signature) = object_size {
            let checkpoint = self.sve_feature_checkpoint();
            for (argument, parameter) in arguments.iter().zip(&signature.parameters) {
                self.check_assignment(parameter, argument)?;
            }
            self.check_object_size_mode(&arguments[1], &signature.parameters[1])?;
            if self.gnu_vector_profile() {
                self.discard_sve_feature_uses(checkpoint);
            }
            return Ok(Some(signature.result));
        }
        match name {
            "__builtin_constant_p" => {
                let checkpoint = self.sve_feature_checkpoint();
                let ty = self.value_expression_type(&arguments[0])?;
                // Clang can evaluate fresh VLA bounds in numeric queries. A
                // nonnumeric operand cannot reach that fallback; GNU suppresses
                // all query operands. Retain uncertain Clang obligations.
                if self.gnu_vector_profile() || !self.is_arithmetic(&ty)? {
                    self.discard_sve_feature_uses(checkpoint);
                }
                if matches!(
                    self.unit.target,
                    toucan_target::Target::X86_64UnknownLinuxGnu
                        | toucan_target::Target::Aarch64UnknownLinuxGnu
                ) {
                    self.require_definite_object(&ty, arguments[0].span.start)?;
                }
                return Ok(Some(Type::new(TypeKind::Integer(IntegerKind::Int))));
            }
            "__builtin_expect" => {
                let ty = Type::new(TypeKind::Integer(IntegerKind::Long));
                for argument in arguments {
                    self.check_assignment(&ty, argument)?;
                }
                return Ok(Some(ty));
            }
            "__builtin_va_start" => {
                let Some(function) = self.current_function_signature() else {
                    return Err(Error::new(
                        offset,
                        "va_start requires a variadic function body",
                    ));
                };
                if function.calling_convention.for_target(self.unit.target)?
                    != crate::CallingConvention::C
                {
                    return Err(Error::new(
                        offset,
                        "va_start with a nondefault variadic ABI is unsupported",
                    ));
                }
                if !function.variadic {
                    return Err(Error::new(
                        offset,
                        "va_start requires a variadic function body",
                    ));
                }
                let ast::Expression::Identifier(identifier) = &arguments[1].node else {
                    return Err(Error::new(
                        arguments[1].span.start,
                        "va_start requires the last named parameter",
                    ));
                };
                let name = &identifier.node.name;
                if function
                    .parameters
                    .last()
                    .and_then(|parameter| parameter.name.as_ref())
                    != Some(name)
                    || self.current_function_parameter(name).is_none()
                {
                    return Err(Error::new(
                        arguments[1].span.start,
                        "va_start requires the last named parameter",
                    ));
                }
                self.check_va_list(&arguments[0], false)?;
            }
            "__builtin_va_copy" => {
                self.check_va_list(&arguments[0], false)?;
                self.check_va_list(&arguments[1], false)?;
            }
            "__builtin_va_end" => self.check_va_list(&arguments[0], false)?,
            _ => {}
        }
        Ok(Some(Type::new(TypeKind::Void)))
    }

    /// Array va_list ABIs pass a pointer to the first state record. Other ABIs
    /// mutate the list object itself, requiring a modifiable lvalue.
    fn check_va_list(
        &mut self,
        expression: &Node<ast::Expression>,
        exact: bool,
    ) -> Result<(), Error> {
        let offset = expression.span.start;
        let list = self
            .unit
            .typedefs
            .get("__builtin_va_list")
            .cloned()
            .ok_or_else(|| Error::new(offset, "target has no builtin va_list type"))?;
        if matches!(self.unit.resolve(&list)?.kind, TypeKind::Array { .. }) && !exact {
            return self.check_assignment(&self.value_type(&list)?, expression);
        }
        let info = self.expression_info(expression)?;
        let actual = self.converted_type(&info, offset)?;
        if !self.compatible(&actual, &self.value_type(&list)?)? {
            return Err(Error::new(
                offset,
                "argument must have the target's va_list type",
            ));
        }
        if !matches!(self.unit.resolve(&list)?.kind, TypeKind::Array { .. }) {
            self.require_modifiable(&info, offset)?;
        }
        Ok(())
    }

    pub(crate) fn va_arg_type(
        &mut self,
        argument: &Node<ast::VaArgExpression>,
    ) -> Result<Type, Error> {
        self.check_va_list(&argument.node.va_list, true)?;
        let ty = self.type_name(&argument.node.type_name.node)?;
        self.require_complete_object(&ty, argument.span.start)?;
        Ok(ty)
    }

    /// The prediction hint preserves the first argument after conversion to long.
    /// Constant evaluation requires both arguments to be arithmetic constants.
    pub(crate) fn eval_expect(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<IntegerValue, Error> {
        let ty = self
            .builtin_call_type(call)?
            .ok_or_else(|| Error::new(call.span.start, "expected builtin prediction hint"))?;
        let first = self.eval_arithmetic(&call.node.arguments[0])?;
        let second = self.eval_arithmetic(&call.node.arguments[1])?;
        self.convert_arithmetic(second, &ty, call.span.start)?;
        self.convert_arithmetic(first, &ty, call.span.start)?
            .integer(call.span.start)
    }

    /// Counts only after parameter conversion. GCC leaves zero undefined, including
    /// a nonzero wider value that becomes zero when converted to the parameter.
    pub(crate) fn eval_bit_count(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<IntegerValue, Error> {
        let offset = call.span.start;
        let result = self
            .builtin_call_type(call)?
            .ok_or_else(|| Error::new(offset, "expected builtin bit count"))?;
        let name = self
            .builtin_name(call)
            .ok_or_else(|| Error::new(offset, "expected builtin bit count"))?;
        let parameter = self
            .bit_count_type(name)
            .ok_or_else(|| Error::new(offset, "expected builtin bit count"))?;
        let value = self.eval_arithmetic(&call.node.arguments[0])?;
        let value = self
            .convert_arithmetic(value, &parameter, offset)?
            .integer(offset)?;
        if value.value == 0 {
            return Err(Error::new(
                offset,
                format!("{name} has an undefined result for zero"),
            ));
        }
        let count = if name.starts_with("__builtin_clz") {
            value.value.leading_zeros() - (128 - u32::from(value.bits))
        } else {
            value.value.trailing_zeros()
        };
        let result = self.integer_type(&result, offset)?;
        Ok(IntegerValue::new(
            u128::from(count),
            result.bits,
            result.signed,
            result.rank,
        ))
    }

    /// Converts the input before swapping exactly the prototype's number of bytes.
    pub(crate) fn eval_byte_swap(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<IntegerValue, Error> {
        let offset = call.span.start;
        let ty = self
            .builtin_call_type(call)?
            .ok_or_else(|| Error::new(offset, "expected builtin byte swap"))?;
        let value = self.eval_arithmetic(&call.node.arguments[0])?;
        let value = self
            .convert_arithmetic(value, &ty, offset)?
            .integer(offset)?;
        Ok(IntegerValue::new(
            value.value.swap_bytes() >> (128 - value.bits),
            value.bits,
            false,
            value.rank,
        ))
    }
}

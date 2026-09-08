//! Source-level GNU atomic operations. This layer checks C operands; instruction
//! selection, lock-free implementation and the memory-model lowering remain backend work.

use lang_c::{ast, span::Node};
use serde::Serialize;
use toucan_target::Target;

use crate::analyze::Analyzer;
use crate::checked::{Conversion, UseContext};
use crate::{Error, FloatKind, IntegerKind, IntegerValue, Type, TypeKind};

/// GNU atomic operations, including Clang's floating add/sub extension.
/// Pointer arithmetic uses byte offsets, without scaling by the pointee size.
/// Generic forms access the size of the first pointed object; test-and-set and
/// clear access one byte regardless of their pointer's original type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum AtomicOperation {
    Load,
    LoadGeneric,
    Store,
    StoreGeneric,
    Exchange,
    ExchangeGeneric,
    CompareExchange,
    CompareExchangeGeneric,
    FetchAdd,
    FetchSub,
    FetchAnd,
    FetchOr,
    FetchXor,
    FetchNand,
    AddFetch,
    SubFetch,
    AndFetch,
    OrFetch,
    XorFetch,
    NandFetch,
    TestAndSet,
    Clear,
    ThreadFence,
    SignalFence,
    AlwaysLockFree,
    IsLockFree,
}

impl AtomicOperation {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "__atomic_load_n" => Self::Load,
            "__atomic_load" => Self::LoadGeneric,
            "__atomic_store_n" => Self::Store,
            "__atomic_store" => Self::StoreGeneric,
            "__atomic_exchange_n" => Self::Exchange,
            "__atomic_exchange" => Self::ExchangeGeneric,
            "__atomic_compare_exchange_n" => Self::CompareExchange,
            "__atomic_compare_exchange" => Self::CompareExchangeGeneric,
            "__atomic_fetch_add" => Self::FetchAdd,
            "__atomic_fetch_sub" => Self::FetchSub,
            "__atomic_fetch_and" => Self::FetchAnd,
            "__atomic_fetch_or" => Self::FetchOr,
            "__atomic_fetch_xor" => Self::FetchXor,
            "__atomic_fetch_nand" => Self::FetchNand,
            "__atomic_add_fetch" => Self::AddFetch,
            "__atomic_sub_fetch" => Self::SubFetch,
            "__atomic_and_fetch" => Self::AndFetch,
            "__atomic_or_fetch" => Self::OrFetch,
            "__atomic_xor_fetch" => Self::XorFetch,
            "__atomic_nand_fetch" => Self::NandFetch,
            "__atomic_test_and_set" => Self::TestAndSet,
            "__atomic_clear" => Self::Clear,
            "__atomic_thread_fence" => Self::ThreadFence,
            "__atomic_signal_fence" => Self::SignalFence,
            "__atomic_always_lock_free" => Self::AlwaysLockFree,
            "__atomic_is_lock_free" => Self::IsLockFree,
            _ => return None,
        })
    }

    /// Argument positions of the success/ordinary order and optional failure order.
    /// Their retained uses include conversion to C `int`. Nonconstant orders are
    /// runtime inputs; a backend must apply its compiler's dynamic-order policy.
    pub fn memory_order_arguments(self) -> Option<(usize, Option<usize>)> {
        Some(match self {
            Self::AlwaysLockFree | Self::IsLockFree => return None,
            Self::ThreadFence | Self::SignalFence => (0, None),
            Self::Load | Self::TestAndSet | Self::Clear => (1, None),
            Self::ExchangeGeneric => (3, None),
            Self::CompareExchange | Self::CompareExchangeGeneric => (4, Some(5)),
            _ => (2, None),
        })
    }

    /// Position of the Boolean weak/spurious-failure request, when present.
    pub fn weak_argument(self) -> Option<usize> {
        matches!(self, Self::CompareExchange | Self::CompareExchangeGeneric).then_some(3)
    }

    /// Queries do not access the pointed object. `AlwaysLockFree` suppresses
    /// both operand evaluations; `IsLockFree` evaluates its arguments normally.
    pub fn is_lock_free_query(self) -> bool {
        matches!(self, Self::AlwaysLockFree | Self::IsLockFree)
    }

    fn count(self) -> usize {
        self.memory_order_arguments()
            .map_or(2, |(order, failure)| failure.unwrap_or(order) + 1)
    }
    fn generic(self) -> bool {
        matches!(
            self,
            Self::LoadGeneric
                | Self::StoreGeneric
                | Self::ExchangeGeneric
                | Self::CompareExchangeGeneric
        )
    }
    fn add_sub(self) -> bool {
        matches!(
            self,
            Self::FetchAdd | Self::FetchSub | Self::AddFetch | Self::SubFetch
        )
    }
    fn rmw(self) -> bool {
        matches!(
            self,
            Self::FetchAdd
                | Self::FetchSub
                | Self::FetchAnd
                | Self::FetchOr
                | Self::FetchXor
                | Self::FetchNand
                | Self::AddFetch
                | Self::SubFetch
                | Self::AndFetch
                | Self::OrFetch
                | Self::XorFetch
                | Self::NandFetch
        )
    }
}

pub(crate) struct AtomicSignature {
    pub(crate) result: Type,
    pub(crate) parameters: [Option<Type>; 6],
    sources: [Option<Type>; 6],
    pub(crate) context: UseContext,
    pub(crate) conversions: [Conversion; 6],
}

impl Analyzer {
    /// Computes the same overloaded source signature for checking and retention.
    pub(crate) fn atomic_signature(
        &mut self,
        op: AtomicOperation,
        call: &Node<ast::CallExpression>,
    ) -> Result<AtomicSignature, Error> {
        use AtomicOperation as A;
        let offset = call.span.start;
        if call.node.arguments.len() != op.count() {
            return Err(Error::new(
                offset,
                "argument count does not match __atomic intrinsic",
            ));
        }
        let integer = Type::new(TypeKind::Integer(IntegerKind::Int));
        let mut signature = AtomicSignature {
            result: Type::new(TypeKind::Void),
            parameters: std::array::from_fn(|_| None),
            sources: std::array::from_fn(|_| None),
            context: UseContext::Value,
            conversions: [Conversion::Assignment; 6],
        };
        if op.is_lock_free_query() {
            signature.result = Type::new(TypeKind::Bool);
            signature.parameters[0] = Some(crate::integer::integer_to_type(self.size_value(0)));
            let mut pointee = Type::new(TypeKind::Void);
            pointee.qualifiers.is_const = true;
            pointee.qualifiers.is_volatile = true;
            signature.parameters[1] = Some(pointee.pointer());
            if op == A::AlwaysLockFree {
                signature.context = UseContext::UnevaluatedValue;
            }
            return Ok(signature);
        }
        if let Some((order, failure)) = op.memory_order_arguments() {
            signature.parameters[order] = Some(integer.clone());
            if let Some(failure) = failure {
                signature.parameters[failure] = Some(integer);
            }
        }
        if matches!(op, A::ThreadFence | A::SignalFence) {
            return Ok(signature);
        }
        if op.weak_argument().is_some() {
            signature.parameters[3] = Some(Type::new(TypeKind::Bool));
        }
        let address = self.value_expression_type(&call.node.arguments[0])?;
        let TypeKind::Pointer(pointee) = &address.kind else {
            return Err(Error::new(offset, "__atomic address must be a pointer"));
        };
        let read_only = matches!(op, A::Load | A::LoadGeneric);
        let qualifiers = self.unit.qualifiers(pointee)?;
        if !read_only && qualifiers.is_const && !self.gnu_sync_profile() {
            return Err(Error::new(
                offset,
                "__atomic operation cannot modify a const-qualified object",
            ));
        }
        signature.sources[0] = Some(address.clone());
        signature.parameters[0] = Some(if !read_only && qualifiers.is_const {
            let mut pointee = self.unqualified(pointee)?;
            pointee.qualifiers = qualifiers;
            pointee.qualifiers.is_const = false;
            signature.conversions[0] = Conversion::IntrinsicArgument;
            pointee.pointer()
        } else {
            address.clone()
        });
        if matches!(op, A::TestAndSet | A::Clear) {
            if matches!(self.unit.resolve(pointee)?.kind, TypeKind::Function(_)) {
                return Err(Error::new(
                    offset,
                    "atomic flag address must point to an object or void",
                ));
            }
            let mut byte = Type::new(TypeKind::Void);
            byte.qualifiers.is_volatile = true;
            signature.parameters[0] = Some(byte.pointer());
            if op == A::TestAndSet {
                signature.result = Type::new(TypeKind::Bool);
            }
            return Ok(signature);
        }
        let mut value = self.unqualified(pointee)?;
        self.require_complete_object(&value, offset)?;
        let size = self.unit.layout(&value)?.size_bytes();
        if size == 0 {
            return Err(Error::new(
                offset,
                "__atomic address must point to a nonzero-sized object",
            ));
        }
        if !op.generic() {
            let valid = match value.kind {
                TypeKind::Integer(_) | TypeKind::Enum(_) => true,
                TypeKind::Bool => !op.rmw() || !self.gnu_sync_profile(),
                TypeKind::Pointer(_) => !op.rmw() || op.add_sub() || self.gnu_sync_profile(),
                TypeKind::Float(FloatKind::Float | FloatKind::Double) => {
                    op.add_sub() && !self.gnu_sync_profile()
                }
                _ => false,
            };
            if !valid || !matches!(size, 1 | 2 | 4 | 8 | 16) {
                return Err(Error::new(
                    offset,
                    "unsupported __atomic operand type for this operation and compiler profile",
                ));
            }
        }
        if self.gnu_sync_profile() {
            value.alignment = None;
        }
        signature.result = match op {
            A::Store | A::LoadGeneric | A::StoreGeneric | A::ExchangeGeneric => {
                Type::new(TypeKind::Void)
            }
            A::CompareExchange | A::CompareExchangeGeneric => Type::new(TypeKind::Bool),
            _ => value.clone(),
        };
        let value_index = match op {
            A::Store | A::Exchange => Some(1),
            A::CompareExchange => Some(2),
            _ if op.rmw() => Some(1),
            _ => None,
        };
        if let Some(index) = value_index {
            signature.parameters[index] = Some(
                if op.add_sub()
                    && matches!(value.kind, TypeKind::Pointer(_))
                    && !self.gnu_sync_profile()
                {
                    Type::new(TypeKind::Integer(
                        if self.unit.target == Target::X86_64PcWindowsMsvc {
                            IntegerKind::LongLong
                        } else {
                            IntegerKind::Long
                        },
                    ))
                } else {
                    value.clone()
                },
            );
            if self.gnu_sync_profile() {
                signature.conversions[index] = Conversion::IntrinsicArgument;
            }
        }
        let pointer_count = match op {
            A::LoadGeneric | A::StoreGeneric | A::CompareExchange => 1,
            A::ExchangeGeneric | A::CompareExchangeGeneric => 2,
            _ => 0,
        };
        for index in 1..=pointer_count {
            let argument = &call.node.arguments[index];
            let source = self.value_expression_type(argument)?;
            let TypeKind::Pointer(other) = &source.kind else {
                return Err(Error::new(
                    argument.span.start,
                    "__atomic buffer operand must be a pointer",
                ));
            };
            let qualifiers = self.unit.qualifiers(other)?;
            let input = matches!(op, A::StoreGeneric)
                || matches!(op, A::ExchangeGeneric) && index == 1
                || matches!(op, A::CompareExchangeGeneric) && index == 2;
            if !self.gnu_sync_profile() && (qualifiers.is_volatile || qualifiers.is_const) {
                return Err(Error::new(
                    argument.span.start,
                    "__atomic buffer has incompatible const or volatile qualifiers",
                ));
            }
            if self.gnu_sync_profile() {
                if op.generic() {
                    self.require_complete_object(other, argument.span.start)?;
                    if self.unit.layout(other)?.size_bytes() != size {
                        return Err(Error::new(
                            argument.span.start,
                            "generic __atomic buffers must have equal object sizes",
                        ));
                    }
                }
                signature.conversions[index] = Conversion::IntrinsicArgument;
            }
            let mut destination = value.clone();
            if input && self.gnu_sync_profile() {
                destination.qualifiers.is_const = true;
            }
            signature.parameters[index] = Some(destination.pointer());
            signature.sources[index] = Some(source);
        }
        Ok(signature)
    }

    pub(crate) fn atomic_call_type(
        &mut self,
        op: AtomicOperation,
        call: &Node<ast::CallExpression>,
    ) -> Result<Type, Error> {
        let key = (call.span.start, call.span.end);
        if op.is_lock_free_query() && self.checked_atomic_queries.contains(&key) {
            return Ok(Type::new(TypeKind::Bool));
        }
        let signature = self.atomic_signature(op, call)?;
        for (index, argument) in call.node.arguments.iter().enumerate() {
            let destination = signature.parameters[index]
                .as_ref()
                .expect("atomic argument signature");
            let source = if let Some(source) = &signature.sources[index] {
                source.clone()
            } else {
                self.value_expression_type(argument)?
            };
            if signature.conversions[index] == Conversion::IntrinsicArgument {
                if !matches!(source.kind, TypeKind::Pointer(_)) && !self.is_arithmetic(&source)? {
                    return Err(Error::new(
                        argument.span.start,
                        "incompatible __atomic value operand",
                    ));
                }
                if matches!(destination.kind, TypeKind::Pointer(_))
                    && matches!(source.kind, TypeKind::Float(_))
                {
                    return Err(Error::new(
                        argument.span.start,
                        "floating value cannot convert to an atomic pointer",
                    ));
                }
            } else {
                self.check_assignment_type(destination, &source, argument)?;
                if self.checked.is_some() {
                    self.retain_assignment(argument, destination)?;
                }
            }
        }
        if op == AtomicOperation::AlwaysLockFree {
            self.atomic_query_size(call).map_err(|_| {
                Error::new(
                    call.span.start,
                    "__atomic_always_lock_free requires a constant size",
                )
            })?;
        }
        self.check_atomic_orders(op, call)?;
        if op.is_lock_free_query() {
            if self.checked_atomic_queries.len() >= 65_536 {
                return Err(Error::new(
                    call.span.start,
                    "atomic query count exceeds the 65536-entry limit",
                ));
            }
            self.checked_atomic_queries.insert(key);
        }
        Ok(signature.result)
    }

    fn check_atomic_orders(
        &mut self,
        op: AtomicOperation,
        call: &Node<ast::CallExpression>,
    ) -> Result<(), Error> {
        use AtomicOperation as A;
        let Some((success, failure)) = op.memory_order_arguments() else {
            return Ok(());
        };
        let mut known = [None; 2];
        for (slot, index) in [Some(success), failure].into_iter().enumerate() {
            let Some(index) = index else {
                continue;
            };
            let Ok(value) = self.eval_arithmetic(&call.node.arguments[index]) else {
                continue;
            };
            let value = self
                .convert_arithmetic(
                    value,
                    &Type::new(TypeKind::Integer(IntegerKind::Int)),
                    call.span.start,
                )?
                .integer(call.span.start)?;
            let raw = value
                .as_u64()
                .map_err(|_| Error::new(call.span.start, "invalid atomic memory order"))?;
            let order = raw & 0xffff;
            let modifiers = raw & !0xffff;
            if order > 5 || modifiers != 0 {
                return Err(Error::new(
                    call.span.start,
                    if modifiers != 0 {
                        "target-specific atomic memory-order modifiers are unsupported"
                    } else {
                        "invalid atomic memory order"
                    },
                ));
            }
            let valid = if slot == 1 || matches!(op, A::Load | A::LoadGeneric) {
                matches!(order, 0 | 1 | 2 | 5)
            } else if matches!(op, A::Store | A::StoreGeneric | A::Clear) {
                matches!(order, 0 | 3 | 5)
            } else {
                true
            };
            if !valid {
                return Err(Error::new(
                    call.span.start,
                    "atomic memory order is invalid for this operation",
                ));
            }
            known[slot] = Some(order);
        }
        if let [Some(success), Some(failure)] = known {
            let valid = match success {
                0 | 3 => failure == 0,
                1 => failure <= 1,
                2 | 4 => failure <= 2,
                _ => true,
            };
            if !valid {
                return Err(Error::new(
                    call.span.start,
                    "atomic failure order is stronger than the success order",
                ));
            }
        }
        Ok(())
    }

    fn atomic_query_size(&mut self, call: &Node<ast::CallExpression>) -> Result<u64, Error> {
        let value = self.eval_arithmetic(&call.node.arguments[0])?;
        let ty = crate::integer::integer_to_type(self.size_value(0));
        self.convert_arithmetic(value, &ty, call.span.start)?
            .integer(call.span.start)?
            .as_u64()
    }

    /// Fold only guaranteed cases. Unknown CPU features, compiler alignment
    /// reasoning and runtime lock-free support never become a fabricated false.
    pub(crate) fn eval_atomic_lock_free(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<IntegerValue, Error> {
        let name = self.builtin_name(call).unwrap_or("");
        let op = AtomicOperation::from_name(name).expect("atomic query");
        self.atomic_call_type(op, call)?;
        let size = self.atomic_query_size(call)?;
        let address = &call.node.arguments[1];
        let ty = self.value_expression_type(address)?;
        let null = self.is_null_pointer_constant(address, &ty)?;
        if (op == AtomicOperation::AlwaysLockFree || null)
            && (size == 0 || !size.is_power_of_two() || size > 16)
        {
            return Ok(IntegerValue::new(0, 8, false, 0));
        }
        if matches!(size, 1 | 2 | 4 | 8) && null {
            return Ok(IntegerValue::new(1, 8, false, 0));
        }
        Err(Error::new(
            call.span.start,
            "atomic lock-free query depends on unproven target features or address alignment",
        ))
    }
}

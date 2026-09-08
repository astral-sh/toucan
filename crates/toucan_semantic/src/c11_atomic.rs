//! Clang's C11 atomic interface, kept separate from GNU's byte-offset intrinsics.

use lang_c::{ast, span::Node};
use serde::Serialize;
use toucan_target::Target;

use crate::analyze::Analyzer;
use crate::atomic::AtomicSignature;
use crate::checked::{Conversion, UseContext};
use crate::{Error, FloatKind, IntegerKind, IntegerValue, Type, TypeKind};

/// Clang C11 atomic operations, available in the Clang target profiles.
///
/// `Init` initializes storage without an atomic store or a memory order. Loads,
/// stores, exchanges and compare-exchanges operate on the complete atomic value,
/// including records. Compare-exchange writes the ordinary expected buffer on
/// failure. All fetch operations return the old value; pointer add/sub operands
/// count pointee elements, including a retained VLA extent, rather than bytes.
/// Fences have no object operand. A lock-free query evaluates its size argument
/// without accessing an object; a backend may need a runtime library query.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum C11AtomicOperation {
    Init,
    Load,
    Store,
    Exchange,
    CompareExchangeStrong,
    CompareExchangeWeak,
    FetchAdd,
    FetchSub,
    FetchAnd,
    FetchOr,
    FetchXor,
    FetchNand,
    FetchMin,
    FetchMax,
    ThreadFence,
    SignalFence,
    IsLockFree,
}

impl C11AtomicOperation {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "__c11_atomic_init" => Self::Init,
            "__c11_atomic_load" => Self::Load,
            "__c11_atomic_store" => Self::Store,
            "__c11_atomic_exchange" => Self::Exchange,
            "__c11_atomic_compare_exchange_strong" => Self::CompareExchangeStrong,
            "__c11_atomic_compare_exchange_weak" => Self::CompareExchangeWeak,
            "__c11_atomic_fetch_add" => Self::FetchAdd,
            "__c11_atomic_fetch_sub" => Self::FetchSub,
            "__c11_atomic_fetch_and" => Self::FetchAnd,
            "__c11_atomic_fetch_or" => Self::FetchOr,
            "__c11_atomic_fetch_xor" => Self::FetchXor,
            "__c11_atomic_fetch_nand" => Self::FetchNand,
            "__c11_atomic_fetch_min" => Self::FetchMin,
            "__c11_atomic_fetch_max" => Self::FetchMax,
            "__c11_atomic_thread_fence" => Self::ThreadFence,
            "__c11_atomic_signal_fence" => Self::SignalFence,
            "__c11_atomic_is_lock_free" => Self::IsLockFree,
            _ => return None,
        })
    }

    /// Positions of the ordinary/success order and optional failure order.
    /// Dynamic orders remain evaluated C `int` inputs. Initialization and queries
    /// have no order, and weak compare-exchange is encoded by the operation itself.
    pub fn memory_order_arguments(self) -> Option<(usize, Option<usize>)> {
        Some(match self {
            Self::Init | Self::IsLockFree => return None,
            Self::ThreadFence | Self::SignalFence => (0, None),
            Self::Load => (1, None),
            Self::CompareExchangeStrong | Self::CompareExchangeWeak => (3, Some(4)),
            _ => (2, None),
        })
    }

    fn count(self) -> usize {
        match self {
            Self::Init => 2,
            Self::IsLockFree => 1,
            _ => {
                let (order, failure) = self.memory_order_arguments().expect("ordered atomic");
                failure.unwrap_or(order) + 1
            }
        }
    }

    fn fetch(self) -> bool {
        matches!(
            self,
            Self::FetchAdd
                | Self::FetchSub
                | Self::FetchAnd
                | Self::FetchOr
                | Self::FetchXor
                | Self::FetchNand
                | Self::FetchMin
                | Self::FetchMax
        )
    }
}

impl Analyzer {
    /// Derives the overloaded signature while the original operand scopes exist.
    pub(crate) fn c11_atomic_signature(
        &mut self,
        op: C11AtomicOperation,
        call: &Node<ast::CallExpression>,
    ) -> Result<AtomicSignature, Error> {
        use C11AtomicOperation as A;
        let offset = call.span.start;
        if self.gnu_sync_profile() {
            return Err(Error::new(
                offset,
                "__c11_atomic intrinsics require a Clang compiler profile",
            ));
        }
        if call.node.arguments.len() != op.count() {
            return Err(Error::new(
                offset,
                "argument count does not match __c11_atomic intrinsic",
            ));
        }
        let mut signature = AtomicSignature {
            result: Type::new(TypeKind::Void),
            parameters: std::array::from_fn(|_| None),
            sources: std::array::from_fn(|_| None),
            context: UseContext::Value,
            conversions: [Conversion::Assignment; 6],
        };
        if op == A::IsLockFree {
            signature.result = Type::new(TypeKind::Bool);
            signature.parameters[0] = Some(crate::integer::integer_to_type(self.size_value(0)));
            return Ok(signature);
        }
        if let Some((success, failure)) = op.memory_order_arguments() {
            for index in [Some(success), failure].into_iter().flatten() {
                signature.parameters[index] = Some(Type::new(TypeKind::Integer(IntegerKind::Int)));
            }
        }
        if matches!(op, A::ThreadFence | A::SignalFence) {
            return Ok(signature);
        }
        let address = self.value_expression_type(&call.node.arguments[0])?;
        let TypeKind::Pointer(pointee) = &address.kind else {
            return Err(Error::new(offset, "__c11_atomic address must be a pointer"));
        };
        if self.unit.atomic_value(pointee)?.is_none() {
            return Err(Error::new(
                offset,
                "__c11_atomic address must point to an _Atomic type",
            ));
        }
        if op != A::Load && self.unit.qualifiers(pointee)?.is_const {
            return Err(Error::new(
                offset,
                "__c11_atomic operation cannot modify a const-qualified atomic object",
            ));
        }
        let value = self.atomic_value_type(pointee)?;
        self.require_complete_object(&value, offset)?;
        if op.fetch() {
            let add_sub = matches!(op, A::FetchAdd | A::FetchSub);
            let floating = add_sub || matches!(op, A::FetchMin | A::FetchMax);
            let valid = match &value.kind {
                TypeKind::Bool | TypeKind::Integer(_) | TypeKind::Enum(_) => true,
                TypeKind::Pointer(inner) if add_sub => {
                    // Clang accepts its function-pointer arithmetic extension,
                    // whose lowering has no ordinary C object stride.
                    if matches!(self.unit.resolve(inner)?.kind, TypeKind::Function(_)) {
                        return Err(Error::new(
                            offset,
                            "Clang atomic function-pointer arithmetic is unsupported",
                        ));
                    }
                    self.require_complete_object(inner, offset)?;
                    true
                }
                TypeKind::Float(FloatKind::Float | FloatKind::Double) => floating,
                TypeKind::Float(FloatKind::LongDouble) => {
                    floating
                        && !matches!(
                            self.unit.target,
                            Target::X86_64UnknownLinuxGnu | Target::X86_64AppleDarwin
                        )
                }
                _ => false,
            };
            if !valid {
                return Err(Error::new(
                    offset,
                    "unsupported __c11_atomic operand type for this operation",
                ));
            }
        }
        signature.sources[0] = Some(address.clone());
        signature.parameters[0] = Some(address);
        signature.result = match op {
            A::Init | A::Store => Type::new(TypeKind::Void),
            A::CompareExchangeStrong | A::CompareExchangeWeak => Type::new(TypeKind::Bool),
            _ => value.clone(),
        };
        if matches!(op, A::CompareExchangeStrong | A::CompareExchangeWeak) {
            signature.parameters[1] = Some(value.clone().pointer());
            signature.parameters[2] = Some(value);
        } else if op != A::Load {
            signature.parameters[1] = Some(
                if matches!(op, A::FetchAdd | A::FetchSub)
                    && matches!(value.kind, TypeKind::Pointer(_))
                {
                    Type::new(TypeKind::Integer(
                        if self.unit.target == Target::X86_64PcWindowsMsvc {
                            IntegerKind::LongLong
                        } else {
                            IntegerKind::Long
                        },
                    ))
                } else {
                    value
                },
            );
        }
        Ok(signature)
    }

    pub(crate) fn c11_atomic_call_type(
        &mut self,
        op: C11AtomicOperation,
        call: &Node<ast::CallExpression>,
    ) -> Result<Type, Error> {
        let key = (call.span.start, call.span.end);
        if op == C11AtomicOperation::IsLockFree && self.checked_atomic_queries.contains(&key) {
            return Ok(Type::new(TypeKind::Bool));
        }
        let signature = self.c11_atomic_signature(op, call)?;
        for (index, argument) in call.node.arguments.iter().enumerate() {
            let destination = signature.parameters[index]
                .as_ref()
                .expect("C11 atomic argument");
            let source = if let Some(source) = &signature.sources[index] {
                source.clone()
            } else {
                self.value_expression_type(argument)?
            };
            self.check_assignment_type(destination, &source, argument)?;
            if self.checked.is_some() {
                self.retain_assignment(argument, destination)?;
            }
        }
        if let Some((success, failure)) = op.memory_order_arguments() {
            self.check_atomic_order_arguments(
                call,
                success,
                failure,
                op == C11AtomicOperation::Load,
                op == C11AtomicOperation::Store,
            )?;
        }
        if op == C11AtomicOperation::IsLockFree {
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

    /// Clang folds zero and guaranteed scalar widths to true. Other sizes may
    /// call a runtime library, so unknown target features never become false.
    pub(crate) fn eval_c11_atomic_lock_free(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<IntegerValue, Error> {
        self.c11_atomic_call_type(C11AtomicOperation::IsLockFree, call)?;
        if matches!(self.atomic_query_size(call)?, 0 | 1 | 2 | 4 | 8) {
            return Ok(IntegerValue::new(1, 8, false, 0));
        }
        Err(Error::new(
            call.span.start,
            "C11 atomic lock-free query depends on unproven target features or runtime support",
        ))
    }
}

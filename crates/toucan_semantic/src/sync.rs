//! Source-level typing for GCC's legacy atomic operations.

use lang_c::{ast, span::Node};
use serde::Serialize;
use toucan_target::Target;

use crate::analyze::Analyzer;
use crate::{Error, Type, TypeKind};

/// Legacy atomic operations. Pointer operands use byte arithmetic, without C's
/// usual pointee-size scaling. Operations provide a full barrier, except
/// `LockTestAndSet` (acquire) and `LockRelease` (release).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum SyncOperation {
    FetchAdd,
    FetchSub,
    FetchOr,
    FetchAnd,
    FetchXor,
    FetchNand,
    AddFetch,
    SubFetch,
    OrFetch,
    AndFetch,
    XorFetch,
    NandFetch,
    BoolCompareAndSwap,
    ValueCompareAndSwap,
    LockTestAndSet,
    LockRelease,
    Synchronize,
}

impl SyncOperation {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "__sync_fetch_and_add" => Self::FetchAdd,
            "__sync_fetch_and_sub" => Self::FetchSub,
            "__sync_fetch_and_or" => Self::FetchOr,
            "__sync_fetch_and_and" => Self::FetchAnd,
            "__sync_fetch_and_xor" => Self::FetchXor,
            "__sync_fetch_and_nand" => Self::FetchNand,
            "__sync_add_and_fetch" => Self::AddFetch,
            "__sync_sub_and_fetch" => Self::SubFetch,
            "__sync_or_and_fetch" => Self::OrFetch,
            "__sync_and_and_fetch" => Self::AndFetch,
            "__sync_xor_and_fetch" => Self::XorFetch,
            "__sync_nand_and_fetch" => Self::NandFetch,
            "__sync_bool_compare_and_swap" => Self::BoolCompareAndSwap,
            "__sync_val_compare_and_swap" => Self::ValueCompareAndSwap,
            "__sync_lock_test_and_set" => Self::LockTestAndSet,
            "__sync_lock_release" => Self::LockRelease,
            "__sync_synchronize" => Self::Synchronize,
            _ => return None,
        })
    }

    pub(crate) fn required(self) -> usize {
        match self {
            Self::Synchronize => 0,
            Self::LockRelease => 1,
            Self::BoolCompareAndSwap | Self::ValueCompareAndSwap => 3,
            _ => 2,
        }
    }

    fn accepts_gnu_bool(self) -> bool {
        matches!(
            self,
            Self::LockTestAndSet
                | Self::LockRelease
                | Self::BoolCompareAndSwap
                | Self::ValueCompareAndSwap
        )
    }
}

pub(crate) struct SyncSignature {
    pub(crate) result: Type,
    pub(crate) address: Option<Type>,
    pub(crate) value: Option<Type>,
}

impl Analyzer {
    pub(crate) fn gnu_sync_profile(&self) -> bool {
        matches!(
            self.unit.target,
            Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
        )
    }

    /// The pointee selects the overloaded operation; qualifiers on the value
    /// disappear, while the object pointer retains its access qualifiers.
    pub(crate) fn sync_signature(
        &mut self,
        operation: SyncOperation,
        call: &Node<ast::CallExpression>,
    ) -> Result<SyncSignature, Error> {
        let offset = call.span.start;
        let count = call.node.arguments.len();
        if count < operation.required() || (operation == SyncOperation::Synchronize && count != 0) {
            return Err(Error::new(
                offset,
                "argument count does not match __sync intrinsic",
            ));
        }
        if operation == SyncOperation::Synchronize {
            return Ok(SyncSignature {
                result: Type::new(TypeKind::Void),
                address: None,
                value: None,
            });
        }
        let address = self.value_expression_type(&call.node.arguments[0])?;
        let TypeKind::Pointer(pointee) = &address.kind else {
            return Err(Error::new(
                offset,
                "__sync address must point to an integer or pointer object",
            ));
        };
        if self.unit.atomic_value(pointee)?.is_some() && !self.gnu_sync_profile() {
            return Err(Error::new(
                offset,
                "this Clang profile does not accept C11 atomic objects in GNU atomic intrinsics",
            ));
        }
        let mut value = self.atomic_value_type(pointee)?;
        if !matches!(
            value.kind,
            TypeKind::Integer(_) | TypeKind::Enum(_) | TypeKind::Bool | TypeKind::Pointer(_)
        ) {
            return Err(Error::new(
                offset,
                "__sync address must point to an integer or pointer object",
            ));
        }
        if matches!(value.kind, TypeKind::Bool)
            && self.gnu_sync_profile()
            && !operation.accepts_gnu_bool()
        {
            return Err(Error::new(
                offset,
                "GCC __sync arithmetic operations do not accept _Bool objects",
            ));
        }
        self.require_complete_object(&value, offset)?;
        let size = self.unit.layout(&value)?.size_bytes();
        if !matches!(size, 1 | 2 | 4 | 8 | 16) {
            return Err(Error::new(
                offset,
                "__sync object width must be 1, 2, 4, 8, or 16 bytes",
            ));
        }
        let qualifiers = self.unit.qualifiers(pointee)?;
        if qualifiers.is_const && !self.gnu_sync_profile() {
            return Err(Error::new(
                offset,
                "Clang __sync operations cannot modify a const-qualified object",
            ));
        }
        let address = if qualifiers.is_const {
            let mut pointee = value.clone();
            pointee.qualifiers = qualifiers;
            pointee.qualifiers.is_const = false;
            pointee.pointer()
        } else {
            address
        };
        if self.gnu_sync_profile() {
            value.alignment = None;
        }
        let result = match operation {
            SyncOperation::BoolCompareAndSwap => Type::new(TypeKind::Bool),
            SyncOperation::LockRelease => Type::new(TypeKind::Void),
            _ => value.clone(),
        };
        Ok(SyncSignature {
            result,
            address: Some(address),
            value: Some(value),
        })
    }

    pub(crate) fn sync_call_type(
        &mut self,
        operation: SyncOperation,
        call: &Node<ast::CallExpression>,
    ) -> Result<Type, Error> {
        let signature = self.sync_signature(operation, call)?;
        for (index, argument) in call.node.arguments.iter().enumerate().skip(1) {
            if index >= operation.required() {
                if self.gnu_sync_profile() {
                    self.value_expression_type(argument)?;
                } else {
                    self.expression_type(argument)?;
                }
                continue;
            }
            let destination = signature.value.as_ref().expect("value operation");
            if self.gnu_sync_profile() {
                let source = self.value_expression_type(argument)?;
                let valid = match &destination.kind {
                    TypeKind::Pointer(_) => matches!(
                        source.kind,
                        TypeKind::Pointer(_)
                            | TypeKind::Integer(_)
                            | TypeKind::Enum(_)
                            | TypeKind::Bool
                    ),
                    _ => {
                        self.is_arithmetic(&source)? || matches!(source.kind, TypeKind::Pointer(_))
                    }
                };
                if !valid {
                    return Err(Error::new(
                        argument.span.start,
                        "incompatible __sync value operand",
                    ));
                }
            } else {
                self.check_assignment(destination, argument)?;
            }
        }
        Ok(signature.result)
    }
}

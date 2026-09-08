//! Supported builtin names share the dispatch and signature classifiers.
use toucan_target::{Compiler, CompilerProfile};

/// Whether the profile advertises an implemented form through `__has_builtin`.
///
/// A positive answer does not waive operand, target-feature, or ABI constraints.
/// Library functions without a builtin spelling and unimplemented names return false.
pub fn has_builtin(profile: CompilerProfile, name: &str) -> bool {
    use crate::builtins;
    if let Some(intrinsic) = crate::x86::X86Intrinsic::from_name(name) {
        return intrinsic.is_available(profile);
    }
    if let Some(intrinsic) = crate::overflow::OverflowIntrinsic::from_name(name) {
        return !intrinsic.is_predicate() || profile.compiler() == Compiler::Gnu;
    }
    if crate::c11_atomic::C11AtomicOperation::from_name(name).is_some()
        || crate::elementwise::ElementwiseOperation::from_name(name).is_some()
        || crate::nontemporal::NontemporalOperation::from_name(name).is_some()
    {
        return profile.compiler() == Compiler::Clang;
    }
    if crate::atomic::AtomicOperation::from_name(name).is_some()
        || crate::sync::SyncOperation::from_name(name).is_some()
        || builtins::infinity_kind(name).is_some()
        || builtins::nan_kind(name).is_some()
        || builtins::complex_unary(name).is_some()
        || builtins::bit_count_kind(name).is_some()
        || builtins::byte_swap_kind(name, profile.target()).is_some()
        || builtins::is_memory_builtin(name)
        || builtins::simple_builtin_arity(name).is_some()
        || crate::fortified::is_fortified_builtin(name)
        || crate::object_size::is_object_size_builtin(name)
    {
        return true;
    }
    // These special forms have parser nodes or dedicated call dispatch instead
    // of ordinary source signatures. Their arguments are still fully checked.
    match name {
        "__builtin_choose_expr"
        | "__builtin_types_compatible_p"
        | "__builtin_offsetof"
        | "__builtin_convertvector"
        | "__builtin_shufflevector" => true,
        // GCC accepts these parser forms but does not register them for its query.
        "__builtin_va_arg" | "__builtin_complex" => profile.compiler() == Compiler::Clang,
        "__builtin_shuffle" | "__builtin_va_arg_pack" | "__builtin_va_arg_pack_len" => {
            profile.compiler() == Compiler::Gnu
        }
        _ => false,
    }
}

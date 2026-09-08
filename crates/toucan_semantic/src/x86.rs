//! Exact source signatures for the supported x86 instruction intrinsics.
//!
//! Signature facts follow GCC 13.3's i386 builtin descriptors and Clang 18.1.3's
//! BuiltinsX86.def. The provenance and compiler probes are recorded in the corpus.

use lang_c::{ast, span::Node};
use serde::Serialize;
use toucan_target::Target;

use crate::analyze::Analyzer;
use crate::{Error, IntegerKind, IntegerValue, Type, TypeKind};

/// CPU instruction sets required when an evaluated intrinsic is lowered.
/// The fixed x86-64 profiles currently enable MMX, SSE, and SSE2. A downstream
/// backend must preserve these requirements when selecting or disabling features.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum X86Feature {
    Mmx,
    Sse2,
}

/// The compiler stage at which an immediate operand must be proven constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum ImmediateStage {
    /// The source expression must already be an integer constant expression.
    Frontend,
    /// Inlining and folding may discharge the requirement. Successful source
    /// analysis alone does not authorize lowering an unresolved operand.
    AfterInlining,
}

/// An integer operand that instruction selection must encode as an immediate.
/// The bounds apply after conversion to the intrinsic's formal parameter type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ImmediateConstraint {
    argument: usize,
    minimum: i64,
    maximum: i64,
    stage: ImmediateStage,
}
impl ImmediateConstraint {
    /// Zero-based position in the retained intrinsic call's argument list.
    pub fn argument(self) -> usize {
        self.argument
    }
    /// Inclusive lower bound after the argument conversion.
    pub fn minimum(self) -> i64 {
        self.minimum
    }
    /// Inclusive upper bound after the argument conversion.
    pub fn maximum(self) -> i64 {
        self.maximum
    }
    /// Whether analysis has checked the constant or a consumer must still do so.
    pub fn stage(self) -> ImmediateStage {
        self.stage
    }
}

/// Owned source-level types of an intrinsic, without an ordinary C function ABI.
/// Instruction lowering uses the intrinsic's identity and target requirements.
#[derive(Clone, Debug, Serialize)]
pub struct X86Signature {
    result: Type,
    parameters: Vec<Type>,
}
impl X86Signature {
    /// Canonical C type of the intrinsic expression.
    pub fn result(&self) -> &Type {
        &self.result
    }
    /// Formal parameter types in source argument order.
    pub fn parameters(&self) -> &[Type] {
        &self.parameters
    }
}

#[derive(Clone, Copy)]
enum Scalar {
    Void,
    Char,
    Short,
    Int,
    V8Char,
    V4Short,
    V2Int,
    V1LongLong,
}
impl Scalar {
    fn ty(self) -> Type {
        let (kind, lanes) = match self {
            Self::Void => return Type::new(TypeKind::Void),
            Self::Char => (IntegerKind::Char, 0),
            Self::Short => (IntegerKind::Short, 0),
            Self::Int => (IntegerKind::Int, 0),
            Self::V8Char => (IntegerKind::Char, 8),
            Self::V4Short => (IntegerKind::Short, 4),
            Self::V2Int => (IntegerKind::Int, 2),
            Self::V1LongLong => (IntegerKind::LongLong, 1),
        };
        let ty = Type::new(TypeKind::Integer(kind));
        if lanes == 0 {
            ty
        } else {
            Type::new(TypeKind::Vector {
                element: Box::new(ty),
                lanes,
            })
        }
    }
}
struct Descriptor {
    name: &'static str,
    features: &'static [X86Feature],
    gcc: (Scalar, &'static [Scalar]),
    clang: (Scalar, &'static [Scalar]),
}
macro_rules! intrinsics {
    ($($variant:ident, $name:literal, [$($feature:ident),*],
       $gcc_return:ident, [$($gcc_parameter:ident),*],
       $clang_return:ident, [$($clang_parameter:ident),*];)*) => {
        /// An instruction intrinsic, distinct from an ordinary external function.
        /// Its argument evaluation and conversions are retained in `BuiltinCall`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
        #[non_exhaustive]
        pub enum X86Intrinsic { $($variant),* }
        impl X86Intrinsic {
            pub(crate) fn from_name(name: &str) -> Option<Self> {
                match name { $($name => Some(Self::$variant),)* _ => None }
            }
            fn descriptor(self) -> &'static Descriptor {
                match self { $(Self::$variant => &Descriptor {
                    name: $name,
                    features: &[$(X86Feature::$feature),*],
                    gcc: (Scalar::$gcc_return, &[$(Scalar::$gcc_parameter),*]),
                    clang: (Scalar::$clang_return, &[$(Scalar::$clang_parameter),*]),
                }),* }
            }
        }
    }
}
intrinsics! {
    Emms, "__builtin_ia32_emms", [Mmx], Void, [], Void, [];
    Packssdw, "__builtin_ia32_packssdw", [Mmx], V4Short, [V2Int, V2Int], V4Short, [V2Int, V2Int];
    Packsswb, "__builtin_ia32_packsswb", [Mmx], V8Char, [V4Short, V4Short], V8Char, [V4Short, V4Short];
    Packuswb, "__builtin_ia32_packuswb", [Mmx], V8Char, [V4Short, V4Short], V8Char, [V4Short, V4Short];
    Paddb, "__builtin_ia32_paddb", [Mmx], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Paddd, "__builtin_ia32_paddd", [Mmx], V2Int, [V2Int, V2Int], V2Int, [V2Int, V2Int];
    Paddq, "__builtin_ia32_paddq", [Mmx, Sse2], V1LongLong, [V1LongLong, V1LongLong], V1LongLong, [V1LongLong, V1LongLong];
    Paddsb, "__builtin_ia32_paddsb", [Mmx], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Paddsw, "__builtin_ia32_paddsw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Paddusb, "__builtin_ia32_paddusb", [Mmx], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Paddusw, "__builtin_ia32_paddusw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Paddw, "__builtin_ia32_paddw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Pand, "__builtin_ia32_pand", [Mmx], V2Int, [V2Int, V2Int], V1LongLong, [V1LongLong, V1LongLong];
    Pandn, "__builtin_ia32_pandn", [Mmx], V2Int, [V2Int, V2Int], V1LongLong, [V1LongLong, V1LongLong];
    Pcmpeqb, "__builtin_ia32_pcmpeqb", [Mmx], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Pcmpeqd, "__builtin_ia32_pcmpeqd", [Mmx], V2Int, [V2Int, V2Int], V2Int, [V2Int, V2Int];
    Pcmpeqw, "__builtin_ia32_pcmpeqw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Pcmpgtb, "__builtin_ia32_pcmpgtb", [Mmx], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Pcmpgtd, "__builtin_ia32_pcmpgtd", [Mmx], V2Int, [V2Int, V2Int], V2Int, [V2Int, V2Int];
    Pcmpgtw, "__builtin_ia32_pcmpgtw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Pmaddwd, "__builtin_ia32_pmaddwd", [Mmx], V2Int, [V4Short, V4Short], V2Int, [V4Short, V4Short];
    Pmulhw, "__builtin_ia32_pmulhw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Pmullw, "__builtin_ia32_pmullw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Por, "__builtin_ia32_por", [Mmx], V2Int, [V2Int, V2Int], V1LongLong, [V1LongLong, V1LongLong];
    Pslld, "__builtin_ia32_pslld", [Mmx], V2Int, [V2Int, V2Int], V2Int, [V2Int, V1LongLong];
    Pslldi, "__builtin_ia32_pslldi", [Mmx], V2Int, [V2Int, Int], V2Int, [V2Int, Int];
    Psllq, "__builtin_ia32_psllq", [Mmx], V1LongLong, [V1LongLong, V1LongLong], V1LongLong, [V1LongLong, V1LongLong];
    Psllqi, "__builtin_ia32_psllqi", [Mmx], V1LongLong, [V1LongLong, Int], V1LongLong, [V1LongLong, Int];
    Psllw, "__builtin_ia32_psllw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V1LongLong];
    Psllwi, "__builtin_ia32_psllwi", [Mmx], V4Short, [V4Short, Int], V4Short, [V4Short, Int];
    Psrad, "__builtin_ia32_psrad", [Mmx], V2Int, [V2Int, V2Int], V2Int, [V2Int, V1LongLong];
    Psradi, "__builtin_ia32_psradi", [Mmx], V2Int, [V2Int, Int], V2Int, [V2Int, Int];
    Psraw, "__builtin_ia32_psraw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V1LongLong];
    Psrawi, "__builtin_ia32_psrawi", [Mmx], V4Short, [V4Short, Int], V4Short, [V4Short, Int];
    Psrld, "__builtin_ia32_psrld", [Mmx], V2Int, [V2Int, V2Int], V2Int, [V2Int, V1LongLong];
    Psrldi, "__builtin_ia32_psrldi", [Mmx], V2Int, [V2Int, Int], V2Int, [V2Int, Int];
    Psrlq, "__builtin_ia32_psrlq", [Mmx], V1LongLong, [V1LongLong, V1LongLong], V1LongLong, [V1LongLong, V1LongLong];
    Psrlqi, "__builtin_ia32_psrlqi", [Mmx], V1LongLong, [V1LongLong, Int], V1LongLong, [V1LongLong, Int];
    Psrlw, "__builtin_ia32_psrlw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V1LongLong];
    Psrlwi, "__builtin_ia32_psrlwi", [Mmx], V4Short, [V4Short, Int], V4Short, [V4Short, Int];
    Psubb, "__builtin_ia32_psubb", [Mmx], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Psubd, "__builtin_ia32_psubd", [Mmx], V2Int, [V2Int, V2Int], V2Int, [V2Int, V2Int];
    Psubq, "__builtin_ia32_psubq", [Mmx, Sse2], V1LongLong, [V1LongLong, V1LongLong], V1LongLong, [V1LongLong, V1LongLong];
    Psubsb, "__builtin_ia32_psubsb", [Mmx], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Psubsw, "__builtin_ia32_psubsw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Psubusb, "__builtin_ia32_psubusb", [Mmx], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Psubusw, "__builtin_ia32_psubusw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Psubw, "__builtin_ia32_psubw", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Punpckhbw, "__builtin_ia32_punpckhbw", [Mmx], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Punpckhdq, "__builtin_ia32_punpckhdq", [Mmx], V2Int, [V2Int, V2Int], V2Int, [V2Int, V2Int];
    Punpckhwd, "__builtin_ia32_punpckhwd", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Punpcklbw, "__builtin_ia32_punpcklbw", [Mmx], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Punpckldq, "__builtin_ia32_punpckldq", [Mmx], V2Int, [V2Int, V2Int], V2Int, [V2Int, V2Int];
    Punpcklwd, "__builtin_ia32_punpcklwd", [Mmx], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Pxor, "__builtin_ia32_pxor", [Mmx], V2Int, [V2Int, V2Int], V1LongLong, [V1LongLong, V1LongLong];
    VecExtV2si, "__builtin_ia32_vec_ext_v2si", [Mmx], Int, [V2Int, Int], Int, [V2Int, Int];
    VecInitV2si, "__builtin_ia32_vec_init_v2si", [Mmx], V2Int, [Int, Int], V2Int, [Int, Int];
    VecInitV4hi, "__builtin_ia32_vec_init_v4hi", [Mmx], V4Short, [Short, Short, Short, Short], V4Short, [Short, Short, Short, Short];
    VecInitV8qi, "__builtin_ia32_vec_init_v8qi", [Mmx], V8Char, [Char, Char, Char, Char, Char, Char, Char, Char], V8Char, [Char, Char, Char, Char, Char, Char, Char, Char];
}
impl X86Intrinsic {
    /// Canonical compiler spelling of this intrinsic.
    pub fn name(self) -> &'static str {
        self.descriptor().name
    }
    /// Instruction sets needed by evaluated uses. Unevaluated calls do not
    /// themselves require instructions to execute.
    pub fn required_features(self) -> &'static [X86Feature] {
        self.descriptor().features
    }
    /// Returns the target profile's exact signature, or `None` on non-x86 targets.
    /// GCC and Clang differ in several 64-bit vector element types.
    pub fn signature(self, target: Target) -> Option<X86Signature> {
        let descriptor = self.descriptor();
        let (result, parameters) = match target {
            Target::X86_64UnknownLinuxGnu => descriptor.gcc,
            Target::X86_64AppleDarwin | Target::X86_64PcWindowsMsvc => descriptor.clang,
            _ => return None,
        };
        Some(X86Signature {
            result: result.ty(),
            parameters: parameters.iter().map(|parameter| parameter.ty()).collect(),
        })
    }
    /// Immediate obligations for this compiler profile. GNU checking may leave
    /// these pending even for known invalid values: its expander checks them only
    /// after inlining. A consumer must discharge them before code generation.
    pub fn immediate_constraints(self, target: Target) -> &'static [ImmediateConstraint] {
        if self != Self::VecExtV2si {
            return &[];
        }
        match target {
            Target::X86_64UnknownLinuxGnu => &[ImmediateConstraint {
                argument: 1,
                minimum: 0,
                maximum: 1,
                stage: ImmediateStage::AfterInlining,
            }],
            Target::X86_64AppleDarwin | Target::X86_64PcWindowsMsvc => &[ImmediateConstraint {
                argument: 1,
                minimum: 0,
                maximum: 1,
                stage: ImmediateStage::Frontend,
            }],
            _ => &[],
        }
    }
    /// Whether Clang classifies the call itself as having ordinary side effects.
    /// Operand effects remain separate. This compiler classification does not
    /// describe every architectural effect on MMX/x87 register state.
    pub fn has_side_effects(self) -> bool {
        self == Self::Emms
    }
}

impl Analyzer {
    pub(crate) fn x86_call_type(
        &mut self,
        intrinsic: X86Intrinsic,
        call: &Node<ast::CallExpression>,
    ) -> Result<Type, Error> {
        let signature = intrinsic.signature(self.unit.target).ok_or_else(|| {
            Error::new(
                call.span.start,
                "x86 instruction intrinsics require an x86-64 target profile",
            )
        })?;
        if signature.parameters.len() != call.node.arguments.len() {
            return Err(Error::new(
                call.span.start,
                format!(
                    "{} requires {} arguments",
                    intrinsic.name(),
                    signature.parameters.len(),
                ),
            ));
        }
        for (parameter, argument) in signature.parameters.iter().zip(&call.node.arguments) {
            let source = self.value_expression_type(argument)?;
            // Clang's default lax-vector conversion reinterprets equal-sized
            // vectors here, including integer/floating lane changes. Scalars
            // cannot implicitly become vector arguments. GCC requires compatible
            // vector types unless its separate lax-vector option is enabled.
            if !self.gnu_vector_profile()
                && matches!(source.kind, TypeKind::Vector { .. })
                && matches!(parameter.kind, TypeKind::Vector { .. })
                && self.unit.layout(&source)?.size_bits == self.unit.layout(parameter)?.size_bits
            {
                continue;
            }
            self.check_assignment_type(parameter, &source, argument)?;
            if self.checked.is_some() {
                self.retain_assignment(argument, parameter)?;
            }
        }
        for constraint in intrinsic.immediate_constraints(self.unit.target) {
            if constraint.stage != ImmediateStage::Frontend {
                continue;
            }
            let argument = &call.node.arguments[constraint.argument];
            if !self.is_integer_constant_expression(argument, 0)? {
                return Err(Error::new(
                    argument.span.start,
                    format!(
                        "{} argument {} requires an integer constant expression",
                        intrinsic.name(),
                        constraint.argument + 1,
                    ),
                ));
            }
            let value = crate::integer::convert(self.eval(argument)?, IntegerValue::int(0));
            if value.signed_value() < i128::from(constraint.minimum)
                || value.signed_value() > i128::from(constraint.maximum)
            {
                return Err(Error::new(
                    argument.span.start,
                    format!(
                        "{} argument {} must be in {}..={}",
                        intrinsic.name(),
                        constraint.argument + 1,
                        constraint.minimum,
                        constraint.maximum,
                    ),
                ));
            }
        }
        Ok(signature.result)
    }
}

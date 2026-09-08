//! Exact source signatures for the supported x86 intrinsics.
//!
//! Signature facts follow GCC 13.3's i386 builtin descriptors and Clang 18.1.3's
//! BuiltinsX86.def. The provenance and compiler probes are recorded in the corpus.

use lang_c::{ast, span::Node};
use serde::Serialize;
use toucan_target::Target;

use crate::analyze::Analyzer;
use crate::{Error, FloatKind, IntegerKind, Type, TypeKind};

/// CPU instruction sets required when an evaluated intrinsic is lowered.
/// The fixed x86-64 profiles currently enable MMX, SSE, and SSE2. A downstream
/// backend must preserve these requirements when selecting or disabling features.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum X86Feature {
    Mmx,
    Sse,
    Sse2,
    Lzcnt,
    Bmi,
    Bmi2,
}

impl X86Feature {
    pub(crate) const ALL: [Self; 6] = [
        Self::Mmx,
        Self::Sse,
        Self::Sse2,
        Self::Lzcnt,
        Self::Bmi,
        Self::Bmi2,
    ];
    pub(crate) const fn bit(self) -> u8 {
        match self {
            Self::Mmx => 1,
            Self::Sse => 2,
            Self::Sse2 => 4,
            Self::Lzcnt => 8,
            Self::Bmi => 16,
            Self::Bmi2 => 32,
        }
    }
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
    multiple_of: u32,
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
    /// Divisibility required by the instruction's argument convention. For
    /// example, GNU whole-register byte shifts take a bit count divisible by 8.
    pub fn multiple_of(self) -> u32 {
        self.multiple_of
    }
    /// Whether analysis has checked the constant or a consumer must still do so.
    pub fn stage(self) -> ImmediateStage {
        self.stage
    }
}

/// An immediate restriction enabled by another converted argument value.
/// A consumer checks the requirement when `condition` matches. This preserves
/// GCC prefetch's instruction/data hint distinction through deferred lowering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ConditionalImmediateConstraint {
    when_argument: usize,
    when_value: i64,
    requirement: ImmediateConstraint,
}
impl ConditionalImmediateConstraint {
    /// Zero-based argument index and converted value that enable the restriction.
    pub fn condition(self) -> (usize, i64) {
        (self.when_argument, self.when_value)
    }
    /// The additional range and stage required when the condition matches.
    pub fn requirement(self) -> ImmediateConstraint {
        self.requirement
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
enum IntrinsicType {
    Char,
    CharPointer,
    ConstDoublePointer,
    ConstV2FloatPointer,
    ConstVoidPointer,
    DoublePointer,
    FloatPointer,
    Int,
    IntPointer,
    LongLong,
    LongLongPointer,
    Short,
    UnsignedInt,
    UnsignedLongLong,
    UnsignedLongLongPointer,
    V16Char,
    V1LongLong,
    V1LongLongPointer,
    V2Double,
    V2FloatPointer,
    V2Int,
    V2LongLong,
    V2LongLongPointer,
    V4Float,
    V4Int,
    V4Short,
    V8Char,
    V8Short,
    Void,
}
impl IntrinsicType {
    fn ty(self) -> Type {
        match self {
            Self::Char => Type::new(TypeKind::Integer(IntegerKind::Char)),
            Self::CharPointer => pointer(Type::new(TypeKind::Integer(IntegerKind::Char)), false),
            Self::ConstDoublePointer => {
                pointer(Type::new(TypeKind::Float(FloatKind::Double)), true)
            }
            Self::ConstV2FloatPointer => {
                pointer(vector(TypeKind::Float(FloatKind::Float), 2), true)
            }
            Self::ConstVoidPointer => pointer(Type::new(TypeKind::Void), true),
            Self::DoublePointer => pointer(Type::new(TypeKind::Float(FloatKind::Double)), false),
            Self::FloatPointer => pointer(Type::new(TypeKind::Float(FloatKind::Float)), false),
            Self::Int => Type::new(TypeKind::Integer(IntegerKind::Int)),
            Self::IntPointer => pointer(Type::new(TypeKind::Integer(IntegerKind::Int)), false),
            Self::LongLong => Type::new(TypeKind::Integer(IntegerKind::LongLong)),
            Self::LongLongPointer => {
                pointer(Type::new(TypeKind::Integer(IntegerKind::LongLong)), false)
            }
            Self::Short => Type::new(TypeKind::Integer(IntegerKind::Short)),
            Self::UnsignedInt => Type::new(TypeKind::Integer(IntegerKind::UnsignedInt)),
            Self::UnsignedLongLong => Type::new(TypeKind::Integer(IntegerKind::UnsignedLongLong)),
            Self::UnsignedLongLongPointer => pointer(
                Type::new(TypeKind::Integer(IntegerKind::UnsignedLongLong)),
                false,
            ),
            Self::V16Char => vector(TypeKind::Integer(IntegerKind::Char), 16),
            Self::V1LongLong => vector(TypeKind::Integer(IntegerKind::LongLong), 1),
            Self::V1LongLongPointer => pointer(Self::V1LongLong.ty(), false),
            Self::V2Double => vector(TypeKind::Float(FloatKind::Double), 2),
            Self::V2FloatPointer => pointer(vector(TypeKind::Float(FloatKind::Float), 2), false),
            Self::V2Int => vector(TypeKind::Integer(IntegerKind::Int), 2),
            Self::V2LongLong => vector(TypeKind::Integer(IntegerKind::LongLong), 2),
            Self::V2LongLongPointer => pointer(Self::V2LongLong.ty(), false),
            Self::V4Float => vector(TypeKind::Float(FloatKind::Float), 4),
            Self::V4Int => vector(TypeKind::Integer(IntegerKind::Int), 4),
            Self::V4Short => vector(TypeKind::Integer(IntegerKind::Short), 4),
            Self::V8Char => vector(TypeKind::Integer(IntegerKind::Char), 8),
            Self::V8Short => vector(TypeKind::Integer(IntegerKind::Short), 8),
            Self::Void => Type::new(TypeKind::Void),
        }
    }
}
fn vector(element: TypeKind, lanes: u64) -> Type {
    Type::new(TypeKind::Vector {
        kind: crate::VectorKind::Gnu,
        element: Box::new(Type::new(element)),
        lanes,
    })
}
fn pointer(mut element: Type, constant: bool) -> Type {
    element.qualifiers.is_const = constant;
    element.pointer()
}
struct Descriptor {
    name: &'static str,
    features: &'static [X86Feature],
    gcc: Option<(IntrinsicType, &'static [IntrinsicType])>,
    clang: Option<(IntrinsicType, &'static [IntrinsicType])>,
}
macro_rules! intrinsic_signature {
    (Unsupported, []) => { None };
    ($result:ident, [$($parameter:ident),*]) => {
        Some((IntrinsicType::$result, &[$(IntrinsicType::$parameter),*]))
    };
}
macro_rules! intrinsics {
    ($($variant:ident, $name:literal, [$($feature:ident),*],
       $gcc_return:ident, [$($gcc_parameter:ident),*],
       $clang_return:ident, [$($clang_parameter:ident),*];)*) => {
        /// A target intrinsic, distinct from an ordinary external function.
        /// Its argument evaluation and conversions are retained in `BuiltinCall`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
        #[non_exhaustive]
        pub enum X86Intrinsic { $($variant),* }
        impl X86Intrinsic {
            /// Recognizes a supported spelling. `signature` determines whether
            /// that intrinsic is available in a particular compiler profile.
            pub fn from_name(name: &str) -> Option<Self> {
                match name { $($name => Some(Self::$variant),)* _ => None }
            }
            fn descriptor(self) -> &'static Descriptor {
                match self { $(Self::$variant => &Descriptor {
                    name: $name,
                    features: &[$(X86Feature::$feature),*],
                    gcc: intrinsic_signature!($gcc_return, [$($gcc_parameter),*]),
                    clang: intrinsic_signature!($clang_return, [$($clang_parameter),*]),
                }),* }
            }
        }
    }
}
intrinsics! {
    Undef128, "__builtin_ia32_undef128", [], Unsupported, [], V2Double, [];
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
    Addss, "__builtin_ia32_addss", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Andnps, "__builtin_ia32_andnps", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Andps, "__builtin_ia32_andps", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Cmpeqps, "__builtin_ia32_cmpeqps", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpeqss, "__builtin_ia32_cmpeqss", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpgeps, "__builtin_ia32_cmpgeps", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Cmpgtps, "__builtin_ia32_cmpgtps", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Cmpleps, "__builtin_ia32_cmpleps", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpless, "__builtin_ia32_cmpless", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpltps, "__builtin_ia32_cmpltps", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpltss, "__builtin_ia32_cmpltss", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpneqps, "__builtin_ia32_cmpneqps", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpneqss, "__builtin_ia32_cmpneqss", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpngeps, "__builtin_ia32_cmpngeps", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Cmpngtps, "__builtin_ia32_cmpngtps", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Cmpnleps, "__builtin_ia32_cmpnleps", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpnless, "__builtin_ia32_cmpnless", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpnltps, "__builtin_ia32_cmpnltps", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpnltss, "__builtin_ia32_cmpnltss", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpordps, "__builtin_ia32_cmpordps", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpordss, "__builtin_ia32_cmpordss", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpunordps, "__builtin_ia32_cmpunordps", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Cmpunordss, "__builtin_ia32_cmpunordss", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Comieq, "__builtin_ia32_comieq", [Sse], Int, [V4Float, V4Float], Int, [V4Float, V4Float];
    Comige, "__builtin_ia32_comige", [Sse], Int, [V4Float, V4Float], Int, [V4Float, V4Float];
    Comigt, "__builtin_ia32_comigt", [Sse], Int, [V4Float, V4Float], Int, [V4Float, V4Float];
    Comile, "__builtin_ia32_comile", [Sse], Int, [V4Float, V4Float], Int, [V4Float, V4Float];
    Comilt, "__builtin_ia32_comilt", [Sse], Int, [V4Float, V4Float], Int, [V4Float, V4Float];
    Comineq, "__builtin_ia32_comineq", [Sse], Int, [V4Float, V4Float], Int, [V4Float, V4Float];
    Cvtpi2ps, "__builtin_ia32_cvtpi2ps", [Mmx, Sse], V4Float, [V4Float, V2Int], V4Float, [V4Float, V2Int];
    Cvtps2pi, "__builtin_ia32_cvtps2pi", [Mmx, Sse], V2Int, [V4Float], V2Int, [V4Float];
    Cvtsi2ss, "__builtin_ia32_cvtsi2ss", [Sse], V4Float, [V4Float, Int], Unsupported, [];
    Cvtsi642ss, "__builtin_ia32_cvtsi642ss", [Sse], V4Float, [V4Float, LongLong], Unsupported, [];
    Cvtss2si, "__builtin_ia32_cvtss2si", [Sse], Int, [V4Float], Int, [V4Float];
    Cvtss2si64, "__builtin_ia32_cvtss2si64", [Sse], LongLong, [V4Float], LongLong, [V4Float];
    Cvttps2pi, "__builtin_ia32_cvttps2pi", [Mmx, Sse], V2Int, [V4Float], V2Int, [V4Float];
    Cvttss2si, "__builtin_ia32_cvttss2si", [Sse], Int, [V4Float], Int, [V4Float];
    Cvttss2si64, "__builtin_ia32_cvttss2si64", [Sse], LongLong, [V4Float], LongLong, [V4Float];
    Divss, "__builtin_ia32_divss", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Ldmxcsr, "__builtin_ia32_ldmxcsr", [Sse], Void, [UnsignedInt], Void, [UnsignedInt];
    Loadhps, "__builtin_ia32_loadhps", [Sse], V4Float, [V4Float, ConstV2FloatPointer], Unsupported, [];
    Loadlps, "__builtin_ia32_loadlps", [Sse], V4Float, [V4Float, ConstV2FloatPointer], Unsupported, [];
    Maskmovdqu, "__builtin_ia32_maskmovdqu", [Sse2], Void, [V16Char, V16Char, CharPointer], Void, [V16Char, V16Char, CharPointer];
    Maskmovq, "__builtin_ia32_maskmovq", [Mmx, Sse], Void, [V8Char, V8Char, CharPointer], Void, [V8Char, V8Char, CharPointer];
    Maxps, "__builtin_ia32_maxps", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Maxss, "__builtin_ia32_maxss", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Minps, "__builtin_ia32_minps", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Minss, "__builtin_ia32_minss", [Sse], V4Float, [V4Float, V4Float], V4Float, [V4Float, V4Float];
    Movhlps, "__builtin_ia32_movhlps", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Movlhps, "__builtin_ia32_movlhps", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Movmskps, "__builtin_ia32_movmskps", [Sse], Int, [V4Float], Int, [V4Float];
    Movntps, "__builtin_ia32_movntps", [Sse], Void, [FloatPointer, V4Float], Unsupported, [];
    Movntq, "__builtin_ia32_movntq", [Mmx, Sse], Void, [UnsignedLongLongPointer, UnsignedLongLong], Void, [V1LongLongPointer, V1LongLong];
    Movss, "__builtin_ia32_movss", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Mulss, "__builtin_ia32_mulss", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Orps, "__builtin_ia32_orps", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Pause, "__builtin_ia32_pause", [], Void, [], Void, [];
    Pavgb, "__builtin_ia32_pavgb", [Mmx, Sse], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Pavgw, "__builtin_ia32_pavgw", [Mmx, Sse], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Pmaxsw, "__builtin_ia32_pmaxsw", [Mmx, Sse], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Pmaxub, "__builtin_ia32_pmaxub", [Mmx, Sse], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Pminsw, "__builtin_ia32_pminsw", [Mmx, Sse], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Pminub, "__builtin_ia32_pminub", [Mmx, Sse], V8Char, [V8Char, V8Char], V8Char, [V8Char, V8Char];
    Pmovmskb, "__builtin_ia32_pmovmskb", [Mmx, Sse], Int, [V8Char], Int, [V8Char];
    Pmulhuw, "__builtin_ia32_pmulhuw", [Mmx, Sse], V4Short, [V4Short, V4Short], V4Short, [V4Short, V4Short];
    Prefetch, "__builtin_ia32_prefetch", [], Void, [ConstVoidPointer, Int, Int, Int], Unsupported, [];
    Psadbw, "__builtin_ia32_psadbw", [Mmx, Sse], V1LongLong, [V8Char, V8Char], V4Short, [V8Char, V8Char];
    Pshufw, "__builtin_ia32_pshufw", [Mmx, Sse], V4Short, [V4Short, Int], V4Short, [V4Short, Char];
    Pslldqi128, "__builtin_ia32_pslldqi128", [Sse2], V2LongLong, [V2LongLong, Int], Unsupported, [];
    Rcpps, "__builtin_ia32_rcpps", [Sse], V4Float, [V4Float], V4Float, [V4Float];
    Rcpss, "__builtin_ia32_rcpss", [Sse], V4Float, [V4Float], V4Float, [V4Float];
    Rsqrtps, "__builtin_ia32_rsqrtps", [Sse], V4Float, [V4Float], V4Float, [V4Float];
    Rsqrtss, "__builtin_ia32_rsqrtss", [Sse], V4Float, [V4Float], V4Float, [V4Float];
    Sfence, "__builtin_ia32_sfence", [Sse], Void, [], Void, [];
    Shufps, "__builtin_ia32_shufps", [Sse], V4Float, [V4Float, V4Float, Int], V4Float, [V4Float, V4Float, Int];
    Sqrtps, "__builtin_ia32_sqrtps", [Sse], V4Float, [V4Float], V4Float, [V4Float];
    Sqrtss, "__builtin_ia32_sqrtss", [Sse], V4Float, [V4Float], V4Float, [V4Float];
    Stmxcsr, "__builtin_ia32_stmxcsr", [Sse], UnsignedInt, [], UnsignedInt, [];
    Storehps, "__builtin_ia32_storehps", [Sse], Void, [V2FloatPointer, V4Float], Unsupported, [];
    Storelps, "__builtin_ia32_storelps", [Sse], Void, [V2FloatPointer, V4Float], Unsupported, [];
    Subss, "__builtin_ia32_subss", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Ucomieq, "__builtin_ia32_ucomieq", [Sse], Int, [V4Float, V4Float], Int, [V4Float, V4Float];
    Ucomige, "__builtin_ia32_ucomige", [Sse], Int, [V4Float, V4Float], Int, [V4Float, V4Float];
    Ucomigt, "__builtin_ia32_ucomigt", [Sse], Int, [V4Float, V4Float], Int, [V4Float, V4Float];
    Ucomile, "__builtin_ia32_ucomile", [Sse], Int, [V4Float, V4Float], Int, [V4Float, V4Float];
    Ucomilt, "__builtin_ia32_ucomilt", [Sse], Int, [V4Float, V4Float], Int, [V4Float, V4Float];
    Ucomineq, "__builtin_ia32_ucomineq", [Sse], Int, [V4Float, V4Float], Int, [V4Float, V4Float];
    Unpckhps, "__builtin_ia32_unpckhps", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Unpcklps, "__builtin_ia32_unpcklps", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    VecExtV4hi, "__builtin_ia32_vec_ext_v4hi", [Mmx, Sse], Short, [V4Short, Int], Int, [V4Short, Int];
    VecSetV4hi, "__builtin_ia32_vec_set_v4hi", [Mmx, Sse], V4Short, [V4Short, Short, Int], V4Short, [V4Short, Int, Int];
    Xorps, "__builtin_ia32_xorps", [Sse], V4Float, [V4Float, V4Float], Unsupported, [];
    Addsd, "__builtin_ia32_addsd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Andnpd, "__builtin_ia32_andnpd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Andpd, "__builtin_ia32_andpd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Clflush, "__builtin_ia32_clflush", [Sse2], Void, [ConstVoidPointer], Void, [ConstVoidPointer];
    Cmpeqpd, "__builtin_ia32_cmpeqpd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpeqsd, "__builtin_ia32_cmpeqsd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpgepd, "__builtin_ia32_cmpgepd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Cmpgtpd, "__builtin_ia32_cmpgtpd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Cmplepd, "__builtin_ia32_cmplepd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmplesd, "__builtin_ia32_cmplesd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpltpd, "__builtin_ia32_cmpltpd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpltsd, "__builtin_ia32_cmpltsd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpneqpd, "__builtin_ia32_cmpneqpd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpneqsd, "__builtin_ia32_cmpneqsd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpngepd, "__builtin_ia32_cmpngepd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Cmpngtpd, "__builtin_ia32_cmpngtpd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Cmpnlepd, "__builtin_ia32_cmpnlepd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpnlesd, "__builtin_ia32_cmpnlesd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpnltpd, "__builtin_ia32_cmpnltpd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpnltsd, "__builtin_ia32_cmpnltsd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpordpd, "__builtin_ia32_cmpordpd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpordsd, "__builtin_ia32_cmpordsd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpunordpd, "__builtin_ia32_cmpunordpd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Cmpunordsd, "__builtin_ia32_cmpunordsd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Comisdeq, "__builtin_ia32_comisdeq", [Sse2], Int, [V2Double, V2Double], Int, [V2Double, V2Double];
    Comisdge, "__builtin_ia32_comisdge", [Sse2], Int, [V2Double, V2Double], Int, [V2Double, V2Double];
    Comisdgt, "__builtin_ia32_comisdgt", [Sse2], Int, [V2Double, V2Double], Int, [V2Double, V2Double];
    Comisdle, "__builtin_ia32_comisdle", [Sse2], Int, [V2Double, V2Double], Int, [V2Double, V2Double];
    Comisdlt, "__builtin_ia32_comisdlt", [Sse2], Int, [V2Double, V2Double], Int, [V2Double, V2Double];
    Comisdneq, "__builtin_ia32_comisdneq", [Sse2], Int, [V2Double, V2Double], Int, [V2Double, V2Double];
    Cvtdq2pd, "__builtin_ia32_cvtdq2pd", [Sse2], V2Double, [V4Int], Unsupported, [];
    Cvtdq2ps, "__builtin_ia32_cvtdq2ps", [Sse2], V4Float, [V4Int], Unsupported, [];
    Cvtpd2dq, "__builtin_ia32_cvtpd2dq", [Sse2], V4Int, [V2Double], V2LongLong, [V2Double];
    Cvtpd2pi, "__builtin_ia32_cvtpd2pi", [Mmx, Sse2], V2Int, [V2Double], V2Int, [V2Double];
    Cvtpd2ps, "__builtin_ia32_cvtpd2ps", [Sse2], V4Float, [V2Double], V4Float, [V2Double];
    Cvtpi2pd, "__builtin_ia32_cvtpi2pd", [Mmx, Sse2], V2Double, [V2Int], V2Double, [V2Int];
    Cvtps2dq, "__builtin_ia32_cvtps2dq", [Sse2], V4Int, [V4Float], V4Int, [V4Float];
    Cvtps2pd, "__builtin_ia32_cvtps2pd", [Sse2], V2Double, [V4Float], Unsupported, [];
    Cvtsd2si, "__builtin_ia32_cvtsd2si", [Sse2], Int, [V2Double], Int, [V2Double];
    Cvtsd2si64, "__builtin_ia32_cvtsd2si64", [Sse2], LongLong, [V2Double], LongLong, [V2Double];
    Cvtsd2ss, "__builtin_ia32_cvtsd2ss", [Sse2], V4Float, [V4Float, V2Double], V4Float, [V4Float, V2Double];
    Cvtsi2sd, "__builtin_ia32_cvtsi2sd", [Sse2], V2Double, [V2Double, Int], Unsupported, [];
    Cvtsi642sd, "__builtin_ia32_cvtsi642sd", [Sse2], V2Double, [V2Double, LongLong], Unsupported, [];
    Cvtss2sd, "__builtin_ia32_cvtss2sd", [Sse2], V2Double, [V2Double, V4Float], Unsupported, [];
    Cvttpd2dq, "__builtin_ia32_cvttpd2dq", [Sse2], V4Int, [V2Double], V4Int, [V2Double];
    Cvttpd2pi, "__builtin_ia32_cvttpd2pi", [Mmx, Sse2], V2Int, [V2Double], V2Int, [V2Double];
    Cvttps2dq, "__builtin_ia32_cvttps2dq", [Sse2], V4Int, [V4Float], V4Int, [V4Float];
    Cvttsd2si, "__builtin_ia32_cvttsd2si", [Sse2], Int, [V2Double], Int, [V2Double];
    Cvttsd2si64, "__builtin_ia32_cvttsd2si64", [Sse2], LongLong, [V2Double], LongLong, [V2Double];
    Divsd, "__builtin_ia32_divsd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Lfence, "__builtin_ia32_lfence", [Sse2], Void, [], Void, [];
    Loadhpd, "__builtin_ia32_loadhpd", [Sse2], V2Double, [V2Double, ConstDoublePointer], Unsupported, [];
    Loadlpd, "__builtin_ia32_loadlpd", [Sse2], V2Double, [V2Double, ConstDoublePointer], Unsupported, [];
    Maxpd, "__builtin_ia32_maxpd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Maxsd, "__builtin_ia32_maxsd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Mfence, "__builtin_ia32_mfence", [Sse2], Void, [], Void, [];
    Minpd, "__builtin_ia32_minpd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Minsd, "__builtin_ia32_minsd", [Sse2], V2Double, [V2Double, V2Double], V2Double, [V2Double, V2Double];
    Movmskpd, "__builtin_ia32_movmskpd", [Sse2], Int, [V2Double], Int, [V2Double];
    Movntdq, "__builtin_ia32_movntdq", [Sse2], Void, [V2LongLongPointer, V2LongLong], Unsupported, [];
    Movnti, "__builtin_ia32_movnti", [Sse2], Void, [IntPointer, Int], Void, [IntPointer, Int];
    Movnti64, "__builtin_ia32_movnti64", [Sse2], Void, [LongLongPointer, LongLong], Void, [LongLongPointer, LongLong];
    Movntpd, "__builtin_ia32_movntpd", [Sse2], Void, [DoublePointer, V2Double], Unsupported, [];
    Movq128, "__builtin_ia32_movq128", [Sse2], V2LongLong, [V2LongLong], Unsupported, [];
    Movsd, "__builtin_ia32_movsd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Mulsd, "__builtin_ia32_mulsd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Orpd, "__builtin_ia32_orpd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Packssdw128, "__builtin_ia32_packssdw128", [Sse2], V8Short, [V4Int, V4Int], V8Short, [V4Int, V4Int];
    Packsswb128, "__builtin_ia32_packsswb128", [Sse2], V16Char, [V8Short, V8Short], V16Char, [V8Short, V8Short];
    Packuswb128, "__builtin_ia32_packuswb128", [Sse2], V16Char, [V8Short, V8Short], V16Char, [V8Short, V8Short];
    Paddsb128, "__builtin_ia32_paddsb128", [Sse2], V16Char, [V16Char, V16Char], Unsupported, [];
    Paddsw128, "__builtin_ia32_paddsw128", [Sse2], V8Short, [V8Short, V8Short], Unsupported, [];
    Paddusb128, "__builtin_ia32_paddusb128", [Sse2], V16Char, [V16Char, V16Char], Unsupported, [];
    Paddusw128, "__builtin_ia32_paddusw128", [Sse2], V8Short, [V8Short, V8Short], Unsupported, [];
    Pandn128, "__builtin_ia32_pandn128", [Sse2], V2LongLong, [V2LongLong, V2LongLong], Unsupported, [];
    Pavgb128, "__builtin_ia32_pavgb128", [Sse2], V16Char, [V16Char, V16Char], V16Char, [V16Char, V16Char];
    Pavgw128, "__builtin_ia32_pavgw128", [Sse2], V8Short, [V8Short, V8Short], V8Short, [V8Short, V8Short];
    Pmaddwd128, "__builtin_ia32_pmaddwd128", [Sse2], V4Int, [V8Short, V8Short], V4Int, [V8Short, V8Short];
    Pmaxsw128, "__builtin_ia32_pmaxsw128", [Sse2], V8Short, [V8Short, V8Short], Unsupported, [];
    Pmaxub128, "__builtin_ia32_pmaxub128", [Sse2], V16Char, [V16Char, V16Char], Unsupported, [];
    Pminsw128, "__builtin_ia32_pminsw128", [Sse2], V8Short, [V8Short, V8Short], Unsupported, [];
    Pminub128, "__builtin_ia32_pminub128", [Sse2], V16Char, [V16Char, V16Char], Unsupported, [];
    Pmovmskb128, "__builtin_ia32_pmovmskb128", [Sse2], Int, [V16Char], Int, [V16Char];
    Pmulhuw128, "__builtin_ia32_pmulhuw128", [Sse2], V8Short, [V8Short, V8Short], V8Short, [V8Short, V8Short];
    Pmulhw128, "__builtin_ia32_pmulhw128", [Sse2], V8Short, [V8Short, V8Short], V8Short, [V8Short, V8Short];
    Pmuludq, "__builtin_ia32_pmuludq", [Mmx, Sse2], V1LongLong, [V2Int, V2Int], V1LongLong, [V2Int, V2Int];
    Pmuludq128, "__builtin_ia32_pmuludq128", [Sse2], V2LongLong, [V4Int, V4Int], V2LongLong, [V4Int, V4Int];
    Psadbw128, "__builtin_ia32_psadbw128", [Sse2], V2LongLong, [V16Char, V16Char], V2LongLong, [V16Char, V16Char];
    Pshufd, "__builtin_ia32_pshufd", [Sse2], V4Int, [V4Int, Int], V4Int, [V4Int, Int];
    Pshufhw, "__builtin_ia32_pshufhw", [Sse2], V8Short, [V8Short, Int], V8Short, [V8Short, Int];
    Pshuflw, "__builtin_ia32_pshuflw", [Sse2], V8Short, [V8Short, Int], V8Short, [V8Short, Int];
    Pslld128, "__builtin_ia32_pslld128", [Sse2], V4Int, [V4Int, V4Int], V4Int, [V4Int, V4Int];
    Pslldi128, "__builtin_ia32_pslldi128", [Sse2], V4Int, [V4Int, Int], V4Int, [V4Int, Int];
    Psllq128, "__builtin_ia32_psllq128", [Sse2], V2LongLong, [V2LongLong, V2LongLong], V2LongLong, [V2LongLong, V2LongLong];
    Psllqi128, "__builtin_ia32_psllqi128", [Sse2], V2LongLong, [V2LongLong, Int], V2LongLong, [V2LongLong, Int];
    Psllw128, "__builtin_ia32_psllw128", [Sse2], V8Short, [V8Short, V8Short], V8Short, [V8Short, V8Short];
    Psllwi128, "__builtin_ia32_psllwi128", [Sse2], V8Short, [V8Short, Int], V8Short, [V8Short, Int];
    Psrad128, "__builtin_ia32_psrad128", [Sse2], V4Int, [V4Int, V4Int], V4Int, [V4Int, V4Int];
    Psradi128, "__builtin_ia32_psradi128", [Sse2], V4Int, [V4Int, Int], V4Int, [V4Int, Int];
    Psraw128, "__builtin_ia32_psraw128", [Sse2], V8Short, [V8Short, V8Short], V8Short, [V8Short, V8Short];
    Psrawi128, "__builtin_ia32_psrawi128", [Sse2], V8Short, [V8Short, Int], V8Short, [V8Short, Int];
    Psrld128, "__builtin_ia32_psrld128", [Sse2], V4Int, [V4Int, V4Int], V4Int, [V4Int, V4Int];
    Psrldi128, "__builtin_ia32_psrldi128", [Sse2], V4Int, [V4Int, Int], V4Int, [V4Int, Int];
    Psrldqi128, "__builtin_ia32_psrldqi128", [Sse2], V2LongLong, [V2LongLong, Int], Unsupported, [];
    Psrlq128, "__builtin_ia32_psrlq128", [Sse2], V2LongLong, [V2LongLong, V2LongLong], V2LongLong, [V2LongLong, V2LongLong];
    Psrlqi128, "__builtin_ia32_psrlqi128", [Sse2], V2LongLong, [V2LongLong, Int], V2LongLong, [V2LongLong, Int];
    Psrlw128, "__builtin_ia32_psrlw128", [Sse2], V8Short, [V8Short, V8Short], V8Short, [V8Short, V8Short];
    Psrlwi128, "__builtin_ia32_psrlwi128", [Sse2], V8Short, [V8Short, Int], V8Short, [V8Short, Int];
    Psubsb128, "__builtin_ia32_psubsb128", [Sse2], V16Char, [V16Char, V16Char], Unsupported, [];
    Psubsw128, "__builtin_ia32_psubsw128", [Sse2], V8Short, [V8Short, V8Short], Unsupported, [];
    Psubusb128, "__builtin_ia32_psubusb128", [Sse2], V16Char, [V16Char, V16Char], Unsupported, [];
    Psubusw128, "__builtin_ia32_psubusw128", [Sse2], V8Short, [V8Short, V8Short], Unsupported, [];
    Punpckhbw128, "__builtin_ia32_punpckhbw128", [Sse2], V16Char, [V16Char, V16Char], Unsupported, [];
    Punpckhdq128, "__builtin_ia32_punpckhdq128", [Sse2], V4Int, [V4Int, V4Int], Unsupported, [];
    Punpckhqdq128, "__builtin_ia32_punpckhqdq128", [Sse2], V2LongLong, [V2LongLong, V2LongLong], Unsupported, [];
    Punpckhwd128, "__builtin_ia32_punpckhwd128", [Sse2], V8Short, [V8Short, V8Short], Unsupported, [];
    Punpcklbw128, "__builtin_ia32_punpcklbw128", [Sse2], V16Char, [V16Char, V16Char], Unsupported, [];
    Punpckldq128, "__builtin_ia32_punpckldq128", [Sse2], V4Int, [V4Int, V4Int], Unsupported, [];
    Punpcklqdq128, "__builtin_ia32_punpcklqdq128", [Sse2], V2LongLong, [V2LongLong, V2LongLong], Unsupported, [];
    Punpcklwd128, "__builtin_ia32_punpcklwd128", [Sse2], V8Short, [V8Short, V8Short], Unsupported, [];
    Shufpd, "__builtin_ia32_shufpd", [Sse2], V2Double, [V2Double, V2Double, Int], V2Double, [V2Double, V2Double, Int];
    Sqrtpd, "__builtin_ia32_sqrtpd", [Sse2], V2Double, [V2Double], V2Double, [V2Double];
    Sqrtsd, "__builtin_ia32_sqrtsd", [Sse2], V2Double, [V2Double], V2Double, [V2Double];
    Subsd, "__builtin_ia32_subsd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Ucomisdeq, "__builtin_ia32_ucomisdeq", [Sse2], Int, [V2Double, V2Double], Int, [V2Double, V2Double];
    Ucomisdge, "__builtin_ia32_ucomisdge", [Sse2], Int, [V2Double, V2Double], Int, [V2Double, V2Double];
    Ucomisdgt, "__builtin_ia32_ucomisdgt", [Sse2], Int, [V2Double, V2Double], Int, [V2Double, V2Double];
    Ucomisdle, "__builtin_ia32_ucomisdle", [Sse2], Int, [V2Double, V2Double], Int, [V2Double, V2Double];
    Ucomisdlt, "__builtin_ia32_ucomisdlt", [Sse2], Int, [V2Double, V2Double], Int, [V2Double, V2Double];
    Ucomisdneq, "__builtin_ia32_ucomisdneq", [Sse2], Int, [V2Double, V2Double], Int, [V2Double, V2Double];
    Unpckhpd, "__builtin_ia32_unpckhpd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    Unpcklpd, "__builtin_ia32_unpcklpd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
    VecExtV4si, "__builtin_ia32_vec_ext_v4si", [Sse2], Int, [V4Int, Int], Int, [V4Int, Int];
    VecExtV8hi, "__builtin_ia32_vec_ext_v8hi", [Sse2], Short, [V8Short, Int], Short, [V8Short, Int];
    VecSetV8hi, "__builtin_ia32_vec_set_v8hi", [Sse2], V8Short, [V8Short, Short, Int], V8Short, [V8Short, Short, Int];
    Xorpd, "__builtin_ia32_xorpd", [Sse2], V2Double, [V2Double, V2Double], Unsupported, [];
}
impl X86Intrinsic {
    /// Canonical compiler spelling of this intrinsic.
    pub fn name(self) -> &'static str {
        self.descriptor().name
    }
    /// Whether the intrinsic produces a stable value with unspecified bits.
    /// Each result must remain stable when reused. It must not become LLVM
    /// per-use `undef` or `poison`, and it is not a C arithmetic constant.
    /// Clang 18 implements `Undef128` with zero; that lowering choice is separate
    /// from the source contract exposed here.
    pub fn has_unspecified_result(self) -> bool {
        self == Self::Undef128
    }
    /// Instruction sets needed by evaluated uses. Unevaluated calls do not
    /// themselves require instructions to execute.
    pub fn required_features(self) -> &'static [X86Feature] {
        self.descriptor().features
    }
    /// Returns the target profile's exact signature, or `None` when the
    /// architecture or compiler profile does not provide this spelling.
    /// GCC and Clang differ in several 64-bit vector element types.
    pub fn signature(self, target: Target) -> Option<X86Signature> {
        self.signature_with_profile(toucan_target::CompilerProfile::default_for(target))
    }
    /// Exact source signature under the selected compiler and physical target.
    pub fn signature_with_profile(
        self,
        profile: toucan_target::CompilerProfile,
    ) -> Option<X86Signature> {
        let (result, parameters) = self.prototype_with_profile(profile)?;
        Some(X86Signature {
            result: result.ty(),
            parameters: parameters.iter().map(|parameter| parameter.ty()).collect(),
        })
    }
    /// Whether the spelling has a source signature in this compiler profile.
    pub fn is_available(self, profile: toucan_target::CompilerProfile) -> bool {
        self.prototype_with_profile(profile).is_some()
    }
    fn prototype_with_profile(
        self,
        profile: toucan_target::CompilerProfile,
    ) -> Option<(IntrinsicType, &'static [IntrinsicType])> {
        let target = profile.target();
        let descriptor = self.descriptor();
        match (target, profile.compiler()) {
            (Target::X86_64UnknownLinuxGnu, toucan_target::Compiler::Gnu) => descriptor.gcc,
            (
                Target::X86_64UnknownLinuxGnu
                | Target::X86_64AppleDarwin
                | Target::X86_64PcWindowsMsvc,
                toucan_target::Compiler::Clang,
            ) => descriptor.clang,
            _ => None,
        }
    }
    /// Immediate obligations for this compiler profile. GNU checking may leave
    /// these pending even for known invalid values: its expander checks them only
    /// after inlining. A consumer must discharge them before code generation.
    pub fn immediate_constraints(self, target: Target) -> &'static [ImmediateConstraint] {
        self.immediate_constraints_with_profile(toucan_target::CompilerProfile::default_for(target))
    }
    /// Immediate obligations for the selected compiler, retaining their lowering stage.
    pub fn immediate_constraints_with_profile(
        self,
        profile: toucan_target::CompilerProfile,
    ) -> &'static [ImmediateConstraint] {
        let target = profile.target();
        use ImmediateStage::{AfterInlining, Frontend};
        // These requirements constrain source operands; masking unused selector
        // bits is part of the identified intrinsic's instruction semantics.
        macro_rules! immediate {
            ($argument:expr, $minimum:expr, $maximum:expr, $multiple:expr, $stage:expr) => {
                &[ImmediateConstraint {
                    argument: $argument,
                    minimum: $minimum,
                    maximum: $maximum,
                    multiple_of: $multiple,
                    stage: $stage,
                }]
            };
        }
        match (target, profile.compiler()) {
            (Target::X86_64UnknownLinuxGnu, toucan_target::Compiler::Gnu) => match self {
                Self::VecExtV2si => immediate!(1, 0, 1, 1, AfterInlining),
                Self::VecExtV4hi | Self::VecExtV4si => immediate!(1, 0, 3, 1, AfterInlining),
                Self::VecSetV4hi => immediate!(2, 0, 3, 1, AfterInlining),
                Self::VecExtV8hi => immediate!(1, 0, 7, 1, AfterInlining),
                Self::VecSetV8hi => immediate!(2, 0, 7, 1, AfterInlining),
                Self::Pshufw | Self::Pshufd | Self::Pshufhw | Self::Pshuflw => {
                    immediate!(1, i32::MIN as i64, i32::MAX as i64, 1, AfterInlining)
                }
                Self::Shufps | Self::Shufpd => {
                    immediate!(2, i32::MIN as i64, i32::MAX as i64, 1, AfterInlining)
                }
                Self::Pslldqi128 | Self::Psrldqi128 => immediate!(1, 0, 2040, 8, AfterInlining),
                Self::Prefetch => &[
                    ImmediateConstraint {
                        argument: 1,
                        minimum: i32::MIN as i64,
                        maximum: i32::MAX as i64,
                        multiple_of: 1,
                        stage: AfterInlining,
                    },
                    ImmediateConstraint {
                        argument: 2,
                        minimum: i32::MIN as i64,
                        maximum: i32::MAX as i64,
                        multiple_of: 1,
                        stage: AfterInlining,
                    },
                    ImmediateConstraint {
                        argument: 3,
                        minimum: i32::MIN as i64,
                        maximum: i32::MAX as i64,
                        multiple_of: 1,
                        stage: AfterInlining,
                    },
                ],
                _ => &[],
            },
            (
                Target::X86_64UnknownLinuxGnu
                | Target::X86_64AppleDarwin
                | Target::X86_64PcWindowsMsvc,
                toucan_target::Compiler::Clang,
            ) => match self {
                Self::VecExtV2si => immediate!(1, 0, 1, 1, Frontend),
                Self::VecExtV4hi | Self::VecExtV4si => immediate!(1, 0, 3, 1, Frontend),
                Self::VecSetV4hi => immediate!(2, 0, 3, 1, Frontend),
                Self::VecExtV8hi => immediate!(1, 0, 7, 1, Frontend),
                Self::VecSetV8hi => immediate!(2, 0, 7, 1, Frontend),
                Self::Pshufw => immediate!(1, i8::MIN as i64, i8::MAX as i64, 1, Frontend),
                Self::Pshufd | Self::Pshufhw | Self::Pshuflw => immediate!(1, 0, 255, 1, Frontend),
                Self::Shufps => immediate!(2, 0, 255, 1, Frontend),
                Self::Shufpd => immediate!(2, 0, 3, 1, Frontend),
                _ => &[],
            },
            _ => &[],
        }
    }
    /// Restrictions whose range depends on another immediate operand.
    /// GNU instruction-prefetch hints require locality 2 or 3; other values of
    /// the instruction/data selector use the data-prefetch handling instead.
    pub fn conditional_immediate_constraints(
        self,
        target: Target,
    ) -> &'static [ConditionalImmediateConstraint] {
        self.conditional_immediate_constraints_with_profile(
            toucan_target::CompilerProfile::default_for(target),
        )
    }
    /// Operand-dependent immediate restrictions under the selected compiler.
    pub fn conditional_immediate_constraints_with_profile(
        self,
        profile: toucan_target::CompilerProfile,
    ) -> &'static [ConditionalImmediateConstraint] {
        if self == Self::Prefetch
            && profile.target() == Target::X86_64UnknownLinuxGnu
            && profile.compiler() == toucan_target::Compiler::Gnu
        {
            &[ConditionalImmediateConstraint {
                when_argument: 3,
                when_value: 1,
                requirement: ImmediateConstraint {
                    argument: 2,
                    minimum: 2,
                    maximum: 3,
                    multiple_of: 1,
                    stage: ImmediateStage::AfterInlining,
                },
            }]
        } else {
            &[]
        }
    }
    /// Whether Clang classifies the call itself as having ordinary side effects.
    /// Operand effects remain separate. This compiler classification does not
    /// describe every architectural effect on floating-point register state.
    /// Spellings unavailable in Clang are conservatively treated as effectful.
    pub fn has_side_effects(self) -> bool {
        self.descriptor().clang.is_none()
            || matches!(
                self,
                Self::Emms
                    | Self::Ldmxcsr
                    | Self::Maskmovdqu
                    | Self::Maskmovq
                    | Self::Movmskps
                    | Self::Movntq
                    | Self::Pause
                    | Self::Sfence
                    | Self::Stmxcsr
                    | Self::Clflush
                    | Self::Lfence
                    | Self::Mfence
                    | Self::Movnti
                    | Self::Movnti64
            )
    }
}

impl Analyzer {
    pub(crate) fn x86_call_type(
        &mut self,
        intrinsic: X86Intrinsic,
        call: &Node<ast::CallExpression>,
    ) -> Result<Type, Error> {
        let signature = intrinsic.signature_with_profile(self.unit.profile()?).ok_or_else(|| {
            Error::new(
                call.span.start,
                if matches!(self.unit.target, Target::Aarch64UnknownLinuxGnu | Target::Aarch64AppleDarwin) {
                    "x86 instruction intrinsics require an x86-64 target profile"
                } else { "this x86 intrinsic spelling is unavailable in the selected compiler profile" },
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
        for constraint in intrinsic.immediate_constraints_with_profile(self.unit.profile()?) {
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
            let parameter = self.integer_type(
                &signature.parameters[constraint.argument],
                argument.span.start,
            )?;
            let value = crate::integer::convert(self.eval(argument)?, parameter);
            if value.signed_value() < i128::from(constraint.minimum)
                || value.signed_value() > i128::from(constraint.maximum)
                || value.signed_value() % i128::from(constraint.multiple_of) != 0
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
        self.require_x86_features(intrinsic, call.span.start)?;
        Ok(signature.result)
    }
}

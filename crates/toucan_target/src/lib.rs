//! Explicit target profiles and C object layouts.
//!
//! Layouts are computed for the selected target, independently of the host. Bit offsets and
//! alignment annotations use bits. The layout rules follow the target's default GCC, Clang,
//! or MSVC ABI through `repc`; flags such as `-fshort-enums` are not implied.

mod language;
mod macros;
mod profile;
pub use language::LanguageMode;
pub use profile::{Compiler, CompilerProfile};

use std::fmt;
use std::str::FromStr;

use repc::layout as abi;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A supported C ABI with its default compiler data model.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Target {
    /// The System V AMD64 ABI with GNU/Linux headers.
    X86_64UnknownLinuxGnu,
    /// The System V i386 ABI with GNU/Linux headers.
    I686UnknownLinuxGnu,
    /// The AArch64 ELF ABI with GNU/Linux headers.
    Aarch64UnknownLinuxGnu,
    /// The Intel macOS ABI.
    X86_64AppleDarwin,
    /// The Apple silicon macOS ABI.
    Aarch64AppleDarwin,
    /// The Microsoft x64 ABI.
    X86_64PcWindowsMsvc,
    /// The Microsoft ARM64 ABI.
    Aarch64PcWindowsMsvc,
    /// The System V AMD64 ABI with musl Linux headers.
    X86_64UnknownLinuxMusl,
    /// The AArch64 ELF ABI with musl Linux headers.
    Aarch64UnknownLinuxMusl,
}

impl Target {
    /// All supported targets, in a stable order.
    pub const ALL: [Self; 9] = [
        Self::X86_64UnknownLinuxGnu,
        Self::Aarch64UnknownLinuxGnu,
        Self::X86_64AppleDarwin,
        Self::Aarch64AppleDarwin,
        Self::X86_64PcWindowsMsvc,
        Self::X86_64UnknownLinuxMusl,
        Self::Aarch64UnknownLinuxMusl,
        Self::Aarch64PcWindowsMsvc,
        Self::I686UnknownLinuxGnu,
    ];

    /// Parses a canonical target triple. Unknown triples are rejected.
    pub fn parse(triple: &str) -> Result<Self, LayoutError> {
        Self::ALL
            .into_iter()
            .find(|target| target.triple() == triple)
            .ok_or_else(|| LayoutError::UnsupportedTarget(triple.to_owned()))
    }

    /// Returns the canonical target triple.
    pub const fn triple(self) -> &'static str {
        match self {
            Self::X86_64UnknownLinuxGnu => "x86_64-unknown-linux-gnu",
            Self::I686UnknownLinuxGnu => "i686-unknown-linux-gnu",
            Self::Aarch64UnknownLinuxGnu => "aarch64-unknown-linux-gnu",
            Self::X86_64AppleDarwin => "x86_64-apple-darwin",
            Self::Aarch64AppleDarwin => "aarch64-apple-darwin",
            Self::X86_64PcWindowsMsvc => "x86_64-pc-windows-msvc",
            Self::Aarch64PcWindowsMsvc => "aarch64-pc-windows-msvc",
            Self::X86_64UnknownLinuxMusl => "x86_64-unknown-linux-musl",
            Self::Aarch64UnknownLinuxMusl => "aarch64-unknown-linux-musl",
        }
    }

    /// Whether the target uses a Linux ABI and headers.
    pub const fn is_linux(self) -> bool {
        matches!(
            self,
            Self::X86_64UnknownLinuxGnu
                | Self::I686UnknownLinuxGnu
                | Self::Aarch64UnknownLinuxGnu
                | Self::X86_64UnknownLinuxMusl
                | Self::Aarch64UnknownLinuxMusl
        )
    }

    /// Whether the target uses the musl C library.
    pub const fn is_musl(self) -> bool {
        matches!(
            self,
            Self::X86_64UnknownLinuxMusl | Self::Aarch64UnknownLinuxMusl
        )
    }

    /// Whether the target uses the AArch64 instruction set.
    pub const fn is_aarch64(self) -> bool {
        matches!(
            self,
            Self::Aarch64UnknownLinuxGnu
                | Self::Aarch64UnknownLinuxMusl
                | Self::Aarch64AppleDarwin
                | Self::Aarch64PcWindowsMsvc
        )
    }

    /// Whether the target uses the Microsoft C ABI and Windows headers.
    pub const fn is_windows(self) -> bool {
        matches!(self, Self::X86_64PcWindowsMsvc | Self::Aarch64PcWindowsMsvc)
    }

    /// Whether the target uses the x86-64 instruction set.
    pub const fn is_x86_64(self) -> bool {
        matches!(
            self,
            Self::X86_64UnknownLinuxGnu
                | Self::X86_64UnknownLinuxMusl
                | Self::X86_64AppleDarwin
                | Self::X86_64PcWindowsMsvc
        )
    }

    /// Alignment requested by GNU `aligned` without an argument, in bytes.
    /// These profiles use their default compiler flags, without wider vector ABIs.
    pub const fn default_maximum_alignment(self) -> u32 {
        match self {
            Self::X86_64UnknownLinuxGnu
            | Self::I686UnknownLinuxGnu
            | Self::Aarch64UnknownLinuxGnu
            | Self::X86_64UnknownLinuxMusl
            | Self::Aarch64UnknownLinuxMusl
            | Self::X86_64AppleDarwin
            | Self::Aarch64AppleDarwin
            | Self::X86_64PcWindowsMsvc
            | Self::Aarch64PcWindowsMsvc => 16,
        }
    }

    /// Returns whether plain `char` is signed in this profile.
    pub const fn char_is_signed(self) -> bool {
        !matches!(
            self,
            Self::Aarch64UnknownLinuxGnu | Self::Aarch64UnknownLinuxMusl
        )
    }

    /// Returns the width of object and function pointers, in bits.
    pub const fn pointer_width(self) -> u64 {
        if matches!(self, Self::I686UnknownLinuxGnu) {
            32
        } else {
            64
        }
    }

    /// Returns the width of `long`, in bits.
    pub const fn long_width(self) -> u64 {
        if self.is_windows() || matches!(self, Self::I686UnknownLinuxGnu) {
            32
        } else {
            64
        }
    }

    /// Returns the width of `wchar_t`, in bits.
    pub const fn wchar_width(self) -> u64 {
        if self.is_windows() { 16 } else { 32 }
    }

    /// Returns whether `wchar_t` is signed in this profile.
    pub const fn wchar_is_signed(self) -> bool {
        !matches!(
            self,
            Self::X86_64PcWindowsMsvc
                | Self::Aarch64PcWindowsMsvc
                | Self::Aarch64UnknownLinuxGnu
                | Self::Aarch64UnknownLinuxMusl
        )
    }

    /// Computes an unannotated scalar layout.
    pub fn builtin_layout(self, builtin: BuiltinType) -> Result<Layout, LayoutError> {
        self.layout(&Type::builtin(builtin))
    }

    /// Computes a complete object's layout, rejecting unsupported or invalid input.
    ///
    /// The semantic layer must check C declaration constraints such as flexible array
    /// placement. This function validates layout-specific input and bounds nesting before
    /// calling the ABI engine.
    pub fn layout(self, ty: &Type) -> Result<Layout, LayoutError> {
        CompilerProfile::default_for(self).layout(ty)
    }

    const fn abi_target(self) -> repc::Target {
        match self {
            Self::X86_64UnknownLinuxGnu => repc::Target::X86_64UnknownLinuxGnu,
            Self::I686UnknownLinuxGnu => repc::Target::I686UnknownLinuxGnu,
            Self::Aarch64UnknownLinuxGnu => repc::Target::Aarch64UnknownLinuxGnu,
            Self::X86_64UnknownLinuxMusl => repc::Target::X86_64UnknownLinuxMusl,
            Self::Aarch64UnknownLinuxMusl => repc::Target::Aarch64UnknownLinuxMusl,
            Self::X86_64AppleDarwin => repc::Target::X86_64AppleMacosx,
            Self::Aarch64AppleDarwin => repc::Target::Aarch64AppleMacosx,
            Self::X86_64PcWindowsMsvc => repc::Target::X86_64PcWindowsMsvc,
            Self::Aarch64PcWindowsMsvc => repc::Target::Aarch64PcWindowsMsvc,
        }
    }

    fn lower(
        self,
        ty: &Type,
        depth: usize,
        compiler: Compiler,
    ) -> Result<abi::Type<()>, LayoutError> {
        if depth >= 256 {
            return Err(LayoutError::NestingLimit);
        }
        let annotations = lower_annotations(&ty.annotations)?;
        let variant = match &ty.variant {
            TypeVariant::Builtin(BuiltinType::Void) => return Err(LayoutError::VoidObject),
            TypeVariant::Builtin(BuiltinType::LongDouble) => {
                if !annotations.is_empty() {
                    return Err(LayoutError::AnnotatedLongDouble);
                }
                let (size_bits, alignment_bits) = match self {
                    Self::Aarch64AppleDarwin
                    | Self::X86_64PcWindowsMsvc
                    | Self::Aarch64PcWindowsMsvc => (64, 64),
                    Self::I686UnknownLinuxGnu => (96, 32),
                    _ => (128, 128),
                };
                abi::TypeVariant::Opaque(abi::TypeLayout {
                    size_bits,
                    field_alignment_bits: alignment_bits,
                    pointer_alignment_bits: alignment_bits,
                    required_alignment_bits: 8,
                })
            }
            TypeVariant::Builtin(builtin @ (BuiltinType::Int128 | BuiltinType::UnsignedInt128))
                if self == Self::I686UnknownLinuxGnu =>
            {
                return Err(LayoutError::UnsupportedBuiltin {
                    target: self,
                    builtin: *builtin,
                });
            }
            TypeVariant::Builtin(builtin) => abi::TypeVariant::Builtin(builtin.to_abi()?),
            TypeVariant::Record(record) => {
                let fields = record
                    .fields
                    .iter()
                    .map(|field| {
                        if field.bit_width.is_some() && !field.ty.is_integer() {
                            return Err(LayoutError::NonIntegerBitfield);
                        }
                        if matches!(field.ty.integer_builtin(), Some(BuiltinType::Bool))
                            && field.bit_width.is_some_and(|width| width > 1)
                        {
                            return Err(LayoutError::BooleanBitfieldWidth);
                        }
                        Ok(abi::RecordField {
                            layout: None,
                            annotations: lower_annotations(&field.annotations)?,
                            named: field.named,
                            bit_width: field.bit_width,
                            ty: self.lower(&field.ty, depth + 1, compiler)?,
                        })
                    })
                    .collect::<Result<_, LayoutError>>()?;
                abi::TypeVariant::Record(abi::Record {
                    kind: match record.kind {
                        RecordKind::Struct => abi::RecordKind::Struct,
                        RecordKind::Union => abi::RecordKind::Union,
                    },
                    fields,
                })
            }
            TypeVariant::Array { element, length } => abi::TypeVariant::Array(abi::Array {
                element_type: Box::new(self.lower(element, depth + 1, compiler)?),
                num_elements: *length,
            }),
            TypeVariant::Enum(values) => {
                let (minimum, maximum) =
                    values.iter().fold((0, 0), |(minimum, maximum), &value| {
                        (minimum.min(value), maximum.max(value))
                    });
                let signed = minimum < 0;
                let maximum_64 = if signed {
                    i128::from(i64::MAX)
                } else {
                    i128::from(u64::MAX)
                };
                if ((compiler == Compiler::Clang && !self.is_windows())
                    || self == Self::I686UnknownLinuxGnu)
                    && (minimum < i128::from(i64::MIN) || maximum > maximum_64)
                {
                    // Neither i686 compiler supports 128-bit enum types. Clang on
                    // the other non-MSVC targets diagnoses lossy recovery.
                    return Err(LayoutError::UnsupportedEnumRange(self));
                }
                let mut values = values.clone();
                if signed {
                    // repc counts positive values without a sign bit, even when another
                    // variant is negative. The complement forces it to reserve that bit
                    // for the largest positive value. Keep this an enum so its alignment
                    // annotations and its use as a bitfield retain their ABI rules.
                    values.push(!maximum);
                }
                abi::TypeVariant::Enum(values)
            }
            TypeVariant::Opaque(layout) => abi::TypeVariant::Opaque(abi::TypeLayout {
                size_bits: layout.size_bits,
                pointer_alignment_bits: layout.alignment_bits,
                field_alignment_bits: layout.field_alignment_bits,
                required_alignment_bits: layout.required_alignment_bits,
            }),
            TypeVariant::Typedef(inner) => {
                abi::TypeVariant::Typedef(Box::new(self.lower(inner, depth + 1, compiler)?))
            }
        };
        Ok(abi::Type {
            layout: (),
            annotations,
            variant,
        })
    }
}

impl FromStr for Target {
    type Err = LayoutError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for Target {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.triple())
    }
}

/// A C type containing only the information needed to compute its object layout.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Type {
    /// Layout annotations applied to the type.
    pub annotations: Vec<Annotation>,
    /// The type's structure.
    pub variant: TypeVariant,
}

impl Type {
    /// Reuses a complete object's layout without expanding its nested fields.
    ///
    /// This representation preserves object and field alignment, including the
    /// alignment that remains required under MSVC packing. It cannot be used as
    /// an integer bitfield type or queried for the original object's fields.
    pub fn opaque_layout(layout: &Layout) -> Self {
        Self {
            annotations: Vec::new(),
            variant: TypeVariant::Opaque(ObjectLayout {
                size_bits: layout.size_bits,
                alignment_bits: layout.alignment_bits,
                field_alignment_bits: layout.field_alignment_bits,
                required_alignment_bits: layout.required_alignment_bits,
            }),
        }
    }

    /// Creates an unannotated scalar type.
    pub fn builtin(builtin: BuiltinType) -> Self {
        Self {
            annotations: Vec::new(),
            variant: TypeVariant::Builtin(builtin),
        }
    }

    /// Creates an object or function pointer layout type.
    pub fn pointer() -> Self {
        Self::builtin(BuiltinType::Pointer)
    }

    fn integer_builtin(&self) -> Option<BuiltinType> {
        let mut current = self;
        for _ in 0..256 {
            match &current.variant {
                TypeVariant::Builtin(builtin) if builtin.is_integer() => return Some(*builtin),
                TypeVariant::Typedef(inner) => current = inner,
                _ => return None,
            }
        }
        None
    }

    fn is_integer(&self) -> bool {
        let mut current = self;
        for _ in 0..256 {
            match &current.variant {
                TypeVariant::Enum(_) => return true,
                TypeVariant::Builtin(builtin) => return builtin.is_integer(),
                TypeVariant::Typedef(inner) => current = inner,
                _ => return false,
            }
        }
        false
    }
}

/// The structural alternatives understood by the object layout engine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TypeVariant {
    /// A C scalar or pointer.
    Builtin(BuiltinType),
    /// A structure or union.
    Record(Record),
    /// An array; `None` represents a flexible array member.
    Array {
        element: Box<Type>,
        length: Option<u64>,
    },
    /// An enumeration, carrying its constant values for integer representation selection.
    Enum(Vec<i128>),
    /// A typedef, which may carry independent alignment annotations.
    Typedef(Box<Type>),
    /// An already computed complete object layout; construct with [`Type::opaque_layout`].
    Opaque(ObjectLayout),
}

/// Compact dimensions of a computed object, excluding its nested field layouts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObjectLayout {
    size_bits: u64,
    alignment_bits: u64,
    field_alignment_bits: u64,
    required_alignment_bits: u64,
}

/// C scalar types. `Void` has no object layout; pointers have a target-specific layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BuiltinType {
    Void,
    Bool,
    Char,
    SignedChar,
    UnsignedChar,
    Short,
    UnsignedShort,
    Int,
    UnsignedInt,
    Long,
    UnsignedLong,
    LongLong,
    UnsignedLongLong,
    Int128,
    UnsignedInt128,
    Float,
    Double,
    LongDouble,
    Pointer,
}

impl BuiltinType {
    /// Returns whether the type is a C integer type, including `_Bool`.
    pub const fn is_integer(self) -> bool {
        !matches!(
            self,
            Self::Void | Self::Float | Self::Double | Self::LongDouble | Self::Pointer
        )
    }

    fn to_abi(self) -> Result<abi::BuiltinType, LayoutError> {
        Ok(match self {
            Self::Void => return Err(LayoutError::VoidObject),
            Self::LongDouble => return Err(LayoutError::AnnotatedLongDouble),
            Self::Bool => abi::BuiltinType::Bool,
            Self::Char => abi::BuiltinType::Char,
            Self::SignedChar => abi::BuiltinType::SignedChar,
            Self::UnsignedChar => abi::BuiltinType::UnsignedChar,
            Self::Short => abi::BuiltinType::Short,
            Self::UnsignedShort => abi::BuiltinType::UnsignedShort,
            Self::Int => abi::BuiltinType::Int,
            Self::UnsignedInt => abi::BuiltinType::UnsignedInt,
            Self::Long => abi::BuiltinType::Long,
            Self::UnsignedLong => abi::BuiltinType::UnsignedLong,
            Self::LongLong => abi::BuiltinType::LongLong,
            Self::UnsignedLongLong => abi::BuiltinType::UnsignedLongLong,
            Self::Int128 => abi::BuiltinType::I128,
            Self::UnsignedInt128 => abi::BuiltinType::U128,
            Self::Float => abi::BuiltinType::Float,
            Self::Double => abi::BuiltinType::Double,
            Self::Pointer => abi::BuiltinType::Pointer,
        })
    }
}

/// A structure or union's ordered fields.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub kind: RecordKind,
    pub fields: Vec<Field>,
}

/// The distinction between sequential and overlapping fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RecordKind {
    Struct,
    Union,
}

/// A field, including optional bit width and field-specific annotations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub ty: Type,
    pub annotations: Vec<Annotation>,
    /// False only for unnamed bitfields; anonymous record members still occupy a field.
    pub named: bool,
    pub bit_width: Option<u64>,
}

/// ABI layout annotations. All numeric arguments are in bits, not bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Annotation {
    /// GNU `__attribute__((packed))`; MSVC targets follow clang-cl's interpretation.
    Packed,
    /// Explicit alignment, or the target's default maximum when no argument was given.
    Align(Option<u64>),
    /// The active `#pragma pack` maximum member alignment.
    PragmaPack(u64),
}

fn lower_annotations(annotations: &[Annotation]) -> Result<Vec<abi::Annotation>, LayoutError> {
    annotations
        .iter()
        .map(|annotation| {
            Ok(match annotation {
                Annotation::Packed => abi::Annotation::AttrPacked,
                Annotation::Align(value) => abi::Annotation::Align(*value),
                Annotation::PragmaPack(bits) => {
                    // repc ignores invalid values; rejecting them prevents accidental ABI guesses.
                    if !matches!(bits, 8 | 16 | 32 | 64 | 128) {
                        return Err(LayoutError::InvalidPragmaPack(*bits));
                    }
                    abi::Annotation::PragmaPack(*bits)
                }
            })
        })
        .collect()
}

/// A computed object layout. Bitfields retain exact bit offsets and widths.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub size_bits: u64,
    /// Alignment of valid pointers, corresponding to Rust's `align_of`.
    pub alignment_bits: u64,
    /// Alignment when embedded as a field, including typedef effects.
    pub field_alignment_bits: u64,
    /// Alignment that remains required when applying MSVC packing.
    pub required_alignment_bits: u64,
    /// Ordered record fields; unnamed bitfields have no addressable layout.
    pub fields: Vec<Option<FieldLayout>>,
}

impl Layout {
    /// Returns the object size in bytes.
    pub const fn size_bytes(&self) -> u64 {
        self.size_bits / 8
    }

    /// Returns the pointer alignment in bytes.
    pub const fn alignment_bytes(&self) -> u64 {
        self.alignment_bits / 8
    }

    fn from_abi(ty: &abi::Type<abi::TypeLayout>) -> Self {
        let mut fields_type = ty;
        while let abi::TypeVariant::Typedef(inner) = &fields_type.variant {
            fields_type = inner;
        }
        let fields = match &fields_type.variant {
            abi::TypeVariant::Record(record) => record
                .fields
                .iter()
                .map(|field| {
                    field.layout.map(|layout| FieldLayout {
                        offset_bits: layout.offset_bits,
                        size_bits: layout.size_bits,
                    })
                })
                .collect(),
            _ => Vec::new(),
        };
        Self {
            size_bits: ty.layout.size_bits,
            alignment_bits: ty.layout.pointer_alignment_bits,
            field_alignment_bits: ty.layout.field_alignment_bits,
            required_alignment_bits: ty.layout.required_alignment_bits,
            fields,
        }
    }
}

/// A named field's offset and size, in bits from the start of its containing object.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FieldLayout {
    pub offset_bits: u64,
    pub size_bits: u64,
}

/// A target selection or object layout failure.
#[derive(Debug, Error)]
pub enum LayoutError {
    #[error(
        "unsupported C language mode `{0}`; expected c90, gnu90, c99, gnu99, c11, gnu11, c17, or gnu17"
    )]
    UnsupportedLanguageModeName(String),
    #[error("unsupported compiler `{0}`; expected gcc or clang")]
    UnsupportedCompilerName(String),
    #[error("compiler `{compiler}` is unsupported for target `{target}`")]
    UnsupportedCompiler { target: Target, compiler: Compiler },
    #[error("unsupported target `{0}`")]
    UnsupportedTarget(String),
    #[error("void has no object layout")]
    VoidObject,
    #[error("annotate a typedef when specifying alignment for long double")]
    AnnotatedLongDouble,
    #[error("{builtin:?} is unsupported for target `{target}`")]
    UnsupportedBuiltin {
        target: Target,
        builtin: BuiltinType,
    },
    #[error("enumeration value range is unsupported for target `{0}`")]
    UnsupportedEnumRange(Target),
    #[error("type nesting exceeds the layout limit of 256")]
    NestingLimit,
    #[error("bitfields must have integer or enumeration types")]
    NonIntegerBitfield,
    #[error("a _Bool bitfield cannot exceed one bit")]
    BooleanBitfieldWidth,
    #[error("invalid pragma pack alignment {0} bits; expected 8, 16, 32, 64, or 128")]
    InvalidPragmaPack(u64),
    #[error(transparent)]
    Abi(#[from] repc::Error),
}

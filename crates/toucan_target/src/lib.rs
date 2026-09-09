//! Explicit target profiles and C object layouts.
//!
//! Layouts are computed for the selected target, independently of the host. Bit offsets and
//! alignment annotations use bits. The layout rules follow the target's default GCC, Clang,
//! or MSVC ABI through `repc`; flags such as `-fshort-enums` are not implied.

mod macros;

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
    /// The AArch64 ELF ABI with GNU/Linux headers.
    Aarch64UnknownLinuxGnu,
    /// The Intel macOS ABI.
    X86_64AppleDarwin,
    /// The Apple silicon macOS ABI.
    Aarch64AppleDarwin,
    /// The Microsoft x64 ABI.
    X86_64PcWindowsMsvc,
}

impl Target {
    /// All supported targets, in a stable order.
    pub const ALL: [Self; 5] = [
        Self::X86_64UnknownLinuxGnu,
        Self::Aarch64UnknownLinuxGnu,
        Self::X86_64AppleDarwin,
        Self::Aarch64AppleDarwin,
        Self::X86_64PcWindowsMsvc,
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
            Self::Aarch64UnknownLinuxGnu => "aarch64-unknown-linux-gnu",
            Self::X86_64AppleDarwin => "x86_64-apple-darwin",
            Self::Aarch64AppleDarwin => "aarch64-apple-darwin",
            Self::X86_64PcWindowsMsvc => "x86_64-pc-windows-msvc",
        }
    }

    /// Alignment requested by GNU `aligned` without an argument, in bytes.
    /// These profiles use their default compiler flags, without wider vector ABIs.
    pub const fn default_maximum_alignment(self) -> u32 {
        match self {
            Self::X86_64UnknownLinuxGnu
            | Self::Aarch64UnknownLinuxGnu
            | Self::X86_64AppleDarwin
            | Self::Aarch64AppleDarwin
            | Self::X86_64PcWindowsMsvc => 16,
        }
    }

    /// Returns whether plain `char` is signed in this profile.
    pub const fn char_is_signed(self) -> bool {
        !matches!(self, Self::Aarch64UnknownLinuxGnu)
    }

    /// Returns the width of object and function pointers, in bits.
    pub const fn pointer_width(self) -> u64 {
        64
    }

    /// Returns the width of `long`, in bits.
    pub const fn long_width(self) -> u64 {
        if matches!(self, Self::X86_64PcWindowsMsvc) {
            32
        } else {
            64
        }
    }

    /// Returns the width of `wchar_t`, in bits.
    pub const fn wchar_width(self) -> u64 {
        if matches!(self, Self::X86_64PcWindowsMsvc) {
            16
        } else {
            32
        }
    }

    /// Returns whether `wchar_t` is signed in this profile.
    pub const fn wchar_is_signed(self) -> bool {
        !matches!(
            self,
            Self::X86_64PcWindowsMsvc | Self::Aarch64UnknownLinuxGnu
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
        let input = self.lower(ty, 0)?;
        let output = repc::compute_layout(self.abi_target(), &input)?;
        Ok(Layout::from_abi(&output))
    }

    const fn abi_target(self) -> repc::Target {
        match self {
            Self::X86_64UnknownLinuxGnu => repc::Target::X86_64UnknownLinuxGnu,
            Self::Aarch64UnknownLinuxGnu => repc::Target::Aarch64UnknownLinuxGnu,
            Self::X86_64AppleDarwin => repc::Target::X86_64AppleMacosx,
            Self::Aarch64AppleDarwin => repc::Target::Aarch64AppleMacosx,
            Self::X86_64PcWindowsMsvc => repc::Target::X86_64PcWindowsMsvc,
        }
    }

    fn lower(self, ty: &Type, depth: usize) -> Result<abi::Type<()>, LayoutError> {
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
                let bits = if matches!(self, Self::Aarch64AppleDarwin | Self::X86_64PcWindowsMsvc) {
                    64
                } else {
                    128
                };
                abi::TypeVariant::Opaque(abi::TypeLayout {
                    size_bits: bits,
                    field_alignment_bits: bits,
                    pointer_alignment_bits: bits,
                    required_alignment_bits: 8,
                })
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
                            ty: self.lower(&field.ty, depth + 1)?,
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
                element_type: Box::new(self.lower(element, depth + 1)?),
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
                if matches!(self, Self::X86_64AppleDarwin | Self::Aarch64AppleDarwin)
                    && (minimum < i128::from(i64::MIN) || maximum > maximum_64)
                {
                    // Clang only offers a lossy, diagnosed recovery for larger enum ranges.
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
                abi::TypeVariant::Typedef(Box::new(self.lower(inner, depth + 1)?))
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

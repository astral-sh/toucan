use std::collections::{BTreeMap, HashMap, HashSet};
use std::num::NonZeroU32;

use serde::{Serialize, Serializer, ser::SerializeStruct};
use toucan_target::{self as target, Target};

use crate::Error;

/// Declarations and canonical tag identities in a preprocessed translation unit.
#[derive(Clone, Debug, Serialize)]
pub struct TranslationUnit {
    pub target: Target,
    /// Compiler behavior retained independently from the physical target.
    pub compiler: target::Compiler,
    /// Source language mode retained for all later expression parsing.
    pub language_mode: target::LanguageMode,
    pub declarations: Vec<Declaration>,
    /// Sparse per-function compilation properties, keyed by declaration index.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub function_options: BTreeMap<usize, crate::FunctionOptions>,
    /// Owner-local sparse parameter contracts used by function types.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameter_contracts: Vec<crate::ParameterContracts>,
    /// Sparse type ancestry used by Clang's common-type alignment rules.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alignment_origins: Vec<crate::AlignmentOrigin>,
    pub records: Vec<Record>,
    /// Nominal GNU typedef variants mapped directly to their source record.
    /// Field declaration identities belong to the source record.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub record_origins: BTreeMap<usize, usize>,
    /// Lexical record containment and source ordering, independent of C scope.
    #[serde(skip_serializing_if = "crate::TagLexicalOrigins::is_empty")]
    pub lexical_tags: crate::TagLexicalOrigins,
    /// Sparse header-cursor discovery facts for enum-expression descendants.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_discovery: Option<Box<crate::TagDiscoveries>>,
    pub enums: Vec<Enum>,
    pub typedefs: BTreeMap<String, Type>,
    pub constants: BTreeMap<String, IntegerValue>,
}

/// A qualified C type. Typedefs and tags retain their declaration identities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Hash)]
pub struct Type {
    /// Typedef alignment and its owner-local ancestry. This does not change C
    /// type compatibility or the canonical layout of a referenced record tag.
    #[serde(flatten)]
    pub alignment: crate::TypeAlignment,
    pub kind: TypeKind,
    pub qualifiers: Qualifiers,
}

impl Type {
    /// Constructs an unqualified type.
    pub fn new(kind: TypeKind) -> Self {
        Self {
            alignment: crate::TypeAlignment::default(),
            kind,
            qualifiers: Qualifiers::default(),
        }
    }

    /// Constructs a pointer to this type, preserving the pointee's qualifiers.
    pub fn pointer(self) -> Self {
        Self::new(TypeKind::Pointer(Box::new(self)))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Qualifiers {
    pub is_const: bool,
    pub is_volatile: bool,
    pub is_restrict: bool,
    /// Keep both Microsoft qualifiers in the fourth byte of every C type.
    microsoft_flags: u8,
}

impl Qualifiers {
    const UNALIGNED: u8 = 1;
    const MSVC_PTR32: u8 = 2;

    /// Microsoft unaligned accesses retain type identity without packing fields.
    pub fn is_unaligned(self) -> bool {
        self.microsoft_flags & Self::UNALIGNED != 0
    }

    /// Set the Microsoft unaligned qualifier without changing pointer width.
    pub fn set_unaligned(&mut self, enabled: bool) {
        self.set_microsoft_flag(Self::UNALIGNED, enabled);
    }

    /// Microsoft `__ptr32` retains distinct pointer identity on Windows.
    pub fn is_msvc_ptr32(self) -> bool {
        self.microsoft_flags & Self::MSVC_PTR32 != 0
    }

    /// Set the Microsoft pointer-width qualifier; layout depends on the target.
    pub fn set_msvc_ptr32(&mut self, enabled: bool) {
        self.set_microsoft_flag(Self::MSVC_PTR32, enabled);
    }

    fn set_microsoft_flag(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.microsoft_flags |= flag;
        } else {
            self.microsoft_flags &= !flag;
        }
    }
}

impl Serialize for Qualifiers {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut fields = serializer.serialize_struct(
            "Qualifiers",
            3 + usize::from(self.is_unaligned()) + usize::from(self.is_msvc_ptr32()),
        )?;
        fields.serialize_field("is_const", &self.is_const)?;
        fields.serialize_field("is_volatile", &self.is_volatile)?;
        fields.serialize_field("is_restrict", &self.is_restrict)?;
        if self.is_unaligned() {
            fields.serialize_field("is_unaligned", &true)?;
        }
        if self.is_msvc_ptr32() {
            fields.serialize_field("is_msvc_ptr32", &true)?;
        }
        fields.end()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Hash)]
pub enum TypeKind {
    Void,
    Bool,
    Integer(IntegerKind),
    Float(FloatKind),
    /// A C complex scalar, with two components of the corresponding real type.
    Complex(FloatKind),
    Pointer(Box<Type>),
    /// C11 atomic object type. Storage layout and observable accesses differ
    /// from the contained non-atomic value; outer qualifiers remain independent.
    Atomic(Box<Type>),
    /// GNU fixed-size vector. Elements are unqualified integer or floating types;
    /// the vector retains its own qualifiers and does not decay to a pointer.
    Vector {
        element: Box<Type>,
        lanes: u64,
        /// Native NEON types have distinct C identity despite equal lane storage.
        #[serde(skip_serializing_if = "VectorKind::is_gnu")]
        kind: VectorKind,
    },
    /// An opaque AArch64 SVE vector or predicate, with no fixed size or alignment.
    Sve(SveKind),
    Array {
        element: Box<Type>,
        length: Option<u64>,
    },
    /// A complete array whose extent is determined at runtime. Unlike an
    /// incomplete array, its extent cannot be completed by a later declaration.
    VariableArray {
        element: Box<Type>,
        /// Exact array-type identity; ordinary C compatibility ignores it.
        identity: crate::VariableArrayId,
    },
    Function(Box<FunctionType>),
    Record(usize),
    Enum(usize),
    Typedef(String),
}

/// C identity of a fixed vector, independent of its element and lane layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Hash)]
pub enum VectorKind {
    #[default]
    Gnu,
    /// GCC's predefined AArch64 Advanced SIMD types.
    Neon,
}
impl VectorKind {
    fn is_gnu(&self) -> bool {
        *self == Self::Gnu
    }
}

/// Sizeless SVE types available without a target vector-length assumption.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Hash)]
pub enum SveKind {
    Float32,
    Float64,
    /// One predicate bit per vector byte, not a vector of C Boolean objects.
    Predicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Hash)]
pub enum IntegerKind {
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Hash)]
pub enum FloatKind {
    /// The distinct GNU/Clang bfloat16 type, with eight significant bits.
    BFloat16,
    Float,
    Double,
    LongDouble,
    Extended {
        format: ExtendedFloatFormat,
        width: usize,
    },
}

/// The target encoding of a floating constant, without object padding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum FloatingFormat {
    /// IEEE binary16: five exponent bits and eleven significant bits.
    Binary16,
    /// Bfloat16: eight exponent bits and eight significant bits.
    BFloat16,
    Binary32,
    Binary64,
    /// The 80 meaningful bits of the x87 extended format, including its explicit integer bit.
    X87,
    Binary128,
}

/// An owned C floating value. Operations have already rounded in the target
/// format; the bits retain signed zero, subnormals, infinities, and NaN payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct FloatingValue {
    pub(crate) kind: FloatKind,
    pub(crate) format: FloatingFormat,
    pub(crate) bits: u128,
}

impl FloatingValue {
    /// Return the C type, retaining `long double` even on binary64 targets.
    pub const fn kind(self) -> FloatKind {
        self.kind
    }

    /// Return the target encoding independently of the C type's storage padding.
    pub const fn format(self) -> FloatingFormat {
        self.format
    }

    /// Return the encoding in the low bits, independent of target byte order.
    pub const fn to_bits(self) -> u128 {
        self.bits
    }
}

/// An owned complex constant with corresponding-real target encodings.
/// The real component precedes the imaginary component in C object storage;
/// these encodings exclude padding within either component.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ComplexValue {
    pub(crate) kind: FloatKind,
    pub(crate) format: FloatingFormat,
    pub(crate) real_bits: u128,
    pub(crate) imaginary_bits: u128,
}

impl ComplexValue {
    /// Return the common C type of the two real components.
    pub const fn kind(self) -> FloatKind {
        self.kind
    }

    /// Return the target encoding of either component, excluding object padding.
    pub const fn format(self) -> FloatingFormat {
        self.format
    }

    /// Return the real component without changing its target representation.
    pub const fn real(self) -> FloatingValue {
        FloatingValue {
            kind: self.kind,
            format: self.format,
            bits: self.real_bits,
        }
    }

    /// Return the imaginary component without changing its target representation.
    pub const fn imaginary(self) -> FloatingValue {
        FloatingValue {
            kind: self.kind,
            format: self.format,
            bits: self.imaginary_bits,
        }
    }
}

/// The result of arithmetic constant evaluation. This query admits floating
/// operands; an integer result does not imply a C integer constant expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ArithmeticConstant {
    Integer(IntegerValue),
    Floating(FloatingValue),
    Complex(ComplexValue),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Hash)]
pub enum ExtendedFloatFormat {
    BinaryInterchange,
    BinaryExtended,
    DecimalInterchange,
    DecimalExtended,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Hash)]
pub struct FunctionType {
    /// Clang function-type promise from GNU `noreturn`, separate from C11 `_Noreturn`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub noreturn: bool,
    /// Effective Clang parameter promises; absence means no such promises.
    /// Retained body signatures also use this for identifier-list entry parameters,
    /// without making their callable declarations into prototypes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_contracts: Option<crate::ParameterContractsId>,
    pub return_type: Type,
    pub parameters: Vec<Parameter>,
    pub variadic: bool,
    /// False for the C11 non-prototype declaration `f()`.
    pub prototype: bool,
    pub calling_convention: CallingConvention,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Hash)]
pub enum CallingConvention {
    /// The target's default C convention, with no explicit ABI attribute.
    C,
    /// An explicitly requested System V x86-64 convention.
    SysV64,
    /// An explicitly requested Microsoft x64 convention.
    Win64,
    /// Preserves the additional AArch64 Advanced SIMD registers.
    Aarch64Vector,
    /// Clang's explicit SVE register-preservation convention.
    Aarch64Sve,
}

impl CallingConvention {
    /// Normalizes an explicit convention that matches the target's default.
    /// Retaining the original value in the type preserves declaration attributes.
    pub fn for_target(self, target: Target) -> Result<Self, Error> {
        match (self, target) {
            (Self::C, _) => Ok(Self::C),
            (
                Self::Aarch64Vector | Self::Aarch64Sve,
                Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
                | Target::Aarch64AppleDarwin,
            ) => Ok(self),
            (
                Self::SysV64,
                Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::X86_64AppleDarwin,
            )
            | (Self::Win64, Target::X86_64PcWindowsMsvc) => Ok(Self::C),
            (Self::SysV64, Target::X86_64PcWindowsMsvc)
            | (
                Self::Win64,
                Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::X86_64AppleDarwin,
            ) => Ok(self),
            (Self::Win64, Target::Aarch64PcWindowsMsvc) => Ok(Self::C),
            _ => Err(Error::new(
                0,
                "explicit calling convention is unsupported on this target",
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Hash)]
pub struct Parameter {
    pub name: Option<String>,
    pub ty: Type,
}

/// The scope in which a tag was introduced. Prototype and block tags retain their
/// type identities, but their names are unavailable at file scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Scope {
    File,
    Prototype,
    Block,
}

#[derive(Clone, Debug, Serialize)]
pub struct Record {
    /// Whether the canonical union type carries GNU transparent argument passing.
    #[serde(skip_serializing_if = "is_false")]
    pub transparent_union: bool,
    pub name: Option<String>,
    pub scope: Scope,
    pub kind: RecordKind,
    /// None denotes an incomplete declaration; Some([]) is an empty GNU record.
    pub fields: Option<Vec<Field>>,
    pub packed: bool,
    /// Explicit alignment in bytes.
    pub alignment: Option<u64>,
    /// Maximum field alignment in bytes from the active pack pragma.
    pub pack: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum RecordKind {
    Struct,
    Union,
}

#[derive(Clone, Debug, Serialize)]
pub struct Field {
    pub name: Option<String>,
    pub ty: Type,
    pub bit_width: Option<u64>,
    pub alignment: Option<u64>,
    pub packed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Enum {
    /// Canonical tag packing. The MSVC ABI retains int representation.
    #[serde(skip_serializing_if = "is_false")]
    pub packed: bool,
    pub name: Option<String>,
    pub scope: Scope,
    /// Whether the closing brace of the definition has been reached.
    pub complete: bool,
    pub variants: Vec<EnumVariant>,
}

#[derive(Clone, Debug, Serialize)]
pub struct EnumVariant {
    pub name: String,
    pub value: IntegerValue,
}

/// Object-file binding of a declaration with external C linkage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum SymbolBinding {
    #[default]
    Strong,
    /// Definitions may be replaced; an unresolved declaration may have a null address.
    Weak,
}
impl SymbolBinding {
    pub(crate) fn is_strong(&self) -> bool {
        *self == Self::Strong
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Declaration {
    /// Summary of file-scope inline occurrences, separate from final body ownership.
    /// `None` means no recorded inline history applies to this declaration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_facts: Option<crate::FunctionInlineFacts>,
    /// Ownership of a function body, independently of `is_definition`, which
    /// continues to indicate a checked body or an initialized object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_definition_kind: Option<crate::FunctionDefinitionKind>,
    /// DLL storage visible at the final file declaration. This does not identify
    /// a library, change the C type, or describe earlier emitted references.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dll_storage_class: Option<crate::DllStorageClass>,
    /// Explicit object/function alignment, independent of the declared C type.
    #[serde(skip_serializing_if = "crate::DeclarationAlignment::is_empty")]
    pub alignment: crate::DeclarationAlignment,
    /// Whether a compatible declaration says this function may return more than once.
    /// This is not a function-pointer type qualifier or a retroactive call-site verdict.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub returns_twice: bool,
    /// Promise visible in the final file declaration. Earlier declarations and
    /// calls keep their own facts in checked code; this does not change its type.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub noreturn: bool,
    /// Symbol binding; applies to externally linked functions and objects.
    #[serde(skip_serializing_if = "SymbolBinding::is_strong")]
    pub symbol_binding: SymbolBinding,
    pub name: String,
    pub ty: Type,
    pub kind: DeclarationKind,
    pub link_name: Option<String>,
    /// Whether this declaration has internal linkage.
    pub is_static: bool,
    /// Whether each thread owns a distinct object; independent of its linkage.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_thread_local: bool,
    pub is_definition: bool,
    /// Storage allocated by GNU initialization of this object's flexible member.
    /// This does not change its declared type, record layout, or `sizeof` result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flexible_array_storage: Option<FlexibleArrayStorage>,
}

/// Additional object storage for an initialized flexible array member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FlexibleArrayStorage {
    pub member_index: usize,
    pub elements: u64,
    /// Total allocated storage, including the record prefix and compiler padding.
    pub size_bits: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum DeclarationKind {
    Typedef,
    Function,
    Variable,
}

/// A C integer with its target width, signedness, and conversion rank.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct IntegerValue {
    /// The low `bits` bits of the value, including the sign bit for signed values.
    pub value: u128,
    pub bits: u8,
    pub signed: bool,
    /// C ranks: bool=0, char=1, short=2, int=3, long=4, long long=5, int128=6.
    pub rank: u8,
}

impl IntegerValue {
    /// Returns the sign-extended integer value.
    pub fn signed_value(self) -> i128 {
        if self.signed
            && self.bits > 0
            && self.bits < 128
            && self.value & (1u128 << (self.bits - 1)) != 0
        {
            (self.value | (!0u128 << self.bits)) as i128
        } else {
            self.value as i128
        }
    }

    /// Converts a nonnegative integer into a layout size or array count.
    pub fn as_u64(self) -> Result<u64, Error> {
        self.validate()?;
        if self.signed && self.signed_value() < 0 {
            return Err(Error::new(0, "expected a nonnegative integer"));
        }
        u64::try_from(self.value).map_err(|_| Error::new(0, "integer does not fit in 64 bits"))
    }

    /// Returns whether this value is representable in the target profiles' 32-bit `int`.
    pub(crate) fn fits_int(self) -> bool {
        if self.signed {
            i32::try_from(self.signed_value()).is_ok()
        } else {
            self.value <= i32::MAX as u128
        }
    }

    pub(crate) fn new(value: u128, bits: u8, signed: bool, rank: u8) -> Self {
        Self {
            value: value & Self::mask(bits),
            bits,
            signed,
            rank,
        }
    }

    pub(crate) fn validate(self) -> Result<(), Error> {
        if !matches!(self.bits, 8 | 16 | 32 | 64 | 128)
            || self.rank > 6
            || self.value > Self::mask(self.bits)
        {
            return Err(Error::new(0, "invalid integer value representation"));
        }
        Ok(())
    }

    pub(crate) fn mask(bits: u8) -> u128 {
        if bits == 128 {
            u128::MAX
        } else {
            (1u128 << bits) - 1
        }
    }
    pub(crate) fn int(value: i128) -> Self {
        Self::new(value as u128, 32, true, 3)
    }
    pub(crate) fn truth(self) -> bool {
        self.value != 0
    }
}

impl TranslationUnit {
    /// Validates compiler identity when callers construct or modify public IR.
    pub fn profile(&self) -> Result<target::CompilerProfile, Error> {
        target::CompilerProfile::new(self.target, self.compiler)
            .map(|profile| profile.with_language_mode(self.language_mode))
            .map_err(|e| Error::new(0, e.to_string()))
    }

    /// Whether this array's size depends on a runtime bound. Pointers to such
    /// arrays still have a constant pointer size.
    pub fn is_variable_length_array(&self, ty: &Type) -> Result<bool, Error> {
        let mut ty = ty;
        for _ in 0..128 {
            match &self.resolve(ty)?.kind {
                TypeKind::VariableArray { .. } => return Ok(true),
                TypeKind::Array { element, .. } => ty = element,
                _ => return Ok(false),
            }
        }
        Err(Error::new(
            0,
            "array type nesting exceeds the 128-level limit",
        ))
    }

    /// Whether an array bound contributes a variably modified type. Function
    /// parameter types do not make the containing function variably modified.
    pub fn is_variably_modified(&self, ty: &Type) -> Result<bool, Error> {
        let mut ty = ty;
        for _ in 0..128 {
            match &self.resolve(ty)?.kind {
                TypeKind::VariableArray { .. } => return Ok(true),
                TypeKind::Array { element, .. }
                | TypeKind::Pointer(element)
                | TypeKind::Atomic(element) => ty = element,
                TypeKind::Function(function) => ty = &function.return_type,
                _ => return Ok(false),
            }
        }
        Err(Error::new(0, "type nesting exceeds the 128-level limit"))
    }

    /// Computes alignment even when a complete array has a runtime extent.
    pub fn alignment(&self, ty: &Type) -> Result<u64, Error> {
        self.profile()?;
        if self.is_sizeless(ty)? {
            return Err(Error::new(0, "sizeless SVE types have no object alignment"));
        }
        let mut ty = ty;
        for _ in 0..128 {
            if let Some(alignment) = self.typedef_alignment(ty)? {
                return Ok(u64::from(alignment.get()));
            }
            match &self.resolve(ty)?.kind {
                TypeKind::Array { element, .. } | TypeKind::VariableArray { element, .. } => {
                    ty = element
                }
                _ => return Ok(self.layout(ty)?.alignment_bytes()),
            }
        }
        Err(Error::new(
            0,
            "array type nesting exceeds the 128-level limit",
        ))
    }

    /// Returns the outermost vendor typedef alignment override, without applying
    /// it to the pointee or mutating a referenced record.
    pub fn typedef_alignment(&self, ty: &Type) -> Result<Option<NonZeroU32>, Error> {
        Ok(self.typedef_alignment_metadata(ty)?.bytes())
    }

    /// Returns the outermost alignment snapshot, including its optional ancestry.
    pub fn typedef_alignment_metadata(&self, ty: &Type) -> Result<crate::TypeAlignment, Error> {
        let mut ty = ty;
        for _ in 0..128 {
            if ty.alignment.has_metadata() {
                let alignment = ty.alignment;
                self.validate_alignment_snapshot(alignment)?;
                if alignment
                    .bytes()
                    .is_some_and(|bytes| !bytes.get().is_power_of_two() || bytes.get() > (1 << 28))
                {
                    return Err(Error::new(
                        0,
                        "typedef alignment must be a supported power of two",
                    ));
                }
                return Ok(alignment);
            }
            let TypeKind::Typedef(name) = &ty.kind else {
                return Ok(crate::TypeAlignment::default());
            };
            ty = self
                .typedefs
                .get(name)
                .ok_or_else(|| Error::new(0, format!("unknown typedef `{name}`")))?;
        }
        Err(Error::new(
            0,
            "typedef alignment resolution exceeds the 128-level limit",
        ))
    }

    /// Collects top-level qualifiers contributed by every typedef in the chain.
    pub fn qualifiers(&self, ty: &Type) -> Result<Qualifiers, Error> {
        let mut ty = ty;
        let mut result = Qualifiers::default();
        for _ in 0..128 {
            result.is_const |= ty.qualifiers.is_const;
            result.is_volatile |= ty.qualifiers.is_volatile;
            result.is_restrict |= ty.qualifiers.is_restrict;
            result.set_unaligned(result.is_unaligned() || ty.qualifiers.is_unaligned());
            result.set_msvc_ptr32(result.is_msvc_ptr32() || ty.qualifiers.is_msvc_ptr32());
            let TypeKind::Typedef(name) = &ty.kind else {
                return Ok(result);
            };
            ty = self
                .typedefs
                .get(name)
                .ok_or_else(|| Error::new(0, format!("unknown typedef `{name}`")))?;
        }
        Err(Error::new(
            0,
            "typedef qualifier resolution exceeds the 128-level limit",
        ))
    }

    /// Resolves typedefs while retaining tag identities. Qualifiers on aliases
    /// remain available on the original type.
    pub fn resolve<'a>(&'a self, ty: &'a Type) -> Result<&'a Type, Error> {
        let mut ty = ty;
        let mut visited = HashSet::new();
        while let TypeKind::Typedef(name) = &ty.kind {
            if visited.len() >= 128 {
                return Err(Error::new(
                    0,
                    "typedef resolution exceeds the 128-level limit",
                ));
            }
            if !visited.insert(name) {
                return Err(Error::new(0, "cyclic typedef"));
            }
            ty = self
                .typedefs
                .get(name)
                .ok_or_else(|| Error::new(0, format!("unknown typedef `{name}`")))?;
        }
        Ok(ty)
    }

    /// Computes target layout, rejecting incomplete or recursively embedded types.
    pub fn layout(&self, ty: &Type) -> Result<target::Layout, Error> {
        self.profile()?;
        let lowered = self.layout_type(ty, &mut HashSet::new(), &mut HashMap::new(), 0, true)?;
        let mut layout = self
            .profile()?
            .layout(&lowered)
            .map_err(|e| Error::new(0, e.to_string()))?;
        // clang-cl honors a GNU typedef's decreased pointer alignment even though
        // the Microsoft field-layout rules retain the natural field alignment.
        if self.target.is_windows() {
            let mut current = ty;
            for _ in 0..128 {
                if let Some(alignment) = self.typedef_alignment(current)? {
                    layout.alignment_bits =
                        layout.alignment_bits.min(u64::from(alignment.get()) * 8);
                    break;
                }
                match &self.resolve(current)?.kind {
                    TypeKind::Array { element, .. } => current = element,
                    _ => break,
                }
            }
        }
        // `__unaligned` lowers the alignment required for an object or
        // pointee. A field of that type still uses its natural record layout;
        // layout_type above deliberately leaves field annotations unchanged.
        let mut current = ty;
        for _ in 0..128 {
            if self.qualifiers(current)?.is_unaligned() {
                layout.alignment_bits = layout.alignment_bits.min(8);
                break;
            }
            match &self.resolve(current)?.kind {
                TypeKind::Array { element, .. } | TypeKind::VariableArray { element, .. } => {
                    current = element;
                }
                _ => break,
            }
        }
        Ok(layout)
    }

    /// Returns a non-bitfield member's alignment under its containing record's
    /// packing rules. Alignment on the containing object or record does not raise
    /// the member's declared alignment.
    pub fn record_field_alignment(&self, record: usize, field: usize) -> Result<u64, Error> {
        let record = self
            .records
            .get(record)
            .ok_or_else(|| Error::new(0, "invalid record identity"))?;
        let field = record
            .fields
            .as_ref()
            .and_then(|fields| fields.get(field))
            .ok_or_else(|| Error::new(0, "invalid or incomplete record field"))?;
        if field.bit_width.is_some() {
            return Err(Error::new(0, "bitfields have no queryable alignment"));
        }
        let mut annotations = Vec::new();
        if record.packed {
            annotations.push(target::Annotation::Packed);
        }
        if let Some(pack) = record.pack {
            annotations.push(target::Annotation::PragmaPack(
                pack.checked_mul(8)
                    .ok_or_else(|| Error::new(0, "pack alignment overflows"))?,
            ));
        }
        let mut field_annotations = Vec::new();
        if field.packed {
            field_annotations.push(target::Annotation::Packed);
        }
        if let Some(alignment) = field.alignment {
            field_annotations.push(target::Annotation::Align(Some(
                alignment
                    .checked_mul(8)
                    .ok_or_else(|| Error::new(0, "alignment overflows"))?,
            )));
        }
        let ty = self.layout_type(
            &field.ty,
            &mut HashSet::new(),
            &mut HashMap::new(),
            0,
            false,
        )?;
        let layout = self
            .profile()?
            .layout(&target::Type {
                annotations,
                variant: target::TypeVariant::Record(target::Record {
                    kind: target::RecordKind::Struct,
                    fields: vec![target::Field {
                        ty,
                        annotations: field_annotations,
                        named: true,
                        bit_width: None,
                    }],
                }),
            })
            .map_err(|error| Error::new(0, error.to_string()))?;
        Ok(layout.field_alignment_bits / 8)
    }

    fn layout_type(
        &self,
        ty: &Type,
        active: &mut HashSet<usize>,
        cache: &mut HashMap<usize, target::Layout>,
        depth: usize,
        expand_record: bool,
    ) -> Result<target::Type, Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "record layout nesting exceeds the 128-level limit",
            ));
        }
        if matches!(ty.kind, TypeKind::Sve(_)) {
            return Err(Error::new(
                0,
                "sizeless SVE types have no fixed object layout",
            ));
        }
        if let TypeKind::Typedef(name) = &ty.kind {
            if ty.alignment.origin().is_some() {
                self.validate_alignment_snapshot(ty.alignment)?;
                let mut effective = self.resolve(ty)?.clone();
                effective.alignment = ty.alignment;
                return self.layout_type(&effective, active, cache, depth + 1, expand_record);
            }
            let inner = self
                .typedefs
                .get(name)
                .ok_or_else(|| Error::new(0, format!("unknown typedef `{name}`")))?;
            return Ok(aligned_layout_type(
                self.layout_type(inner, active, cache, depth + 1, expand_record)?,
                ty.alignment.bytes(),
            ));
        }
        let resolved = ty;
        if resolved.qualifiers.is_msvc_ptr32() {
            match self.target {
                Target::X86_64PcWindowsMsvc => {
                    return Ok(aligned_layout_type(
                        target::Type::opaque_layout(&target::Layout {
                            size_bits: 32,
                            alignment_bits: 32,
                            field_alignment_bits: 32,
                            required_alignment_bits: 8,
                            fields: Vec::new(),
                        }),
                        ty.alignment.bytes(),
                    ));
                }
                Target::Aarch64PcWindowsMsvc => {}
                _ => {
                    return Err(Error::new(
                        0,
                        "__ptr32 pointer ABI is unsupported on this target",
                    ));
                }
            }
        }
        if !expand_record && let TypeKind::Record(id) = resolved.kind {
            if !cache.contains_key(&id) {
                // Shared record definitions form a graph. Expanding every edge
                // as a separate tree duplicates nested fields exponentially.
                let lowered =
                    self.layout_type(&Type::new(TypeKind::Record(id)), active, cache, depth, true)?;
                let layout = self
                    .profile()?
                    .layout(&lowered)
                    .map_err(|error| Error::new(0, error.to_string()))?;
                cache.insert(id, layout);
            }
            return Ok(aligned_layout_type(
                target::Type::opaque_layout(&cache[&id]),
                ty.alignment.bytes(),
            ));
        }
        if let TypeKind::Atomic(value) = &resolved.kind {
            if self.qualifiers(value)? != Qualifiers::default()
                || matches!(
                    self.resolve(value)?.kind,
                    TypeKind::Atomic(_)
                        | TypeKind::Array { .. }
                        | TypeKind::VariableArray { .. }
                        | TypeKind::Function(_)
                        | TypeKind::Void
                )
            {
                return Err(Error::new(
                    0,
                    "atomic layout requires an unqualified non-atomic object value",
                ));
            }
            let inner = self.layout_type(value, active, cache, depth + 1, false)?;
            let inner = self
                .profile()?
                .layout(&inner)
                .map_err(|e| Error::new(0, e.to_string()))?;
            let layout = crate::atomic_type::atomic_layout(self.target, self.compiler, inner)?;
            return Ok(aligned_layout_type(
                target::Type::opaque_layout(&layout),
                ty.alignment.bytes(),
            ));
        }
        if let TypeKind::Vector { element, lanes, .. } = &resolved.kind {
            let element = self.resolve(element)?;
            if !matches!(element.kind, TypeKind::Integer(_) | TypeKind::Float(_))
                || !lanes.is_power_of_two()
            {
                return Err(Error::new(0, "invalid vector element type or lane count"));
            }
            let element = self.layout_type(element, active, cache, depth + 1, false)?;
            let size = self
                .target
                .layout(&element)
                .map_err(|error| Error::new(0, error.to_string()))?
                .size_bits
                .checked_mul(*lanes)
                .ok_or_else(|| Error::new(0, "vector size overflows"))?;
            if size == 0 || size > 128 || !size.is_power_of_two() {
                return Err(Error::new(
                    0,
                    "vectors larger than 16 bytes require unsupported target-feature configuration",
                ));
            }
            return Ok(aligned_layout_type(
                target::Type::opaque_layout(&target::Layout {
                    size_bits: size,
                    alignment_bits: size,
                    field_alignment_bits: size,
                    required_alignment_bits: 8,
                    fields: Vec::new(),
                }),
                ty.alignment.bytes(),
            ));
        }
        if let TypeKind::Float(kind) = resolved.kind
            && (kind.is_narrow() || kind == FloatKind::FLOAT128)
        {
            let bits = if kind == FloatKind::FLOAT128 { 128 } else { 16 };
            return Ok(aligned_layout_type(
                target::Type::opaque_layout(&target::Layout {
                    size_bits: bits,
                    alignment_bits: bits,
                    field_alignment_bits: bits,
                    required_alignment_bits: 8,
                    fields: Vec::new(),
                }),
                ty.alignment.bytes(),
            ));
        }
        let builtin = match &resolved.kind {
            TypeKind::Void => Some(target::BuiltinType::Void),
            TypeKind::Bool => Some(target::BuiltinType::Bool),
            TypeKind::Integer(kind) => Some(match kind {
                IntegerKind::Char => target::BuiltinType::Char,
                IntegerKind::SignedChar => target::BuiltinType::SignedChar,
                IntegerKind::UnsignedChar => target::BuiltinType::UnsignedChar,
                IntegerKind::Short => target::BuiltinType::Short,
                IntegerKind::UnsignedShort => target::BuiltinType::UnsignedShort,
                IntegerKind::Int => target::BuiltinType::Int,
                IntegerKind::UnsignedInt => target::BuiltinType::UnsignedInt,
                IntegerKind::Long => target::BuiltinType::Long,
                IntegerKind::UnsignedLong => target::BuiltinType::UnsignedLong,
                IntegerKind::LongLong => target::BuiltinType::LongLong,
                IntegerKind::UnsignedLongLong => target::BuiltinType::UnsignedLongLong,
                IntegerKind::Int128 => target::BuiltinType::Int128,
                IntegerKind::UnsignedInt128 => target::BuiltinType::UnsignedInt128,
            }),
            TypeKind::Float(kind) => Some(match *kind {
                FloatKind::Float | FloatKind::FLOAT32 => target::BuiltinType::Float,
                FloatKind::Double | FloatKind::FLOAT64 | FloatKind::FLOAT32X => {
                    target::BuiltinType::Double
                }
                FloatKind::LongDouble => target::BuiltinType::LongDouble,
                FloatKind::FLOAT64X if self.compiler == toucan_target::Compiler::Gnu => {
                    target::BuiltinType::LongDouble
                }
                FloatKind::FLOAT64X => {
                    return Err(Error::new(
                        0,
                        "_Float64x layout is only defined in the supported GNU profiles",
                    ));
                }
                FloatKind::BFloat16 | FloatKind::Extended { .. } => {
                    return Err(Error::new(
                        0,
                        "extended floating-point layout is unsupported",
                    ));
                }
            }),
            TypeKind::Pointer(_) => Some(target::BuiltinType::Pointer),
            _ => None,
        };
        if let Some(builtin) = builtin {
            return Ok(aligned_layout_type(
                target::Type::builtin(builtin),
                ty.alignment.bytes(),
            ));
        }
        let mut annotations = Vec::new();
        let variant = match &resolved.kind {
            TypeKind::Record(id) => {
                if !active.insert(*id) {
                    return Err(Error::new(0, "record contains itself by value"));
                }
                let record = self
                    .records
                    .get(*id)
                    .ok_or_else(|| Error::new(0, "invalid record identity"))?;
                let fields = record
                    .fields
                    .as_ref()
                    .ok_or_else(|| Error::new(0, "incomplete record has no layout"))?;
                if record.packed {
                    annotations.push(target::Annotation::Packed);
                }
                if let Some(alignment) = record.alignment {
                    annotations.push(target::Annotation::Align(Some(
                        alignment
                            .checked_mul(8)
                            .ok_or_else(|| Error::new(0, "alignment overflows"))?,
                    )));
                }
                if let Some(pack) = record.pack {
                    annotations.push(target::Annotation::PragmaPack(
                        pack.checked_mul(8)
                            .ok_or_else(|| Error::new(0, "pack alignment overflows"))?,
                    ));
                }
                let fields = fields
                    .iter()
                    .map(|field| {
                        let mut annotations = Vec::new();
                        if field.packed {
                            annotations.push(target::Annotation::Packed);
                        }
                        if let Some(alignment) = field.alignment {
                            annotations.push(target::Annotation::Align(Some(
                                alignment
                                    .checked_mul(8)
                                    .ok_or_else(|| Error::new(0, "alignment overflows"))?,
                            )));
                        }
                        Ok(target::Field {
                            ty: self.layout_type(&field.ty, active, cache, depth + 1, false)?,
                            annotations,
                            named: field.name.is_some() || field.bit_width.is_none(),
                            bit_width: field.bit_width,
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()?;
                active.remove(id);
                target::TypeVariant::Record(target::Record {
                    kind: match record.kind {
                        RecordKind::Struct => target::RecordKind::Struct,
                        RecordKind::Union => target::RecordKind::Union,
                    },
                    fields,
                })
            }
            TypeKind::Complex(kind) => {
                if !matches!(
                    *kind,
                    FloatKind::Float
                        | FloatKind::Double
                        | FloatKind::LongDouble
                        | FloatKind::FLOAT128
                        | FloatKind::FLOAT32
                        | FloatKind::FLOAT64
                        | FloatKind::FLOAT32X
                        | FloatKind::FLOAT64X
                ) {
                    return Err(Error::new(0, "extended complex types are unsupported"));
                }
                target::TypeVariant::Array {
                    element: Box::new(self.layout_type(
                        &Type::new(TypeKind::Float(*kind)),
                        active,
                        cache,
                        depth + 1,
                        false,
                    )?),
                    length: Some(2),
                }
            }
            TypeKind::Array { element, length } => target::TypeVariant::Array {
                element: Box::new(self.layout_type(element, active, cache, depth + 1, false)?),
                length: *length,
            },
            TypeKind::VariableArray { .. } => {
                return Err(Error::new(
                    0,
                    "variable-length array size requires a runtime bound",
                ));
            }
            TypeKind::Enum(id) => {
                let enumeration = self
                    .enums
                    .get(*id)
                    .ok_or_else(|| Error::new(0, "invalid enum identity"))?;
                if !enumeration.complete {
                    return Err(Error::new(0, "incomplete enum has no object layout"));
                }
                if enumeration.packed {
                    annotations.push(target::Annotation::Packed);
                }
                target::TypeVariant::Enum(
                    enumeration
                        .variants
                        .iter()
                        .map(|variant| {
                            variant.value.validate()?;
                            if variant.value.signed {
                                Ok(variant.value.signed_value())
                            } else {
                                i128::try_from(variant.value.value)
                                    .map_err(|_| Error::new(0, "enum value exceeds layout support"))
                            }
                        })
                        .collect::<Result<_, _>>()?,
                )
            }
            TypeKind::Function(_) => return Err(Error::new(0, "a function has no object layout")),
            _ => unreachable!("builtin and typedef cases handled above"),
        };
        Ok(aligned_layout_type(
            target::Type {
                annotations,
                variant,
            },
            ty.alignment.bytes(),
        ))
    }
}

fn aligned_layout_type(inner: target::Type, alignment: Option<NonZeroU32>) -> target::Type {
    match alignment {
        Some(alignment) => target::Type {
            annotations: vec![target::Annotation::Align(Some(
                u64::from(alignment.get()) * 8,
            ))],
            variant: target::TypeVariant::Typedef(Box::new(inner)),
        },
        None => inner,
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

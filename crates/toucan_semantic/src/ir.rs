use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;
use toucan_target::{self as target, Target};

use crate::Error;

/// Declarations and canonical tag identities in a preprocessed translation unit.
#[derive(Clone, Debug, Serialize)]
pub struct TranslationUnit {
    pub target: Target,
    pub declarations: Vec<Declaration>,
    pub records: Vec<Record>,
    pub enums: Vec<Enum>,
    pub typedefs: BTreeMap<String, Type>,
    pub constants: BTreeMap<String, IntegerValue>,
}

/// A qualified C type. Typedefs and tags retain their declaration identities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Type {
    pub kind: TypeKind,
    pub qualifiers: Qualifiers,
}

impl Type {
    /// Constructs an unqualified type.
    pub fn new(kind: TypeKind) -> Self {
        Self {
            kind,
            qualifiers: Qualifiers::default(),
        }
    }

    /// Constructs a pointer to this type, preserving the pointee's qualifiers.
    pub fn pointer(self) -> Self {
        Self::new(TypeKind::Pointer(Box::new(self)))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Qualifiers {
    pub is_const: bool,
    pub is_volatile: bool,
    pub is_restrict: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum TypeKind {
    Void,
    Bool,
    Integer(IntegerKind),
    Float(FloatKind),
    Pointer(Box<Type>),
    Array {
        element: Box<Type>,
        length: Option<u64>,
    },
    /// A complete array whose extent is determined at runtime. Unlike an
    /// incomplete array, its extent cannot be completed by a later declaration.
    VariableArray {
        element: Box<Type>,
    },
    Function(Box<FunctionType>),
    Record(usize),
    Enum(usize),
    Typedef(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum FloatKind {
    Float,
    Double,
    LongDouble,
    Extended {
        format: ExtendedFloatFormat,
        width: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ExtendedFloatFormat {
    BinaryInterchange,
    BinaryExtended,
    DecimalInterchange,
    DecimalExtended,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FunctionType {
    pub return_type: Type,
    pub parameters: Vec<Parameter>,
    pub variadic: bool,
    /// False for the C11 non-prototype declaration `f()`.
    pub prototype: bool,
    pub calling_convention: CallingConvention,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CallingConvention {
    C,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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

#[derive(Clone, Debug, Serialize)]
pub struct Declaration {
    pub name: String,
    pub ty: Type,
    pub kind: DeclarationKind,
    pub link_name: Option<String>,
    pub is_static: bool,
    pub is_definition: bool,
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
                TypeKind::Array { element, .. } | TypeKind::Pointer(element) => ty = element,
                TypeKind::Function(function) => ty = &function.return_type,
                _ => return Ok(false),
            }
        }
        Err(Error::new(0, "type nesting exceeds the 128-level limit"))
    }

    /// Computes alignment even when a complete array has a runtime extent.
    pub fn alignment(&self, ty: &Type) -> Result<u64, Error> {
        let mut ty = ty;
        for _ in 0..128 {
            match &self.resolve(ty)?.kind {
                TypeKind::Array { element, .. } | TypeKind::VariableArray { element } => {
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

    /// Collects top-level qualifiers contributed by every typedef in the chain.
    pub fn qualifiers(&self, ty: &Type) -> Result<Qualifiers, Error> {
        let mut ty = ty;
        let mut result = Qualifiers::default();
        for _ in 0..128 {
            result.is_const |= ty.qualifiers.is_const;
            result.is_volatile |= ty.qualifiers.is_volatile;
            result.is_restrict |= ty.qualifiers.is_restrict;
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
        let ty = self.layout_type(ty, &mut HashSet::new(), &mut HashMap::new(), 0, true)?;
        self.target
            .layout(&ty)
            .map_err(|e| Error::new(0, e.to_string()))
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
        let resolved = self.resolve(ty)?;
        if !expand_record && let TypeKind::Record(id) = resolved.kind {
            if !cache.contains_key(&id) {
                // Shared record definitions form a graph. Expanding every edge
                // as a separate tree duplicates nested fields exponentially.
                let lowered = self.layout_type(ty, active, cache, depth, true)?;
                let layout = self
                    .target
                    .layout(&lowered)
                    .map_err(|error| Error::new(0, error.to_string()))?;
                cache.insert(id, layout);
            }
            return Ok(target::Type::opaque_layout(&cache[&id]));
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
            TypeKind::Float(kind) => Some(match kind {
                FloatKind::Float => target::BuiltinType::Float,
                FloatKind::Double => target::BuiltinType::Double,
                FloatKind::LongDouble => target::BuiltinType::LongDouble,
                FloatKind::Extended { .. } => {
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
            return Ok(target::Type::builtin(builtin));
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
        Ok(target::Type {
            annotations,
            variant,
        })
    }
}

//! Scalar object values use their checked C destination type, independently of macros.
use crate::{Emitter, Error, Options};
use std::fmt::Write;
use toucan_semantic::{
    ArithmeticConstant, Declaration, DeclarationKind, FloatKind, Type, TypeKind,
};

pub(crate) enum ObjectConstant<'a> {
    Arithmetic(ArithmeticConstant),
    String(&'a [u8]),
}

impl Options {
    /// Validate the profile, indices, names, and types of supplied object occurrences.
    pub(crate) fn validate_object_bindings(
        &self,
        unit: &toucan_semantic::TranslationUnit,
    ) -> Result<(), Error> {
        for (name, object) in &self.object_bindings {
            if object.profile() != unit.profile()? {
                return Err(Error(format!(
                    "object occurrence `{name}` has a different compiler profile"
                )));
            }
            let Some(declaration) = unit.declarations.get(object.declaration()) else {
                return Err(Error(
                    "object occurrence has an invalid declaration index".into(),
                ));
            };
            if name != object.name()
                || name != &declaration.name
                || declaration.kind != DeclarationKind::Variable
            {
                return Err(Error(format!(
                    "object occurrence `{name}` does not match its declaration"
                )));
            }
            if let Some(bytes) = object.string_literal() {
                let element = match &unit.resolve(object.ty())?.kind {
                    TypeKind::Pointer(element) | TypeKind::Array { element, .. } => element,
                    _ => {
                        return Err(Error(format!(
                            "string object occurrence `{name}` has a different destination type"
                        )));
                    }
                };
                if bytes.last() != Some(&0)
                    || !matches!(
                        unit.resolve(element)?.kind,
                        TypeKind::Integer(
                            toucan_semantic::IntegerKind::Char
                                | toucan_semantic::IntegerKind::SignedChar
                                | toucan_semantic::IntegerKind::UnsignedChar
                        )
                    )
                {
                    return Err(Error(format!(
                        "string object occurrence `{name}` has invalid byte storage"
                    )));
                }
            }
            if let Some(value) = object.value() {
                let destination = unit.atomic_value(object.ty())?.unwrap_or(object.ty());
                let kind = &unit.resolve(destination)?.kind;
                let matches = match (value, kind) {
                    (ArithmeticConstant::Integer(value), TypeKind::Bool) => {
                        value.rank == 0 && !value.signed && value.bits == 8 && value.value <= 1
                    }
                    (ArithmeticConstant::Integer(value), TypeKind::Integer(kind)) => {
                        use toucan_semantic::IntegerKind as I;
                        let signed = match kind {
                            I::Char => unit.target.char_is_signed(),
                            I::UnsignedChar
                            | I::UnsignedShort
                            | I::UnsignedInt
                            | I::UnsignedLong
                            | I::UnsignedLongLong
                            | I::UnsignedInt128 => false,
                            _ => true,
                        };
                        value.signed == signed
                            && u64::from(value.bits) == unit.layout(destination)?.size_bits
                    }
                    (ArithmeticConstant::Floating(value), TypeKind::Float(kind)) => {
                        value.kind() == *kind
                    }
                    _ => false,
                };
                if !matches {
                    return Err(Error(format!(
                        "object occurrence `{name}` has a different scalar destination type"
                    )));
                }
            }
            let first = unit.resolve(object.ty())?;
            let last = unit.resolve(&declaration.ty)?;
            let completed_array = matches!((&first.kind, &last.kind), (TypeKind::Array { element: a, length: None }, TypeKind::Array { element: b, .. }) if a == b && first.qualifiers == last.qualifiers);
            if first != last && !completed_array {
                return Err(Error(format!(
                    "object occurrence `{name}` has a different declaration type"
                )));
            }
        }
        Ok(())
    }
}

impl<'unit> Emitter<'unit> {
    /// Retrieve a selected arithmetic projection, preserving unsupported reference cases.
    pub(crate) fn object_constant(
        &self,
        declaration: &Declaration,
    ) -> Result<Option<ObjectConstant<'unit>>, Error> {
        let Some(object) = self.options.object_bindings.get(&declaration.name) else {
            return Ok(None);
        };
        if let Some(bytes) = object.string_literal() {
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .ok_or_else(|| Error("string object literal has no terminating NUL".into()))?;
            if let TypeKind::Array {
                length: Some(length),
                ..
            } = self.unit.resolve(object.ty())?.kind
                && end as u64 >= length
            {
                return Err(Error(format!(
                    "string object `{}` has no terminating NUL within its {length}-byte C array",
                    declaration.name
                )));
            }
            return Ok(Some(ObjectConstant::String(&bytes[..=end])));
        }
        let Some(value) = object.value() else {
            return Ok(None);
        };
        match value {
            ArithmeticConstant::Integer(value) => {
                crate::validate_integer(value)?;
                if value.bits > 64 {
                    return Err(Error(format!(
                        "object constant `{}` exceeds bindgen's 64-bit integer projection; a C wrapper is required",
                        declaration.name
                    )));
                }
                if !value.signed
                    && value.value > i64::MAX as u128
                    && !object.integer_literal_fallback()
                {
                    return Ok(None);
                }
            }
            ArithmeticConstant::Floating(value)
                if !matches!(value.kind(), FloatKind::Float | FloatKind::Double) =>
            {
                return Err(Error(format!(
                    "object constant `{}` has an unsupported Rust floating representation",
                    declaration.name
                )));
            }
            ArithmeticConstant::Complex(_) => {
                return Err(Error(
                    "complex object constants have no Rust scalar representation".into(),
                ));
            }
            _ => {}
        }
        Ok(Some(ObjectConstant::Arithmetic(value)))
    }

    /// Use the selected declaration's type; atomic literals project their contained scalar.
    pub(crate) fn object_type(
        &self,
        declaration: &'unit Declaration,
    ) -> Result<&'unit Type, Error> {
        let ty = self
            .options
            .object_bindings
            .get(&declaration.name)
            .map_or(&declaration.ty, |object| object.ty());
        if self.object_constant(declaration)?.is_some() {
            return Ok(self.unit.atomic_value(ty)?.unwrap_or(ty));
        }
        Ok(ty)
    }

    /// Emit a checked scalar initializer using the declaration's Rust name and type.
    pub(crate) fn emit_object_constant(
        &self,
        declaration: &'unit Declaration,
        source: &mut String,
    ) -> Result<(), Error> {
        let Some(value) = self.object_constant(declaration)? else {
            return Ok(());
        };
        let name = self.generated_name(&declaration.name)?;
        let value = match value {
            ObjectConstant::Arithmetic(value) => value,
            ObjectConstant::String(bytes) => {
                if self.options.generate_cstr {
                    write!(source, "pub const {name}: &::core::ffi::CStr = unsafe {{ ::core::ffi::CStr::from_bytes_with_nul_unchecked(&[").unwrap();
                } else {
                    write!(
                        source,
                        "pub const {name}: &[::core::primitive::u8; {}] = &[",
                        bytes.len()
                    )
                    .unwrap();
                }
                for byte in bytes {
                    write!(source, "{byte}, ").unwrap();
                }
                source.push_str(if self.options.generate_cstr {
                    "]) };\n"
                } else {
                    "];\n"
                });
                return Ok(());
            }
        };
        let ty = self.ty(self.object_type(declaration)?)?;
        match value {
            ArithmeticConstant::Integer(value) => {
                let literal = if value.rank == 0 {
                    (value.value != 0).to_string()
                } else if value.signed {
                    value.signed_value().to_string()
                } else {
                    value.value.to_string()
                };
                writeln!(source, "pub const {name}: {ty} = {literal};").unwrap();
            }
            ArithmeticConstant::Floating(value) => {
                let width = if value.kind() == FloatKind::Float {
                    32
                } else {
                    64
                };
                source.push_str(&crate::floating_bits_constant_typed(
                    &name,
                    &ty,
                    width,
                    value.to_bits(),
                    self.options.rust_target,
                ));
            }
            ArithmeticConstant::Complex(_) => unreachable!("validated above"),
        }
        Ok(())
    }
}

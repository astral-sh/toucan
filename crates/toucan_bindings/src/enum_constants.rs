//! Project C enumerator names without changing selection or macro identities.

use std::borrow::Cow;
use std::collections::BTreeMap;

use toucan_semantic::{DeclarationKind, Scope, Type, TypeKind};

use crate::{Emitter, Error, MacroValue, identifier};

/// Public constants accompanying generated enum representations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnumConstantStyle {
    /// Preserve global integer projections for every selected enumerator.
    #[default]
    Integer,
    /// Match bindgen: named Rust enums use variants; unnamed Rust enums also
    /// expose enum-typed constants. Integer representations keep global constants.
    Bindgen,
}

pub(super) struct Constant {
    name: Option<String>,
    rust_enum: Option<usize>,
}

#[derive(Default)]
pub(super) struct Names<'a> {
    values: BTreeMap<&'a str, Constant>,
}

impl<'a> Emitter<'a> {
    /// Cache original-to-generated names once for the opt-in naming policies.
    pub(super) fn prepare_enum_constant_names(&mut self) -> Result<(), Error> {
        if !self.options.prepend_enum_name
            && self.options.enum_constant_style == EnumConstantStyle::Integer
        {
            return Ok(());
        }
        let mut typedefs = BTreeMap::new();
        if self.options.enum_constant_style == EnumConstantStyle::Integer {
            for declaration in &self.unit.declarations {
                if declaration.kind == DeclarationKind::Typedef
                    && let TypeKind::Enum(id) = self.unit.resolve(&declaration.ty)?.kind
                {
                    typedefs.entry(id).or_insert(declaration.name.as_str());
                }
            }
        }
        for (id, enumeration) in self.unit.enums.iter().enumerate() {
            if enumeration.scope != Scope::File {
                continue;
            }
            let (enum_name, named) =
                if self.options.enum_constant_style == EnumConstantStyle::Bindgen {
                    let named = enumeration.name.is_some()
                        || crate::lexical_names::Names::typedef_name(
                            self.unit,
                            self.unit.lexical_tags.enums.get(&id),
                        )
                        .is_some();
                    (
                        if named {
                            self.lexical_names.enum_name(self.unit, id)
                        } else {
                            self.lexical_names.enum_parent(self.unit, id)
                        },
                        named,
                    )
                } else {
                    let name = enumeration
                        .name
                        .as_deref()
                        .or_else(|| typedefs.get(&id).copied());
                    (name, name.is_some())
                };
            let scoped = self.options.enum_constant_style == EnumConstantStyle::Bindgen
                && self.is_rustified_enum(id);
            let omitted =
                (scoped && named) || self.external_key(&Type::new(TypeKind::Enum(id)))?.is_some();
            for variant in &enumeration.variants {
                let name = if omitted {
                    None
                } else if (self.options.prepend_enum_name || scoped)
                    && let Some(prefix) = enum_name
                {
                    Some(format!(
                        "{}_{}",
                        name_part(prefix)?,
                        name_part(&variant.name)?
                    ))
                } else if self.options.enum_constant_style == EnumConstantStyle::Bindgen {
                    Some(name_part(&variant.name)?.into_owned())
                } else {
                    Some(self.names.identifier(&variant.name)?)
                };
                if self
                    .enum_constant_names
                    .values
                    .insert(
                        &variant.name,
                        Constant {
                            name,
                            rust_enum: scoped.then_some(id),
                        },
                    )
                    .is_some()
                {
                    return Err(Error(format!("duplicate enumerator `{}`", variant.name)));
                }
            }
        }
        Ok(())
    }

    /// Select by C spelling, then apply the output name and shadowing policy.
    pub(super) fn emitted_constant_name(
        &self,
        name: &str,
        macros: &BTreeMap<String, Option<MacroValue>>,
    ) -> Result<Option<String>, Error> {
        if !self.options.includes_constant(name) || self.blocked_enumerator(name) {
            return Ok(None);
        }
        if let Some(constant) = self.enum_constant_names.values.get(name) {
            let Some(generated) = &constant.name else {
                return Ok(None);
            };
            // Prefixing separates the enum constant from a macro with the old
            // C spelling. Bindgen preserves both declarations. Toucan rejects
            // actual output collisions, including when prefixing is disabled.
            if self.options.enum_constant_style != EnumConstantStyle::Bindgen
                && macros.contains_key(name)
                && *generated == self.names.identifier(name)?
            {
                return Ok(None);
            }
            return Ok(Some(generated.clone()));
        }
        if macros.contains_key(name) {
            return Ok(None);
        }
        Ok(Some(self.names.identifier(name)?))
    }

    /// Unnamed Rust enums retain enum-typed globals for their C enumerators.
    pub(super) fn rust_enum_constant(&self, name: &str) -> Option<usize> {
        self.enum_constant_names
            .values
            .get(name)
            .and_then(|constant| constant.rust_enum)
    }
}

/// Bindgen escapes each keyword or primitive component before joining it.
pub(super) fn name_part(name: &str) -> Result<Cow<'_, str>, Error> {
    let escaped = identifier(name)?;
    if escaped != name
        || matches!(
            name,
            "alignof"
                | "offsetof"
                | "proc"
                | "pure"
                | "sizeof"
                | "str"
                | "bool"
                | "f32"
                | "f64"
                | "usize"
                | "isize"
                | "u128"
                | "i128"
                | "u64"
                | "i64"
                | "u32"
                | "i32"
                | "u16"
                | "i16"
                | "u8"
                | "i8"
        )
    {
        Ok(Cow::Owned(format!("{name}_")))
    } else {
        Ok(Cow::Borrowed(name))
    }
}

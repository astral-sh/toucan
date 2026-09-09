//! Compatible enum integers remain distinct from enumerator expression types.

use crate::{Error, IntegerKind, TranslationUnit, Type, TypeKind};

impl TranslationUnit {
    /// Returns the complete enum's compatible C integer type.
    ///
    /// Packing can select a byte or short integer, while enumerator identifiers
    /// still have type `int` when their values fit. Distinct enum tags keep their
    /// identities. This query does not replace the enum's own storage layout.
    pub fn enum_integer_kind(&self, id: usize) -> Result<IntegerKind, Error> {
        let enumeration = self
            .enums
            .get(id)
            .ok_or_else(|| Error::new(0, "invalid enum identity"))?;
        // Layout selects a representation from the values and tag attributes;
        // it does not call this query or Analyzer::integer_type.
        let layout = self.layout(&Type::new(TypeKind::Enum(id)))?;
        let signed = self.target.is_windows()
            || enumeration
                .variants
                .iter()
                .any(|variant| variant.value.signed && variant.value.signed_value() < 0);
        Ok(match (layout.size_bits, signed) {
            (8, true) => IntegerKind::SignedChar,
            (8, false) => IntegerKind::UnsignedChar,
            (16, true) => IntegerKind::Short,
            (16, false) => IntegerKind::UnsignedShort,
            (32, true) => IntegerKind::Int,
            (32, false) => IntegerKind::UnsignedInt,
            (64, true) if self.target.long_width() == 64 => IntegerKind::Long,
            (64, false) if self.target.long_width() == 64 => IntegerKind::UnsignedLong,
            (64, true) => IntegerKind::LongLong,
            (64, false) => IntegerKind::UnsignedLongLong,
            (128, true) => IntegerKind::Int128,
            (128, false) => IntegerKind::UnsignedInt128,
            _ => return Err(Error::new(0, "unsupported enum integer width")),
        })
    }
}

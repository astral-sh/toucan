use std::num::NonZeroU32;

use serde::Serialize;
use toucan_target::Compiler;

use crate::analyze::{Analyzer, Attributes};
use crate::{Declaration, DeclarationKind, Error, TranslationUnit, Type};

/// Alignment attached to an object or function declaration, not its C type.
///
/// GNU and Microsoft attributes can decrease object alignment. C11 requirements cannot do so,
/// and Clang additionally checks agreement between C11 redeclarations. Keeping
/// the two spellings separate preserves those rules when declarations merge.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct DeclarationAlignment {
    // Zero means absent. Otherwise these encode a bounded power-of-two exponent.
    gnu: u8,
    msvc: u8,
    // The extra value 1 preserves a written _Alignas(0).
    c11: u8,
    effective: u8,
}

impl DeclarationAlignment {
    /// Creates declaration annotations from byte values. GNU zero is ignored;
    /// C11 zero remains explicit for redeclaration constraints.
    pub fn new(gnu: Option<u32>, c11: Option<u32>) -> Result<Self, Error> {
        let gnu = encode(gnu.unwrap_or(0))?;
        let c11 = c11
            .map(|value| encode(value).map(|encoded| encoded + 1))
            .transpose()?
            .unwrap_or(0);
        Ok(Self {
            gnu,
            msvc: 0,
            c11,
            effective: 0,
        })
    }

    /// Adds a Microsoft alignment while preserving its spelling separately.
    pub fn with_msvc(mut self, bytes: u32) -> Result<Self, Error> {
        if !bytes.is_power_of_two() || bytes > 8192 {
            return Err(Error::new(
                0,
                "Microsoft alignment must be a power of two from 1 through 8192 bytes",
            ));
        }
        self.msvc = self.msvc.max(encode(bytes)?);
        Ok(self)
    }

    /// The strongest Microsoft `__declspec(align)` attribute, in bytes.
    pub fn msvc(self) -> Option<NonZeroU32> {
        decode(self.msvc)
    }

    /// The strongest GNU `aligned` attribute, in bytes.
    pub fn gnu(self) -> Option<NonZeroU32> {
        decode(self.gnu)
    }

    /// The strongest written C11 requirement, including explicit zero.
    pub fn c11(self) -> Option<u32> {
        (self.c11 != 0).then(|| decode(self.c11 - 1).map_or(0, NonZeroU32::get))
    }

    /// Effective object, parameter or function alignment after declaration merging.
    /// Absence uses the declared type's alignment. Record fields instead require
    /// their containing record's field-layout rules; this value is absent for them.
    pub fn effective(self) -> Option<NonZeroU32> {
        decode(self.effective)
    }

    /// Whether this declaration has no written or inherited alignment facts.
    pub fn is_empty(&self) -> bool {
        self.gnu == 0 && self.msvc == 0 && self.c11 == 0 && self.effective == 0
    }

    /// The greatest nonzero explicit alignment, before the natural minimum.
    pub fn explicit(self) -> Option<NonZeroU32> {
        self.gnu()
            .max(self.msvc())
            .max(self.c11().and_then(NonZeroU32::new))
    }

    pub(crate) fn combined(self, other: Self) -> Self {
        Self {
            gnu: self.gnu.max(other.gnu),
            msvc: self.msvc.max(other.msvc),
            c11: self.c11.max(other.c11),
            effective: self.effective.max(other.effective),
        }
    }

    fn set_effective(&mut self, value: u64, offset: usize) -> Result<(), Error> {
        self.effective = encode(u32::try_from(value).map_err(|_| {
            Error::new(offset, "declaration alignment exceeds the supported range")
        })?)
        .map_err(|mut error| {
            error.offset = offset;
            error
        })?;
        Ok(())
    }

    fn validate(self) -> Result<(), Error> {
        if self.gnu > 29 || self.msvc > 14 || self.c11 > 30 || self.effective > 29 {
            return Err(Error::new(
                0,
                "declaration alignment exceeds the supported range",
            ));
        }
        Ok(())
    }

    fn object(self, natural: u64) -> Result<u64, Error> {
        self.validate()?;
        if let Some(effective) = self.effective() {
            return Ok(u64::from(effective.get()));
        }
        let explicit = self
            .explicit()
            .map_or(natural, |value| u64::from(value.get()));
        // _Alignas(0) is a layout no-op, even alongside a GNU decrease.
        Ok(if self.c11().is_some_and(|value| value != 0) {
            explicit.max(natural)
        } else {
            explicit
        })
    }
}

fn encode(bytes: u32) -> Result<u8, Error> {
    if bytes == 0 {
        return Ok(0);
    }
    if !bytes.is_power_of_two() || bytes > (1 << 28) {
        return Err(Error::new(
            0,
            "declaration alignment must be a supported power of two",
        ));
    }
    Ok(bytes.trailing_zeros() as u8 + 1)
}

fn decode(exponent: u8) -> Option<NonZeroU32> {
    if exponent == 0 {
        None
    } else {
        NonZeroU32::new(1u32.checked_shl(u32::from(exponent - 1))?)
    }
}

impl std::fmt::Debug for DeclarationAlignment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeclarationAlignment")
            .field("gnu", &self.gnu())
            .field("msvc", &self.msvc())
            .field("c11", &self.c11())
            .field("effective", &self.effective())
            .finish()
    }
}

impl Serialize for DeclarationAlignment {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let count = usize::from(self.gnu != 0)
            + usize::from(self.msvc != 0)
            + usize::from(self.c11 != 0)
            + usize::from(self.effective != 0);
        let mut state = serializer.serialize_struct("DeclarationAlignment", count)?;
        if let Some(value) = self.gnu() {
            state.serialize_field("gnu", &value)?;
        }
        if let Some(value) = self.msvc() {
            state.serialize_field("msvc", &value)?;
        }
        if let Some(value) = self.c11() {
            state.serialize_field("c11", &value)?;
        }
        if let Some(value) = self.effective() {
            state.serialize_field("effective", &value)?;
        }
        state.end()
    }
}

impl TranslationUnit {
    /// Computes a declared object's alignment without changing its type layout.
    ///
    /// This is its language-level alignment, not a prediction of a linker's
    /// placement. The declaration must denote an object whose alignment is known.
    pub fn declaration_alignment(&self, declaration: &Declaration) -> Result<u64, Error> {
        if declaration.kind != DeclarationKind::Variable {
            return Err(Error::new(
                0,
                "object alignment requires an object declaration",
            ));
        }
        let natural = self.alignment(&declaration.ty)?;
        let alignment = declaration.alignment;
        alignment.validate()?;
        if let Some(c11) = alignment.c11().filter(|value| *value != 0) {
            let requested = if self.compiler == Compiler::Clang {
                c11.max(alignment.gnu().map_or(0, NonZeroU32::get))
            } else {
                c11
            };
            if u64::from(requested) < natural {
                return Err(Error::new(
                    0,
                    "_Alignas cannot reduce the object's natural alignment",
                ));
            }
        }
        alignment.object(natural)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum AlignmentSubject {
    Object { register: bool },
    Function,
    Parameter,
    Field { bitfield: bool },
}

impl Analyzer {
    pub(crate) fn alignment_operand(
        &mut self,
        check: impl FnOnce(&mut Self) -> Result<u64, Error>,
    ) -> Result<u64, Error> {
        let checkpoint = self
            .checked
            .as_ref()
            .map(|checked| checked.evaluation_checkpoint());
        let sve = self.sve_feature_checkpoint();
        let value = check(self);
        self.discard_sve_feature_uses(sve);
        if value.is_ok()
            && let (Some(checked), Some(checkpoint)) = (&mut self.checked, checkpoint)
        {
            checked.finish_alignment_operand(checkpoint);
        }
        value
    }

    pub(crate) fn check_declaration_alignment(
        &self,
        ty: &Type,
        base: &Attributes,
        extra: &Attributes,
        subject: AlignmentSubject,
        offset: usize,
    ) -> Result<DeclarationAlignment, Error> {
        let gnu = base.alignment.max(extra.alignment);
        let msvc = base.msvc_alignment.max(extra.msvc_alignment);
        let c11 = base.c11_alignment.max(extra.c11_alignment);
        let mut alignment =
            DeclarationAlignment::new(gnu.map(|value| value as u32), c11.map(|value| value as u32))
                .map_err(|mut error| {
                    error.offset = offset;
                    error
                })?;
        if let Some(value) = msvc {
            alignment = alignment.with_msvc(value as u32)?;
        }
        alignment.validate().map_err(|mut error| {
            error.offset = offset;
            error
        })?;
        if c11.is_some()
            && !matches!(
                subject,
                AlignmentSubject::Object { register: false }
                    | AlignmentSubject::Field { bitfield: false }
            )
        {
            return Err(Error::new(
                offset,
                "_Alignas requires an object without register storage",
            ));
        }
        if gnu.is_some()
            && matches!(subject, AlignmentSubject::Parameter)
            && self.unit.compiler == Compiler::Gnu
        {
            return Err(Error::new(
                offset,
                "GNU alignment attributes are not permitted on parameters",
            ));
        }
        if c11.is_some()
            && (self.unit.compiler == Compiler::Gnu || self.is_complete_object(ty, 0)?)
            && let Some(natural) = self.declaration_natural_alignment(ty)?
        {
            let requested = if self.unit.compiler == Compiler::Clang {
                c11.max(gnu).max(msvc).unwrap_or(0)
            } else {
                c11.unwrap_or(0)
            };
            if requested < natural
                && (requested != 0
                    || (self.unit.compiler == Compiler::Clang && gnu.max(msvc).is_some()))
            {
                return Err(Error::new(
                    offset,
                    "_Alignas cannot reduce the object's natural alignment",
                ));
            }
        }
        if !alignment.is_empty()
            && !matches!(subject, AlignmentSubject::Field { .. })
            && let Some(natural) = self.declaration_natural_alignment(ty)?
        {
            alignment.set_effective(alignment.object(natural)?, offset)?;
        }
        Ok(alignment)
    }

    fn declaration_natural_alignment(&self, ty: &Type) -> Result<Option<u64>, Error> {
        if matches!(self.unit.resolve(ty)?.kind, crate::TypeKind::Function(_)) {
            return Ok(Some(
                if self.unit.compiler == Compiler::Gnu
                    && self.unit.target == toucan_target::Target::X86_64UnknownLinuxGnu
                {
                    1
                } else {
                    4
                },
            ));
        }
        if matches!(
            self.unit.resolve(ty)?.kind,
            crate::TypeKind::Array { .. } | crate::TypeKind::VariableArray { .. }
        ) || self.is_complete_object(ty, 0)?
        {
            Ok(Some(self.unit.alignment(ty)?))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn merge_declaration_alignment(
        &self,
        ty: &Type,
        previous: DeclarationAlignment,
        written: DeclarationAlignment,
        previous_definition: bool,
        definition: bool,
        offset: usize,
    ) -> Result<DeclarationAlignment, Error> {
        if previous.is_empty() && written.is_empty() {
            return Ok(written);
        }
        if self.unit.compiler == Compiler::Clang {
            if previous.c11().is_some()
                && written.c11().is_some()
                && let Some(natural) = self.declaration_natural_alignment(ty)?
                && previous.object(natural)? != written.object(natural)?
            {
                return Err(Error::new(
                    offset,
                    "redeclaration has a different _Alignas requirement",
                ));
            }
            if (definition && previous.c11().is_some() && written.c11().is_none())
                || (previous_definition && previous.c11().is_none() && written.c11().is_some())
            {
                return Err(Error::new(
                    offset,
                    "_Alignas must be specified on the object definition",
                ));
            }
        }
        let mut combined = previous.combined(written);
        if self.unit.compiler == Compiler::Gnu
            && let Some(natural) = self.declaration_natural_alignment(ty)?
        {
            combined.set_effective(
                previous.object(natural)?.max(written.object(natural)?),
                offset,
            )?;
        }
        Ok(combined)
    }

    pub(crate) fn visible_linked_alignment(&self, name: &str) -> Option<DeclarationAlignment> {
        for scope in self.lexical_scopes.iter().rev() {
            if scope.names.contains_key(name) && scope.linked.contains(name) {
                return Some(
                    scope
                        .alignments
                        .as_ref()
                        .and_then(|values| values.get(name))
                        .copied()
                        .unwrap_or_default(),
                );
            }
        }
        self.unit
            .declarations
            .iter()
            .find(|declaration| {
                declaration.name == name && declaration.kind != DeclarationKind::Typedef
            })
            .map(|declaration| declaration.alignment)
    }

    pub(crate) fn retain_local_alignment(&mut self, name: &str, alignment: DeclarationAlignment) {
        if !alignment.is_empty() {
            self.lexical_scopes
                .last_mut()
                .expect("local alignment scope")
                .alignments
                .get_or_insert_with(Default::default)
                .insert(name.to_owned(), alignment);
        }
    }
}

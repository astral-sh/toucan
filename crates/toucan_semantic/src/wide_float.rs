//! Binary128 identities and target-dependent GNU spelling/machine-mode routes.
use crate::{Error, ExtendedFloatFormat, FloatKind, Type, TypeKind};
use toucan_target::{Compiler, Target};

impl FloatKind {
    /// The distinct IEEE binary128 interchange type (`_Float128` in GNU C).
    pub const FLOAT128: Self = Self::Extended {
        format: ExtendedFloatFormat::BinaryInterchange,
        width: 128,
    };
}

pub(crate) fn q_literal_kind(target: Target, compiler: Compiler) -> FloatKind {
    if target == Target::Aarch64UnknownLinuxGnu && compiler == Compiler::Gnu {
        FloatKind::LongDouble
    } else {
        FloatKind::FLOAT128
    }
}

/// GNU's predefined typedef is shadowable and must not mutate already checked types.
pub(crate) fn predefined_type(name: &str, target: Target, compiler: Compiler) -> Option<FloatKind> {
    (name == "__float128" && target == Target::X86_64UnknownLinuxGnu && compiler == Compiler::Gnu)
        .then_some(FloatKind::FLOAT128)
}

impl crate::analyze::Analyzer {
    pub(crate) fn floating_machine_mode(
        &self,
        ty: &Type,
        mode: &str,
        offset: usize,
    ) -> Result<Option<Type>, Error> {
        let (kind, complex) = match mode {
            "SF" => (FloatKind::Float, false),
            "DF" => (FloatKind::Double, false),
            "SC" => (FloatKind::Float, true),
            "DC" => (FloatKind::Double, true),
            "TF" | "TC" => {
                let kind = match self.unit.target {
                    Target::X86_64UnknownLinuxGnu => FloatKind::FLOAT128,
                    Target::Aarch64UnknownLinuxGnu => FloatKind::LongDouble,
                    _ => {
                        return Err(Error::new(
                            offset,
                            "TF/TC floating machine modes are unavailable in this target profile",
                        ));
                    }
                };
                (kind, mode == "TC")
            }
            _ => return Ok(None),
        };
        let source = &self.unit.resolve(ty)?.kind;
        if !matches!(source, TypeKind::Float(_) | TypeKind::Complex(_))
            || (complex && !matches!(source, TypeKind::Complex(_)))
            || (!complex
                && matches!(source, TypeKind::Complex(_))
                && self.unit.compiler == Compiler::Gnu)
        {
            return Err(Error::new(
                offset,
                "floating machine mode has an incompatible source type",
            ));
        }
        let mut result = ty.clone();
        result.kind = if complex {
            TypeKind::Complex(kind)
        } else {
            TypeKind::Float(kind)
        };
        Ok(Some(result))
    }
}

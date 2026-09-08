//! Nominal C floating identities, separate from intermediate evaluation precision.
use crate::{Error, ExtendedFloatFormat, FloatKind};
use lang_c::ast;

impl FloatKind {
    /// The TS 18661 `_Float16` type, distinct from `__bf16` and ARM `__fp16`.
    pub const FLOAT16: Self = Self::Extended {
        format: ExtendedFloatFormat::BinaryInterchange,
        width: 16,
    };

    /// Whether this type stores IEEE binary16 or bfloat16 values.
    /// Its nominal type does not prescribe runtime intermediate precision.
    pub const fn is_narrow(self) -> bool {
        matches!(self, Self::FLOAT16 | Self::BFloat16)
    }
}

pub(crate) fn literal_kind(format: &ast::FloatFormat, offset: usize) -> Result<FloatKind, Error> {
    Ok(match format {
        ast::FloatFormat::Float => FloatKind::Float,
        ast::FloatFormat::Double => FloatKind::Double,
        ast::FloatFormat::LongDouble => FloatKind::LongDouble,
        ast::FloatFormat::TS18661Format(ast::TS18661FloatType {
            format: ast::TS18661FloatFormat::BinaryInterchange,
            width: 16,
        }) => FloatKind::FLOAT16,
        _ => {
            return Err(Error::new(
                offset,
                "extended floating literal format is unsupported",
            ));
        }
    })
}

/// Selects the common nominal floating type before any integer promotion.
/// Both supported compiler families rank `_Float16` above `__bf16`.
pub(crate) fn common_kind(
    left: Option<FloatKind>,
    right: Option<FloatKind>,
    offset: usize,
) -> Result<Option<FloatKind>, Error> {
    let rank = |kind| match kind {
        FloatKind::BFloat16 => Ok(0),
        FloatKind::FLOAT16 => Ok(1),
        FloatKind::Float => Ok(2),
        FloatKind::Double => Ok(3),
        FloatKind::LongDouble => Ok(4),
        _ => Err(Error::new(
            offset,
            "extended floating arithmetic is unsupported",
        )),
    };
    Ok(match (left, right) {
        (None, None) => None,
        (Some(kind), None) | (None, Some(kind)) => {
            rank(kind)?;
            Some(kind)
        }
        (Some(left), Some(right)) => Some(if rank(left)? >= rank(right)? {
            left
        } else {
            right
        }),
    })
}

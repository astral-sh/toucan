//! Nominal C floating identities, separate from intermediate evaluation precision.
use crate::{Error, ExtendedFloatFormat, FloatKind};
use lang_c::ast;

impl FloatKind {
    /// The TS 18661 `_Float16` type, distinct from `__bf16` and ARM `__fp16`.
    pub const FLOAT16: Self = Self::Extended {
        format: ExtendedFloatFormat::BinaryInterchange,
        width: 16,
    };

    /// The distinct TS 18661 binary32 interchange type.
    pub const FLOAT32: Self = Self::Extended {
        format: ExtendedFloatFormat::BinaryInterchange,
        width: 32,
    };
    /// The distinct TS 18661 binary64 interchange type.
    pub const FLOAT64: Self = Self::Extended {
        format: ExtendedFloatFormat::BinaryInterchange,
        width: 64,
    };
    /// The TS 18661 extended binary32 type, stored as binary64 on supported GNU targets.
    pub const FLOAT32X: Self = Self::Extended {
        format: ExtendedFloatFormat::BinaryExtended,
        width: 32,
    };
    /// The TS 18661 extended binary64 type: x87 on GNU x86-64 and binary128 on GNU AArch64.
    pub const FLOAT64X: Self = Self::Extended {
        format: ExtendedFloatFormat::BinaryExtended,
        width: 64,
    };

    /// Whether this type stores IEEE binary16 or bfloat16 values.
    /// Its nominal type does not prescribe runtime intermediate precision.
    pub const fn is_narrow(self) -> bool {
        matches!(self, Self::FLOAT16 | Self::BFloat16)
    }
}

pub(crate) fn literal_kind(
    format: &ast::FloatFormat,
    target: toucan_target::Target,
    compiler: toucan_target::Compiler,
    offset: usize,
) -> Result<FloatKind, Error> {
    Ok(match format {
        ast::FloatFormat::Float128 => crate::wide_float::q_literal_kind(target, compiler),
        ast::FloatFormat::TS18661Format(ast::TS18661FloatType {
            format: ast::TS18661FloatFormat::BinaryInterchange,
            width: 128,
        }) => {
            if compiler != toucan_target::Compiler::Gnu {
                return Err(Error::new(
                    offset,
                    "the Clang profile rejects the f128 floating literal suffix",
                ));
            }
            FloatKind::FLOAT128
        }
        ast::FloatFormat::TS18661Format(ast::TS18661FloatType {
            format,
            width: width @ (32 | 64),
        }) if matches!(
            format,
            ast::TS18661FloatFormat::BinaryInterchange | ast::TS18661FloatFormat::BinaryExtended
        ) =>
        {
            if compiler != toucan_target::Compiler::Gnu {
                return Err(Error::new(
                    offset,
                    "the Clang profile rejects GNU f32/f64/f32x/f64x floating suffixes",
                ));
            }
            match (format, width) {
                (ast::TS18661FloatFormat::BinaryInterchange, 32) => FloatKind::FLOAT32,
                (ast::TS18661FloatFormat::BinaryInterchange, 64) => FloatKind::FLOAT64,
                (ast::TS18661FloatFormat::BinaryExtended, 32) => FloatKind::FLOAT32X,
                _ => FloatKind::FLOAT64X,
            }
        }
        ast::FloatFormat::Float => FloatKind::Float,
        ast::FloatFormat::Double => FloatKind::Double,
        ast::FloatFormat::LongDouble => FloatKind::LongDouble,
        ast::FloatFormat::TS18661Format(ast::TS18661FloatType {
            format: ast::TS18661FloatFormat::BinaryInterchange,
            width: 16,
        }) => {
            if target == toucan_target::Target::I686UnknownLinuxGnu {
                return Err(Error::new(
                    offset,
                    "the f16 floating literal suffix is unavailable for _Float16 on i686 GNU Linux",
                ));
            }
            FloatKind::FLOAT16
        }
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
/// At equal precision GNU prefers interchange types, then standard types,
/// then extended types. The ordering below holds on both supported GNU architectures.
pub(crate) fn common_kind(
    left: Option<FloatKind>,
    right: Option<FloatKind>,
    offset: usize,
) -> Result<Option<FloatKind>, Error> {
    let rank = |kind| match kind {
        FloatKind::BFloat16 => Ok(0),
        FloatKind::FLOAT16 => Ok(1),
        FloatKind::Float => Ok(2),
        FloatKind::FLOAT32 => Ok(3),
        FloatKind::FLOAT32X => Ok(4),
        FloatKind::Double => Ok(5),
        FloatKind::FLOAT64 => Ok(6),
        FloatKind::FLOAT64X => Ok(7),
        FloatKind::LongDouble => Ok(8),
        FloatKind::FLOAT128 => Ok(9),
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

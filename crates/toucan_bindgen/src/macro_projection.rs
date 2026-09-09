//! Rust representations for the adapter's untyped macro values.

use std::{fmt, io, str::FromStr};

use toucan::MacroValue;

use crate::macro_values::{Character, Value};

/// Default integer representation for parsed macro constants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MacroTypeVariation {
    /// Prefer signed integers, widening when the value requires it.
    Signed,
    /// Prefer unsigned integers for nonnegative values.
    #[default]
    Unsigned,
}

impl fmt::Display for MacroTypeVariation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Signed => "signed",
            Self::Unsigned => "unsigned",
        })
    }
}

impl FromStr for MacroTypeVariation {
    type Err = io::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "signed" => Ok(Self::Signed),
            "unsigned" => Ok(Self::Unsigned),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Got an invalid MacroTypeVariation. Accepted values are 'signed' and 'unsigned'",
            )),
        }
    }
}

/// Choose an output representation without attributing a C type to the value.
pub(crate) fn project(
    value: Value,
    variation: MacroTypeVariation,
    fit: bool,
) -> Result<Option<MacroValue>, &'static str> {
    Ok(Some(match value {
        Value::Invalid => return Ok(None),
        Value::Bytes(bytes) => MacroValue::String(bytes),
        Value::Float(value) => MacroValue::RustFloat(value),
        Value::Integer(value) => {
            let signed = value < 0 || variation == MacroTypeVariation::Signed;
            let bits = [8u8, 16, 32, 64]
                .into_iter()
                .filter(|&bits| fit || bits >= 32)
                .find(|&bits| {
                    if signed {
                        let bound = 1i128 << (bits - 1);
                        (-bound..bound).contains(&i128::from(value))
                    } else {
                        (value as u128) < (1u128 << bits)
                    }
                })
                .expect("a Rust 64-bit integer represents the parsed i64 domain");
            MacroValue::RustInteger {
                value: u128::from(value as u64) & ((1u128 << bits) - 1),
                bits,
                signed,
            }
        }
        Value::Character(character) => {
            let value = match character {
                Character::Unicode(value) if value.is_ascii() => value as u8,
                Character::Raw(value) => u8::try_from(value)
                    .map_err(|_| "character macro exceeds bindgen's one-byte representation")?,
                Character::Unicode(_) => {
                    return Err("character macro exceeds bindgen's one-byte representation");
                }
            };
            MacroValue::RustInteger {
                value: u128::from(value),
                bits: 8,
                signed: false,
            }
        }
    }))
}

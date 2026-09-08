//! C language modes, independent of the target ABI and compiler family.
use crate::LayoutError;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Selects keyword and preprocessing defaults; it does not enable pedantic diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum LanguageMode {
    /// ISO C11 keywords and compiler-specific standard-mode preprocessing defaults.
    #[serde(rename = "c11")]
    C11,
    /// GNU C11 keywords and preprocessing defaults.
    #[default]
    #[serde(rename = "gnu11")]
    Gnu11,
}
impl LanguageMode {
    /// Supported language modes. Compiler families remain a separate choice.
    pub const ALL: [Self; 2] = [Self::C11, Self::Gnu11];
}
impl fmt::Display for LanguageMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::C11 => "c11",
            Self::Gnu11 => "gnu11",
        })
    }
}
impl FromStr for LanguageMode {
    type Err = LayoutError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "c11" => Ok(Self::C11),
            "gnu11" => Ok(Self::Gnu11),
            _ => Err(LayoutError::UnsupportedLanguageModeName(value.into())),
        }
    }
}

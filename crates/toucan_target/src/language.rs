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
    /// ISO C90 syntax with the selected compiler's non-pedantic extensions.
    #[serde(rename = "c90")]
    C90,
    /// GNU C90 syntax and preprocessing defaults.
    #[serde(rename = "gnu90")]
    Gnu90,
}
impl LanguageMode {
    /// Supported language modes. Compiler families remain a separate choice.
    pub const ALL: [Self; 4] = [Self::C11, Self::Gnu11, Self::C90, Self::Gnu90];

    /// Whether bare GNU extension keywords and nonstandard platform macros are enabled.
    pub const fn is_gnu(self) -> bool {
        matches!(self, Self::Gnu11 | Self::Gnu90)
    }

    /// Whether the mode selects C11 keyword and preprocessing rules.
    pub const fn is_c11(self) -> bool {
        matches!(self, Self::C11 | Self::Gnu11)
    }

    /// Whether C90 implicit-int declaration rules apply.
    pub const fn is_c90(self) -> bool {
        matches!(self, Self::C90 | Self::Gnu90)
    }
}
impl fmt::Display for LanguageMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::C11 => "c11",
            Self::Gnu11 => "gnu11",
            Self::C90 => "c90",
            Self::Gnu90 => "gnu90",
        })
    }
}
impl FromStr for LanguageMode {
    type Err = LayoutError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "c11" => Ok(Self::C11),
            "gnu11" => Ok(Self::Gnu11),
            "c89" | "c90" | "iso9899:1990" => Ok(Self::C90),
            "gnu89" | "gnu90" => Ok(Self::Gnu90),
            _ => Err(LayoutError::UnsupportedLanguageModeName(value.into())),
        }
    }
}

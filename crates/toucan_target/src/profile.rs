//! Compiler behavior selected independently from the physical C target.
use crate::{BuiltinType, LanguageMode, Layout, LayoutError, Target, Type};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// A supported compiler family. This selects semantics, not an installed compiler.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Compiler {
    /// GNU C semantics and extensions.
    #[serde(rename = "gcc")]
    Gnu,
    /// Clang C semantics and extensions.
    #[serde(rename = "clang")]
    Clang,
}
impl fmt::Display for Compiler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Gnu => "gcc",
            Self::Clang => "clang",
        })
    }
}
impl FromStr for Compiler {
    type Err = LayoutError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "gcc" => Ok(Self::Gnu),
            "clang" => Ok(Self::Clang),
            _ => Err(LayoutError::UnsupportedCompilerName(value.into())),
        }
    }
}

/// A validated target, compiler family, and C language mode. GCC is supported on
/// validated Linux targets; ARMv7 hard-float currently supports Clang only.
/// Clang uses the Microsoft ABI on Windows.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ProfileFields")]
pub struct CompilerProfile {
    target: Target,
    compiler: Compiler,
    language_mode: LanguageMode,
}
#[derive(Deserialize)]
struct ProfileFields {
    target: Target,
    compiler: Compiler,
    #[serde(default)]
    language_mode: LanguageMode,
}
impl TryFrom<ProfileFields> for CompilerProfile {
    type Error = LayoutError;
    fn try_from(value: ProfileFields) -> Result<Self, Self::Error> {
        Self::new(value.target, value.compiler)
            .map(|profile| profile.with_language_mode(value.language_mode))
    }
}
impl CompilerProfile {
    /// Supported profiles. The original seven entries retain their order; musl
    /// profiles follow them. Fuzz campaign manifests record the selector count.
    pub const ALL: [Self; 15] = [
        Self::default_for(Target::X86_64UnknownLinuxGnu),
        Self::default_for(Target::Aarch64UnknownLinuxGnu),
        Self::default_for(Target::X86_64AppleDarwin),
        Self::default_for(Target::Aarch64AppleDarwin),
        Self::default_for(Target::X86_64PcWindowsMsvc),
        Self {
            target: Target::X86_64UnknownLinuxGnu,
            compiler: Compiler::Clang,
            language_mode: LanguageMode::Gnu11,
        },
        Self {
            target: Target::Aarch64UnknownLinuxGnu,
            compiler: Compiler::Clang,
            language_mode: LanguageMode::Gnu11,
        },
        Self::default_for(Target::X86_64UnknownLinuxMusl),
        Self::default_for(Target::Aarch64UnknownLinuxMusl),
        Self {
            target: Target::X86_64UnknownLinuxMusl,
            compiler: Compiler::Clang,
            language_mode: LanguageMode::Gnu11,
        },
        Self {
            target: Target::Aarch64UnknownLinuxMusl,
            compiler: Compiler::Clang,
            language_mode: LanguageMode::Gnu11,
        },
        Self::default_for(Target::Aarch64PcWindowsMsvc),
        Self::default_for(Target::I686UnknownLinuxGnu),
        Self {
            target: Target::I686UnknownLinuxGnu,
            compiler: Compiler::Clang,
            language_mode: LanguageMode::Gnu11,
        },
        Self::default_for(Target::Armv7UnknownLinuxGnueabihf),
    ];
    /// Rejects compiler/target pairs whose semantics and ABI have not been validated.
    pub fn new(target: Target, compiler: Compiler) -> Result<Self, LayoutError> {
        if compiler == Compiler::Gnu
            && (!target.is_linux() || target == Target::Armv7UnknownLinuxGnueabihf)
        {
            return Err(LayoutError::UnsupportedCompiler { target, compiler });
        }
        Ok(Self {
            target,
            compiler,
            language_mode: LanguageMode::Gnu11,
        })
    }
    /// GCC is the Linux default where its target semantics are validated.
    pub const fn default_for(target: Target) -> Self {
        Self {
            target,
            language_mode: LanguageMode::Gnu11,
            compiler: if target.is_linux() && !target.is_armv7() {
                Compiler::Gnu
            } else {
                Compiler::Clang
            },
        }
    }
    /// Physical ABI, operating system, and architecture.
    pub const fn target(self) -> Target {
        self.target
    }
    /// Compiler family governing extensions, builtins, and layout differences.
    pub const fn compiler(self) -> Compiler {
        self.compiler
    }
    /// Selects C keyword and preprocessing defaults without changing the target ABI.
    pub const fn with_language_mode(mut self, mode: LanguageMode) -> Self {
        self.language_mode = mode;
        self
    }
    /// Language mode used for preprocessing, parsing, and expression reparsing.
    pub const fn language_mode(self) -> LanguageMode {
        self.language_mode
    }
    /// Trigraph default before an explicit preprocessing override. Clang's Microsoft
    /// compatibility mode leaves trigraphs disabled in both ISO standard modes.
    pub const fn default_trigraphs(self) -> bool {
        !self.language_mode.is_gnu() && !self.target.is_windows()
    }
    /// Computes a scalar layout under this validated profile.
    pub fn builtin_layout(self, builtin: BuiltinType) -> Result<Layout, LayoutError> {
        self.layout(&Type::builtin(builtin))
    }
    /// Computes object layout without substituting another physical target.
    pub fn layout(self, ty: &Type) -> Result<Layout, LayoutError> {
        let input = self.target.lower(ty, 0, self.compiler)?;
        let target = self.target.abi_target();
        let compiler = match self.compiler {
            Compiler::Gnu => repc::Compiler::Gcc,
            Compiler::Clang if self.target.is_windows() => repc::system_compiler(target),
            Compiler::Clang => repc::Compiler::Clang,
        };
        Ok(Layout::from_abi(&repc::compute_layout_with_compiler(
            target, compiler, &input,
        )?))
    }
}

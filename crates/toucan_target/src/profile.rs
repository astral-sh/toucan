//! Compiler behavior selected independently from the physical C target.
use crate::{BuiltinType, Layout, LayoutError, Target, Type};
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

/// A validated physical target and compiler pair. GCC is supported on Linux;
/// Clang is supported on all five targets, using the Microsoft ABI on Windows.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ProfileFields")]
pub struct CompilerProfile {
    target: Target,
    compiler: Compiler,
}
#[derive(Deserialize)]
struct ProfileFields {
    target: Target,
    compiler: Compiler,
}
impl TryFrom<ProfileFields> for CompilerProfile {
    type Error = LayoutError;
    fn try_from(value: ProfileFields) -> Result<Self, Self::Error> {
        Self::new(value.target, value.compiler)
    }
}
impl CompilerProfile {
    /// Supported profiles, with the five target defaults followed by Clang on Linux.
    /// This order also defines the semantic fuzzers' seven selector buckets.
    pub const ALL: [Self; 7] = [
        Self::default_for(Target::X86_64UnknownLinuxGnu),
        Self::default_for(Target::Aarch64UnknownLinuxGnu),
        Self::default_for(Target::X86_64AppleDarwin),
        Self::default_for(Target::Aarch64AppleDarwin),
        Self::default_for(Target::X86_64PcWindowsMsvc),
        Self {
            target: Target::X86_64UnknownLinuxGnu,
            compiler: Compiler::Clang,
        },
        Self {
            target: Target::Aarch64UnknownLinuxGnu,
            compiler: Compiler::Clang,
        },
    ];
    /// Rejects compiler/target pairs whose semantics and ABI have not been validated.
    pub fn new(target: Target, compiler: Compiler) -> Result<Self, LayoutError> {
        if compiler == Compiler::Gnu
            && !matches!(
                target,
                Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
            )
        {
            return Err(LayoutError::UnsupportedCompiler { target, compiler });
        }
        Ok(Self { target, compiler })
    }
    /// Preserves the original target defaults: GCC on Linux, Clang elsewhere.
    pub const fn default_for(target: Target) -> Self {
        Self {
            target,
            compiler: match target {
                Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu => Compiler::Gnu,
                _ => Compiler::Clang,
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
            Compiler::Clang if self.target == Target::X86_64PcWindowsMsvc => {
                repc::system_compiler(target)
            }
            Compiler::Clang => repc::Compiler::Clang,
        };
        Ok(Layout::from_abi(&repc::compute_layout_with_compiler(
            target, compiler, &input,
        )?))
    }
}

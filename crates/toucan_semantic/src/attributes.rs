//! Shared attribute spellings, feature queries, and symbol-identity constraints.
use std::borrow::Cow;
use std::collections::HashMap;

use lang_c::span::Span;
use toucan_target::{Compiler, CompilerProfile, Target};

use crate::{Declaration, Error, TranslationUnit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Attribute {
    MinimumVectorWidth,
    Target,
    AlwaysInline,
    GnuInline,
    NoInline,
    NoEscape,
    NoDebug,
    TransparentUnion,
    ReturnsTwice,
    NoReturn,
    Weak,
    Warning,
    Error,
    DiagnoseIf,
    EnableIf,
    Mode,
    VectorSize,
    Packed,
    Aligned,
    Aarch64VectorPcs,
    Aarch64SvePcs,
    Cdecl,
    Stdcall,
    Fastcall,
    Thiscall,
    MsAbi,
    SysvAbi,
    Ignored,
}
impl Attribute {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "min_vector_width" => Self::MinimumVectorWidth,
            "target" => Self::Target,
            "always_inline" => Self::AlwaysInline,
            "gnu_inline" => Self::GnuInline,
            "noinline" => Self::NoInline,
            "noescape" => Self::NoEscape,
            "nodebug" => Self::NoDebug,
            "transparent_union" => Self::TransparentUnion,
            "returns_twice" => Self::ReturnsTwice,
            "noreturn" => Self::NoReturn,
            "weak" => Self::Weak,
            "warning" => Self::Warning,
            "error" => Self::Error,
            "diagnose_if" => Self::DiagnoseIf,
            "enable_if" => Self::EnableIf,
            "mode" => Self::Mode,
            "vector_size" => Self::VectorSize,
            "packed" => Self::Packed,
            "aligned" => Self::Aligned,
            "aarch64_vector_pcs" => Self::Aarch64VectorPcs,
            "aarch64_sve_pcs" => Self::Aarch64SvePcs,
            "cdecl" => Self::Cdecl,
            "stdcall" => Self::Stdcall,
            "fastcall" => Self::Fastcall,
            "thiscall" => Self::Thiscall,
            "ms_abi" => Self::MsAbi,
            "sysv_abi" => Self::SysvAbi,
            "nothrow"
            | "leaf"
            | "nonnull"
            | "format"
            | "format_arg"
            | "warn_unused_result"
            | "malloc"
            | "alloc_size"
            | "alloc_align"
            | "access"
            | "deprecated"
            | "pure"
            | "const"
            | "visibility"
            | "sentinel"
            | "unused"
            | "used"
            | "artificial"
            | "returns_nonnull"
            | "cold"
            | "hot"
            | "may_alias"
            | "noclone"
            | "no_sanitize"
            | "no_sanitize_address"
            | "no_sanitize_thread"
            | "no_sanitize_undefined"
            | "fallthrough"
            | "warn_unused"
            | "externally_visible"
            | "nonnull_all"
            | "warn_if_not_aligned" => Self::Ignored,
            _ => return None,
        })
    }
}

/// Whether the frontend implements the attribute's source-level semantics.
/// Argument and subject constraints remain checked when an attribute is used.
/// Known spellings whose constraints are currently ignored return zero.
pub fn has_attribute(profile: CompilerProfile, name: &str) -> u64 {
    let name = name
        .strip_prefix("__")
        .and_then(|n| n.strip_suffix("__"))
        .unwrap_or(name);
    let Some(attribute) = Attribute::from_name(name) else {
        return 0;
    };
    use Attribute::*;
    u64::from(match attribute {
        Ignored | DiagnoseIf | EnableIf => false,
        MinimumVectorWidth | NoEscape | NoDebug => profile.compiler() == Compiler::Clang,
        Aarch64VectorPcs => {
            profile.compiler() == Compiler::Clang
                || profile.target().is_aarch64() && profile.target().is_linux()
        }
        Aarch64SvePcs => profile.compiler() == Compiler::Clang,
        Cdecl | SysvAbi if profile.target() == toucan_target::Target::I686UnknownLinuxGnu => true,
        Stdcall | Fastcall | Thiscall | MsAbi
            if profile.target() == toucan_target::Target::I686UnknownLinuxGnu =>
        {
            false
        }
        Cdecl | Stdcall | Fastcall | Thiscall | MsAbi | SysvAbi => {
            profile.compiler() == Compiler::Clang
                || profile.target().is_x86_64() && profile.target().is_linux()
        }
        _ => true,
    })
}

/// Rejects assembler labels shared with another C name when an attribute needs
/// a unique symbol identity. Diagnostics point to the supplied attribute span,
/// including for names declared only at block scope.
pub(crate) fn validate_symbol_aliases<'a>(
    unit: &TranslationUnit,
    attributed: impl Iterator<Item = (&'a str, Span)>,
    message: &str,
) -> Result<(), Error> {
    let mut attributed = attributed.peekable();
    if attributed.peek().is_none()
        || !unit
            .declarations
            .iter()
            .any(|item| item.link_name.is_some())
    {
        return Ok(());
    }
    let declarations = unit
        .declarations
        .iter()
        .map(|item| (item.name.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut symbols = HashMap::new();
    for (name, span) in attributed {
        let label = symbol_name(unit.target, name, declarations.get(name).copied());
        if let Some((previous, _)) = symbols.insert(label, (name, span))
            && previous != name
        {
            return Err(Error::new(span.start, message));
        }
    }
    for declaration in &unit.declarations {
        let label = symbol_name(unit.target, &declaration.name, Some(declaration));
        if let Some((owner, span)) = symbols.get(&label)
            && *owner != declaration.name
        {
            return Err(Error::new(span.start, message));
        }
    }
    Ok(())
}

/// Compare assembler names, including ordinary target prefixes and literal labels.
/// Block-only attributed declarations have no file declaration and use their C name.
fn symbol_name<'a>(
    target: Target,
    name: &'a str,
    declaration: Option<&'a Declaration>,
) -> Cow<'a, str> {
    let label = declaration
        .and_then(|item| item.link_name.as_deref())
        .unwrap_or(name);
    if matches!(
        target,
        Target::X86_64AppleDarwin | Target::Aarch64AppleDarwin
    ) && !declaration.is_some_and(|item| item.link_name_is_literal)
    {
        format!("_{label}").into()
    } else {
        label.into()
    }
}

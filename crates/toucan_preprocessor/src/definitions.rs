//! Command-line macro bodies enter preprocessing separately from physical files.
use std::borrow::Cow;

/// Interpretation of caller-provided predefined macro replacement text.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PredefinedMacroMode {
    /// Replacement tokens supplied directly, preserving the standalone API default.
    #[default]
    Tokens,
    /// GNU `-D`: stop at the first physical newline and remove comments.
    GnuCommandLine,
    /// Clang `-D`: GNU handling plus trigraphs when enabled for preprocessing.
    ClangCommandLine,
}

pub(crate) fn prepare(
    source: &str,
    mode: PredefinedMacroMode,
    trigraphs: bool,
) -> Result<Cow<'_, str>, String> {
    if mode == PredefinedMacroMode::Tokens {
        return Ok(Cow::Borrowed(source));
    }
    // Drivers truncate the body before translation phases. A definition cannot
    // splice into another caller definition or an in-memory forced include.
    let source = source.split(['\r', '\n']).next().unwrap_or_default();
    let trigraphs = trigraphs && mode == PredefinedMacroMode::ClangCommandLine;
    if !(trigraphs && source.contains("??")) && !source.contains("/*") && !source.contains("//") {
        return Ok(Cow::Borrowed(source));
    }
    Ok(Cow::Owned(
        super::token::normalize(source, trigraphs)?.source,
    ))
}

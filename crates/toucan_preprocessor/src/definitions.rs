//! Command-line macro bodies enter preprocessing separately from physical files.
use std::borrow::Cow;

use crate::Config;
use crate::comments::CommentState;

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

pub(crate) fn prepare<'a>(
    source: &'a str,
    mode: PredefinedMacroMode,
    trigraphs: bool,
    comments: &mut CommentState,
) -> Result<Cow<'a, str>, String> {
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
        super::token::normalize_with_comments(source, trigraphs, comments, false, |_, _, _, _| {
            Ok(())
        })?
        .source,
    ))
}

/// Normalizes ordered command-line definitions before putting them in a macro map.
///
/// Clang C90 source compilation keeps line-comment state across `-D` arguments,
/// even when a later `-U` removes a definition. Call [`Self::prepare`] for every
/// definition in argument order, apply `-U` to the map separately, then set
/// [`Config::predefined_macro_mode`] to [`PredefinedMacroMode::Tokens`] so the
/// resulting definitions do not repeat translation phases. Physical files each
/// start with independent comment state.
pub struct CommandLineMacroNormalizer {
    mode: PredefinedMacroMode,
    trigraphs: bool,
    comments: CommentState,
    bytes: usize,
    max_bytes: usize,
}

impl CommandLineMacroNormalizer {
    /// Copies preprocessing policy and the source budget, without copying macros.
    /// Normalization has its own cumulative input budget; preprocessing then
    /// charges the resulting macro map and physical sources against its budget.
    pub fn new(config: &Config) -> Self {
        Self {
            mode: config.predefined_macro_mode,
            trigraphs: config.trigraphs,
            comments: CommentState::new(config.line_comments),
            bytes: config.defines.iter().fold(0usize, |bytes, (name, value)| {
                bytes
                    .saturating_add(name.len())
                    .saturating_add(value.len())
                    .saturating_add(1)
            }),
            max_bytes: config.max_source_bytes,
        }
    }

    /// Returns a normalized map key and replacement. The cumulative budget counts
    /// original spelling, including comments and definitions later removed by `-U`.
    pub fn prepare(&mut self, name: &str, value: &str) -> Result<(String, String), String> {
        self.bytes = self
            .bytes
            .saturating_add(name.len())
            .saturating_add(value.len())
            .saturating_add(1);
        if self.bytes > self.max_bytes {
            return Err("source byte limit exceeded".into());
        }
        if name.contains(['\r', '\n']) {
            return Err("predefined macro name contains a newline".into());
        }
        let definition = format!("{name} {value}");
        let prepared = prepare(&definition, self.mode, self.trigraphs, &mut self.comments)?;
        let prepared = prepared.trim_start();
        let (name, value) = prepared
            .split_once(char::is_whitespace)
            .unwrap_or((prepared, ""));
        Ok((name.to_owned(), value.to_owned()))
    }
}

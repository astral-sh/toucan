//! Line-comment behavior during translation phase three.

/// Interpretation of `//` in physical source files and command-line definitions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LineComments {
    /// C99 and later, and GNU language modes: `//` begins a comment.
    #[default]
    Enabled,
    /// GCC's ISO C90 mode: retain slash tokens, and diagnose `//` in active
    /// ordinary source. Unused macro replacement lists may contain these tokens.
    GnuC90,
    /// GCC's ISO C90 preprocessing-only (`-E`) behavior: ordinary source still
    /// diagnoses `//`, while pragma payloads retain slash tokens for the compiler.
    GnuC90Preprocessing,
    /// Clang's ISO C90 source compilation extension. Initially `//**/` spells
    /// division followed by a block comment; the first other `//` enables line
    /// comments for the rest of that physical file, including skipped groups.
    ClangC90,
    /// Clang's ISO C90 preprocessing-only (`-E`) behavior: retain slash tokens.
    /// This deliberately differs from [`Self::ClangC90`] source compilation.
    ClangC90Preprocessing,
}

pub(crate) struct CommentState {
    mode: LineComments,
    enabled: bool,
}

impl CommentState {
    pub(crate) fn new(mode: LineComments) -> Self {
        Self {
            mode,
            enabled: mode == LineComments::Enabled,
        }
    }

    /// Called at two adjacent slashes, before consuming either slash.
    pub(crate) fn line_comment(&mut self, third: Option<char>) -> bool {
        if self.mode == LineComments::ClangC90 && third != Some('*') {
            self.enabled = true;
        }
        self.enabled
    }
}

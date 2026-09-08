//! Native, bounded C preprocessing for header consumers.
//!
//! The caller supplies the target's include paths and predefined macros. No host
//! compiler is invoked, and missing includes and unsupported directives are errors.

mod file_identity;
mod file_origins;
mod include_search;
pub use file_origins::{FileMapping, FileOrigins};
mod macro_definitions;
pub use macro_definitions::MacroDefinition;
use macro_definitions::MacroDefinitions;
mod macro_redefinitions;
use macro_redefinitions::{DefinitionLocation, MacroRedefinitions};
pub use macro_redefinitions::{MacroRedefinition, MacroRedefinitionPolicy};

mod comments;
mod definitions;
mod expand;
mod expression;
mod provenance;
mod queries;
mod timestamp;
mod token;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use comments::LineComments;
pub use definitions::{CommandLineMacroNormalizer, PredefinedMacroMode};
pub use provenance::{OriginKind, SourceLocation, SourceMapping};
pub use queries::{FeatureQueries, FeatureQuery, FeatureQueryProvider, QueryDialect};
pub use timestamp::{PreprocessingTimestamp, TimestampError};

use expand::Expansion;
use token::{Kind, Token, lex_limited, lex_with_scope, normalize, render};

/// Include search paths, predefined macros, and per-translation-unit resource limits.
#[derive(Clone, Debug)]
pub struct Config {
    /// Record physical header paths and final macro-definition origins for file selection.
    pub record_file_origins: bool,
    /// Capture successful written definitions in order, including later-undefined macros.
    /// The conservative retained-data estimate is bounded by `max_source_bytes`.
    pub record_macro_definitions: bool,
    /// Strict by default. Compatibility replacement retains bounded diagnostics.
    pub macro_redefinition_policy: MacroRedefinitionPolicy,
    /// Optional compiler feature-query operators. Standalone default: disabled.
    pub feature_queries: Option<FeatureQueries>,
    /// Replace trigraphs before physical line splicing. Standalone default: C11.
    pub trigraphs: bool,
    /// Lex `::` as one punctuator, as in GNU C modes and all Clang C modes.
    /// Defaults to strict C tokenization; adjacent colon pairs still retain the
    /// identity required by GNU attribute-query syntax.
    pub scope_punctuator: bool,
    /// Line-comment policy. Defaults to C99-and-later behavior.
    pub line_comments: LineComments,
    /// How predefined replacement strings are interpreted; physical headers and
    /// forced includes always follow the ordinary source translation phases.
    pub predefined_macro_mode: PredefinedMacroMode,
    /// Whether plain C char is unsigned when interpreting ordinary character
    /// constants in #if. Macro definitions do not change this data-model setting.
    pub char_unsigned: bool,
    /// Fixed UTC timestamp for `__DATE__` and `__TIME__`. Defaults to the Unix
    /// epoch for reproducible embedding. The library never reads the clock or
    /// `SOURCE_DATE_EPOCH`; callers supply another timestamp explicitly.
    pub timestamp: PreprocessingTimestamp,
    /// Permit filesystem reads for entry points, includes, and include queries.
    pub allow_filesystem: bool,
    /// Regular search directories, before system directories.
    pub include_dirs: Vec<PathBuf>,
    /// System search directories. A physical directory listed in both groups
    /// belongs to this group; duplicates are normalized anew for each run.
    pub system_include_dirs: Vec<PathBuf>,
    /// In-memory headers processed in order before the entry point.
    ///
    /// They share macros, resource limits, and source mappings with the entry point.
    /// Their paths locate diagnostics and resolve quoted includes; their contents
    /// are used directly, even when filesystem access is disabled.
    pub forced_includes: Vec<ForcedInclude>,
    /// Resource headers consulted after the caller's include directories.
    /// Keys match header names exactly, without decoding escapes or rewriting separators.
    pub virtual_headers: BTreeMap<String, String>,
    pub defines: BTreeMap<String, String>,
    pub max_include_depth: usize,
    pub max_expansion_depth: usize,
    pub max_tokens: usize,
    /// Cumulative source, expanded replacement, and output byte budget.
    /// Also bounds the retained-data estimate for optional macro-definition capture.
    pub max_source_bytes: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            record_file_origins: false,
            record_macro_definitions: false,
            macro_redefinition_policy: MacroRedefinitionPolicy::Strict,
            feature_queries: None,
            trigraphs: true,
            scope_punctuator: false,
            line_comments: LineComments::Enabled,
            predefined_macro_mode: PredefinedMacroMode::Tokens,
            char_unsigned: false,
            timestamp: PreprocessingTimestamp::UNIX_EPOCH,
            allow_filesystem: true,
            include_dirs: Vec::new(),
            system_include_dirs: Vec::new(),
            forced_includes: Vec::new(),
            virtual_headers: BTreeMap::new(),
            defines: BTreeMap::new(),
            max_include_depth: 64,
            max_expansion_depth: 128,
            max_tokens: 1_000_000,
            max_source_bytes: 64 * 1024 * 1024,
        }
    }
}

impl Config {
    /// Remove a predefined macro or query, as for a command-line `-U` option.
    pub fn undefine(&mut self, name: &str) {
        self.defines
            .retain(|key, _| key.split('(').next() != Some(name));
        if let (Some(queries), Some(kind)) =
            (&mut self.feature_queries, FeatureQuery::from_name(name))
        {
            queries.disable(kind);
        }
    }
}

/// An in-memory header included before the translation unit's entry point.
#[derive(Clone, Debug)]
pub struct ForcedInclude {
    pub path: PathBuf,
    pub source: String,
}

/// A macro definition. Parameters exclude the optional variadic parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Macro {
    pub parameters: Option<Vec<String>>,
    pub variadic: bool,
    pub variadic_parameter: Option<String>,
    pub replacement: String,
}

/// Expanded source, final macro definitions, and files read while preprocessing.
#[derive(Clone, Debug)]
pub struct Preprocessed {
    pub source: String,
    pub macros: BTreeMap<String, Macro>,
    pub dependencies: Vec<PathBuf>,
    /// Source anchors for generated byte ranges. Valid while `source` is unchanged.
    pub mappings: Vec<SourceMapping>,
    file_origins: Option<Box<FileOrigins>>,
    macro_definitions: Option<Box<MacroDefinitions>>,
    macro_redefinitions: Option<Box<MacroRedefinitions>>,
    config: Config,
    active_queries: u8,
    path: PathBuf,
}

impl Preprocessed {
    /// Physical file and macro origins when requested through the preprocessor config.
    pub fn file_origins(&self) -> Option<&FileOrigins> {
        self.file_origins.as_deref()
    }

    /// Successful written `#define` directives in preprocessing order, when requested.
    ///
    /// Independent of file-origin capture. Definitions in source-based forced and
    /// virtual headers are included; configured predefined macros are excluded.
    /// A captured empty source returns `Some(&[])`; disabled capture returns `None`.
    pub fn macro_definitions(&self) -> Option<&[MacroDefinition]> {
        self.macro_definitions
            .as_deref()
            .map(MacroDefinitions::entries)
    }

    /// Accepted incompatible definitions in order. None means strict policy;
    /// an empty slice means compatibility mode saw no incompatible definitions.
    pub fn macro_redefinitions(&self) -> Option<&[MacroRedefinition]> {
        self.macro_redefinitions
            .as_deref()
            .map(MacroRedefinitions::entries)
    }

    /// Whether the final environment contains a macro or active predefined operator.
    pub fn is_defined(&self, name: &str) -> bool {
        self.macros.contains_key(name)
            || is_builtin(name)
            || query_active(self.active_queries, name).is_some()
    }

    /// Resolve a generated byte offset to the source token or macro invocation.
    ///
    /// Offsets inside a token resolve to its start. Separator bytes resolve to the
    /// preceding token; end of output resolves to its final source anchor. Empty
    /// output and invalid UTF-8 byte boundaries have no location.
    pub fn resolve_location(&self, offset: usize) -> Option<&SourceLocation> {
        if !self.source.is_char_boundary(offset) {
            return None;
        }
        if offset == self.source.len() {
            return self.mappings.last().map(|mapping| &mapping.origin);
        }
        let index = self
            .mappings
            .partition_point(|mapping| mapping.generated.end <= offset);
        let mapping = self.mappings.get(index)?;
        mapping
            .generated
            .contains(&offset)
            .then_some(&mapping.origin)
    }

    /// Expand an object macro against the final macro environment.
    ///
    /// Function macros and undefined names return `None`. An unused macro with an
    /// invalid replacement can fail here without invalidating the translation unit.
    /// `__DATE__` and `__TIME__` use the translation unit's configured timestamp.
    /// Stateful `__COUNTER__` and `_Pragma` expansions are rejected in this read-only query.
    pub fn expand_object_macro(&self, name: &str) -> Result<Option<String>, Error> {
        match self.macros.get(name) {
            Some(definition) if definition.parameters.is_some() => return Ok(None),
            None if !matches!(name, "__DATE__" | "__TIME__") => return Ok(None),
            _ => {}
        }
        let mut expansion = Expansion {
            macros: &self.macros,
            active_queries: self.active_queries,
            config: &self.config,
            file: &self.path,
            produced: 0,
            produced_bytes: 0,
            recursion: 0,
            location: None,
            counter: None,
        };
        expansion
            .expand(vec![Token::new(Kind::Identifier, name)])
            .and_then(|tokens| {
                if tokens.iter().any(|token| token.kind == Kind::Pragma) {
                    return Err(
                        "_Pragma cannot be evaluated from the final macro environment".into(),
                    );
                }
                Ok(Some(render(&tokens)))
            })
            .map_err(|message| Error::new(&self.path, 1, message))
    }
}

/// A preprocessing diagnostic located at a source file and logical line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl Error {
    fn new(path: &Path, line: usize, message: impl Into<String>) -> Self {
        Self::at(path, line, 1, message)
    }

    fn at(path: &Path, line: usize, column: usize, message: impl Into<String>) -> Self {
        Self {
            path: path.to_owned(),
            line,
            column,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}: {}",
            self.path.display(),
            self.line,
            self.column,
            self.message
        )
    }
}

impl std::error::Error for Error {}

/// Stateful preprocessor. Each entry point starts a fresh translation unit.
pub struct Preprocessor {
    config: Config,
    include_search: Option<Box<include_search::SearchOrder>>,
    active_queries: u8,
    macros: BTreeMap<String, Macro>,
    dependencies: BTreeMap<PathBuf, Option<Arc<Path>>>,
    once: BTreeSet<PathBuf>,
    hardlinks: BTreeMap<(u64, u64), Arc<Path>>,
    tokens: usize,
    expansion_tokens: usize,
    source_bytes: usize,
    expansion_bytes: usize,
    output_tokens: usize,
    counter: u64,
    mappings: Vec<SourceMapping>,
    file_origins: Option<Box<FileOrigins>>,
    macro_definitions: Option<Box<MacroDefinitions>>,
    macro_redefinitions: Option<Box<MacroRedefinitions>>,
}

/// Separates the read path, shared file identity, access spelling, and main-file rules.
#[derive(Clone, Copy)]
struct InputFile<'a> {
    physical: &'a Path,
    identity: &'a Path,
    accessed: &'a Path,
    main: bool,
    system: bool,
}

impl<'a> InputFile<'a> {
    fn named(path: &'a Path, main: bool) -> Self {
        Self {
            physical: path,
            identity: path,
            accessed: path,
            main,
            system: false,
        }
    }
}

#[derive(Debug)]
struct Conditional {
    parent_active: bool,
    active: bool,
    taken: bool,
    seen_else: bool,
}

/// Keep literal `./` components in the including directory; `Path::parent` removes them.
fn accessed_parent(path: &Path) -> Option<std::borrow::Cow<'_, Path>> {
    use std::borrow::Cow;
    // UTF-8 names need no platform-specific allocation, including on Windows.
    if let Some(name) = path.to_str() {
        return name
            .rfind(std::path::is_separator)
            .map(|index| Cow::Borrowed(Path::new(&name[..index + 1])))
            .or_else(|| path.parent().map(Cow::Borrowed));
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let bytes = path.as_os_str().as_bytes();
        bytes
            .iter()
            .rposition(|&byte| byte == b'/')
            .map(|index| Cow::Borrowed(Path::new(std::ffi::OsStr::from_bytes(&bytes[..index + 1]))))
            .or_else(|| path.parent().map(Cow::Borrowed))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let mut units: Vec<_> = path.as_os_str().encode_wide().collect();
        if let Some(index) = units
            .iter()
            .rposition(|&unit| unit == b'/' as u16 || unit == b'\\' as u16)
        {
            units.truncate(index + 1);
            Some(Cow::Owned(std::ffi::OsString::from_wide(&units).into()))
        } else {
            path.parent().map(Cow::Borrowed)
        }
    }
    #[cfg(not(any(unix, windows)))]
    path.parent().map(Cow::Borrowed)
}

impl Preprocessor {
    /// Configure include resolution and macro expansion without invoking a compiler.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            include_search: None,
            active_queries: 0,
            macros: BTreeMap::new(),
            dependencies: BTreeMap::new(),
            once: BTreeSet::new(),
            hardlinks: BTreeMap::new(),
            tokens: 0,
            expansion_tokens: 0,
            source_bytes: 0,
            expansion_bytes: 0,
            output_tokens: 0,
            counter: 0,
            mappings: Vec::new(),
            file_origins: None,
            macro_definitions: None,
            macro_redefinitions: None,
        }
    }

    /// Read and preprocess a header, resolving includes relative to its accessed directory.
    pub fn preprocess(&mut self, path: &Path) -> Result<Preprocessed, Error> {
        self.preprocess_inputs(path, std::iter::empty())
    }

    /// Preprocess ordered headers in one macro environment.
    ///
    /// The final path is the main file and must exist as written. Earlier paths
    /// behave like compiler `-include` inputs: search the working directory, then
    /// configured include directories. Configured in-memory forced includes run first.
    /// At least one path is required. Main-file spelling is registered before any
    /// forced header, matching Clang's first-name rule for repeated physical files.
    pub fn preprocess_files(&mut self, paths: &[PathBuf]) -> Result<Preprocessed, Error> {
        let (main, headers) = paths.split_last().ok_or_else(|| {
            Error::new(
                Path::new("<input>"),
                1,
                "at least one input header is required",
            )
        })?;
        self.preprocess_inputs(main, headers.iter().map(PathBuf::as_path))
    }

    fn preprocess_inputs<'a>(
        &mut self,
        main: &Path,
        headers: impl Iterator<Item = &'a Path>,
    ) -> Result<Preprocessed, Error> {
        if !self.config.allow_filesystem {
            return Err(Error::new(
                main,
                1,
                "filesystem access is disabled; use preprocess_str with virtual headers",
            ));
        }
        self.reset()?;
        // Clang opens the main file before processing -include inputs. Register
        // its name first, including when a forced input names the same file.
        let physical = fs::canonicalize(main)
            .map_err(|error| Error::new(main, 1, format!("cannot open header: {error}")))?;
        let (file, identity) = self.open_file(&physical, main)?;
        let spelling = self.register_file(&physical, main, identity.as_ref());
        let accessed = if self.clang_paths() {
            spelling.as_deref().unwrap_or(&physical)
        } else {
            main
        };
        let input = InputFile {
            physical: &physical,
            identity: identity.as_deref().unwrap_or(&physical),
            accessed,
            main: true,
            system: false,
        };
        let mut output = String::new();
        self.forced_includes(&mut output)?;
        for header in headers {
            let candidate = std::iter::once((Path::new(".").join(header), None, false))
                .chain(
                    self.include_directories()
                        .map(|(index, directory, system)| {
                            (directory.join(header), Some(index), system)
                        }),
                )
                .find(|(path, _, _)| path.is_file())
                .ok_or_else(|| {
                    Error::new(
                        header,
                        1,
                        "forced header not found; configure the include paths",
                    )
                })?;
            self.file(&candidate.0, 0, candidate.1, candidate.2, &mut output)?;
        }
        self.read_file(input, file, 0, None, &mut output)?;
        Ok(self.finish(main, output))
    }

    fn clang_paths(&self) -> bool {
        self.config.feature_queries.as_ref().map_or_else(
            || self.macros.contains_key("__clang__"),
            |queries| queries.dialect == QueryDialect::Clang,
        )
    }

    /// Register dependencies once and retain only Clang's noncanonical first name.
    fn register_file(
        &mut self,
        physical: &Path,
        accessed: &Path,
        identity: Option<&Arc<Path>>,
    ) -> Option<Arc<Path>> {
        let clang = self.clang_paths();
        if clang
            && let Some(identity) = identity
            && identity.as_ref() != physical
        {
            let spelling = self
                .dependencies
                .get(identity.as_ref())
                .and_then(Clone::clone)
                .unwrap_or_else(|| Arc::clone(identity));
            self.dependencies
                .entry(physical.to_owned())
                .or_insert_with(|| Some(Arc::clone(&spelling)));
            return Some(spelling);
        }
        self.dependencies
            .entry(physical.to_owned())
            .or_insert_with(|| {
                (clang && physical.as_os_str() != accessed.as_os_str()).then(|| Arc::from(accessed))
            })
            .clone()
    }

    /// Preprocess in-memory source. `name` determines diagnostics and quoted includes.
    pub fn preprocess_str(&mut self, name: &Path, source: &str) -> Result<Preprocessed, Error> {
        self.reset()?;
        let mut output = String::new();
        self.forced_includes(&mut output)?;
        self.source(InputFile::named(name, true), source, 0, None, &mut output)?;
        Ok(self.finish(name, output))
    }

    fn forced_includes(&mut self, output: &mut String) -> Result<(), Error> {
        for include in self.config.forced_includes.clone() {
            if !self.once.contains(&include.path) {
                self.source(
                    InputFile::named(&include.path, false),
                    &include.source,
                    0,
                    None,
                    output,
                )?;
            }
        }
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Error> {
        self.include_search = None;
        self.include_search = include_search::SearchOrder::resolve(&self.config)?;
        self.macros.clear();
        self.active_queries = self
            .config
            .feature_queries
            .as_ref()
            .map_or(0, |queries| queries.enabled);
        self.dependencies.clear();
        self.once.clear();
        self.hardlinks.clear();
        self.tokens = 0;
        self.expansion_tokens = 0;
        self.source_bytes = 0;
        self.expansion_bytes = 0;
        self.output_tokens = 0;
        self.counter = 0;
        self.mappings.clear();
        self.file_origins = self.config.record_file_origins.then(Box::default);
        self.macro_definitions = self.config.record_macro_definitions.then(Box::default);
        self.macro_redefinitions = (self.config.macro_redefinition_policy
            == MacroRedefinitionPolicy::RecordAndReplace)
            .then(Box::default);
        self.source_bytes =
            self.config
                .defines
                .iter()
                .fold(0usize, |bytes, (name, replacement)| {
                    bytes
                        .saturating_add(name.len())
                        .saturating_add(replacement.len())
                        .saturating_add(1)
                });
        if self.source_bytes > self.config.max_source_bytes {
            return Err(Error::new(
                Path::new("<predefined>"),
                1,
                "source byte limit exceeded",
            ));
        }
        let mut comments = comments::CommentState::new(self.config.line_comments);
        for (name, replacement) in self.config.defines.clone() {
            if self.config.predefined_macro_mode != PredefinedMacroMode::Tokens
                && name.contains(['\r', '\n'])
            {
                return Err(Error::new(
                    Path::new("<predefined>"),
                    1,
                    "predefined macro name contains a newline",
                ));
            }
            let definition = format!("{name} {replacement}");
            let prepared = definitions::prepare(
                &definition,
                self.config.predefined_macro_mode,
                self.config.trigraphs,
                &mut comments,
            )
            .map_err(|message| Error::new(Path::new("<predefined>"), 1, message))?;
            let definition = lex_limited(
                &prepared,
                self.config.max_tokens.saturating_sub(self.tokens),
                self.config.scope_punctuator,
            )
            .map_err(|message| Error::new(Path::new("<predefined>"), 1, message))?;
            self.tokens += definition.len();
            self.define(&definition, None)
                .map_err(|message| Error::new(Path::new("<predefined>"), 1, message))?;
        }
        Ok(())
    }

    fn finish(&mut self, path: &Path, source: String) -> Preprocessed {
        Preprocessed {
            source,
            macros: self.macros.clone(),
            dependencies: self.dependencies.keys().cloned().collect(),
            mappings: std::mem::take(&mut self.mappings),
            file_origins: self.file_origins.take(),
            macro_definitions: self.macro_definitions.take(),
            macro_redefinitions: self.macro_redefinitions.take(),
            config: self.config.clone(),
            active_queries: self.active_queries,
            path: path.to_owned(),
        }
    }

    fn file(
        &mut self,
        path: &Path,
        depth: usize,
        include_origin: Option<usize>,
        system: bool,
        output: &mut String,
    ) -> Result<(), Error> {
        if depth >= self.config.max_include_depth {
            return Err(Error::new(path, 1, "include depth limit exceeded"));
        }
        let physical = fs::canonicalize(path)
            .map_err(|error| Error::new(path, 1, format!("cannot open header: {error}")))?;
        if self.once.contains(&physical) {
            return Ok(());
        }
        let (file, identity) = self.open_file(&physical, path)?;
        let once_path = identity.as_deref().unwrap_or(&physical);
        if self.once.contains(once_path) {
            // Clang records a distinct hard-link access as a dependency even
            // when once suppresses its contents. GNU records only the first.
            if self.clang_paths() {
                self.register_file(&physical, path, identity.as_ref());
            }
            return Ok(());
        }
        let spelling = self.register_file(&physical, path, identity.as_ref());
        let accessed = if self.clang_paths() {
            spelling.as_deref().unwrap_or(&physical)
        } else {
            path
        };
        self.read_file(
            InputFile {
                physical: &physical,
                identity: once_path,
                accessed,
                main: false,
                system,
            },
            file,
            depth,
            include_origin,
            output,
        )
    }

    /// Inspect and read the same open file, caching identities only for hard links.
    fn open_file(
        &mut self,
        physical: &Path,
        accessed: &Path,
    ) -> Result<(fs::File, Option<Arc<Path>>), Error> {
        let fail = |error| Error::new(accessed, 1, format!("cannot read header: {error}"));
        let file = fs::File::open(physical).map_err(fail)?;
        let identity = file_identity::linked_identity(&file)
            .map_err(fail)?
            .map(|identity| {
                Arc::clone(
                    self.hardlinks
                        .entry(identity)
                        .or_insert_with(|| Arc::from(physical)),
                )
            });
        Ok((file, identity))
    }

    fn read_file(
        &mut self,
        input: InputFile<'_>,
        file: fs::File,
        depth: usize,
        include_origin: Option<usize>,
        output: &mut String,
    ) -> Result<(), Error> {
        if depth >= self.config.max_include_depth {
            return Err(Error::new(
                input.accessed,
                1,
                "include depth limit exceeded",
            ));
        }
        let limit = self
            .config
            .max_source_bytes
            .saturating_sub(self.source_bytes);
        let mut source = String::new();
        file.take((limit as u64).saturating_add(1))
            .read_to_string(&mut source)
            .map_err(|error| {
                Error::new(input.accessed, 1, format!("cannot read header: {error}"))
            })?;
        if source.len() > limit {
            return Err(Error::new(input.accessed, 1, "source byte limit exceeded"));
        }
        self.source(input, &source, depth, include_origin, output)
    }

    fn source(
        &mut self,
        input: InputFile<'_>,
        source: &str,
        depth: usize,
        include_origin: Option<usize>,
        output: &mut String,
    ) -> Result<(), Error> {
        let path = input.physical;
        self.source_bytes = self.source_bytes.saturating_add(source.len());
        if self.source_bytes > self.config.max_source_bytes {
            return Err(Error::new(input.accessed, 1, "source byte limit exceeded"));
        }
        let source = normalize(
            source,
            self.config.trigraphs,
            &mut comments::CommentState::new(self.config.line_comments),
        )
        .map_err(|message| Error::new(input.accessed, 1, message))?;
        let mut conditions: Vec<Conditional> = Vec::new();
        let mut pending = Vec::new();
        let mut line_adjustment = 0i64;
        let mut logical_path = input.accessed.to_owned();
        let mut marker_paths = Vec::new();
        let mut offset = 0;
        for line in source.source.split_inclusive('\n') {
            let start = offset;
            offset += line.len();
            let logical_line = (source.line_at(start) as i64 + line_adjustment) as usize;
            let fail = |message| Error::new(&logical_path, logical_line, message);
            let mut tokens = lex_limited(
                line,
                self.config.max_tokens.saturating_sub(self.tokens),
                self.config.scope_punctuator,
            )
            .map_err(&fail)?;
            for token in &mut tokens {
                token.line =
                    (source.line_at(start + token.offset) as i64 + line_adjustment) as usize;
                token.column = source.column_at(start + token.offset);
            }
            self.tokens = self
                .tokens
                .checked_add(tokens.len())
                .ok_or_else(|| fail("input token count overflow".into()))?;
            if self.tokens > self.config.max_tokens {
                return Err(fail("input token limit exceeded".into()));
            }
            let active = conditions.last().is_none_or(|condition| condition.active);
            if tokens.first().is_none_or(|token| token.text != "#") {
                if active {
                    if matches!(
                        self.config.line_comments,
                        LineComments::GnuC90 | LineComments::GnuC90Preprocessing
                    ) && token::adjacent_slashes(&tokens)
                    {
                        return Err(fail("C++ style comments are not allowed in ISO C90".into()));
                    }
                    if let Some(token) = tokens.first_mut() {
                        token.space = !pending.is_empty();
                    }
                    pending.extend(tokens);
                }
                continue;
            }
            self.flush(&logical_path, input, &mut pending, output)?;
            let Some(directive) = tokens.get(1) else {
                continue;
            };
            let rest = &tokens[2..];
            if active
                && directive.text == "pragma"
                && self.config.line_comments == LineComments::GnuC90
                && token::adjacent_slashes(rest)
            {
                return Err(fail("C++ style comments are not allowed in ISO C90".into()));
            }
            match directive.text.as_str() {
                "if" | "ifdef" | "ifndef" => {
                    if conditions.len() >= self.config.max_include_depth.saturating_mul(4).min(256)
                    {
                        return Err(fail("conditional nesting limit exceeded".into()));
                    }
                    let matches = if !active {
                        false
                    } else if directive.text == "if" {
                        self.condition(&logical_path, input, include_origin, rest, output)
                            .map_err(&fail)?
                    } else {
                        let name = identifier(rest).map_err(&fail)?;
                        let defined = self.macros.contains_key(name)
                            || is_builtin(name)
                            || query_active(self.active_queries, name).is_some();
                        if directive.text == "ifdef" {
                            defined
                        } else {
                            !defined
                        }
                    };
                    conditions.push(Conditional {
                        parent_active: active,
                        active: active && matches,
                        taken: matches,
                        seen_else: false,
                    });
                }
                "elif" => {
                    let condition = conditions
                        .last_mut()
                        .ok_or_else(|| fail("#elif without #if".into()))?;
                    if condition.seen_else {
                        return Err(fail("#elif after #else".into()));
                    }
                    let matches = condition.parent_active
                        && !condition.taken
                        && self
                            .condition(&logical_path, input, include_origin, rest, output)
                            .map_err(&fail)?;
                    condition.active = matches;
                    condition.taken |= matches;
                }
                "else" => {
                    if !rest.is_empty() {
                        return Err(fail("unexpected tokens after #else".into()));
                    }
                    let condition = conditions
                        .last_mut()
                        .ok_or_else(|| fail("#else without #if".into()))?;
                    if condition.seen_else {
                        return Err(fail("duplicate #else".into()));
                    }
                    condition.active = condition.parent_active && !condition.taken;
                    condition.taken = true;
                    condition.seen_else = true;
                }
                "endif" => {
                    if !rest.is_empty() {
                        return Err(fail("unexpected tokens after #endif".into()));
                    }
                    if conditions.pop().is_none() {
                        return Err(fail("#endif without #if".into()));
                    }
                }
                _ if !active => {}
                "define" => {
                    let location = if self.macro_redefinitions.is_some() {
                        rest.first().map(|name| DefinitionLocation {
                            input,
                            line: source.line_at(start + name.offset),
                            column: source.column_at(start + name.offset),
                        })
                    } else {
                        None
                    };
                    self.define(rest, location).map_err(&fail)?;
                    if let Some(definitions) = &mut self.macro_definitions {
                        let name = &rest[0];
                        definitions
                            .record(
                                &name.text,
                                &self.macros[&name.text],
                                input,
                                (
                                    source.line_at(start + name.offset),
                                    source.column_at(start + name.offset),
                                ),
                                self.config.max_source_bytes,
                            )
                            .map_err(&fail)?;
                    }
                    if let Some(origins) = &mut self.file_origins {
                        let name = &rest[0];
                        origins.define(
                            &name.text,
                            path,
                            input.accessed,
                            source.line_at(start + name.offset),
                            source.column_at(start + name.offset),
                        );
                    }
                }
                "undef" => {
                    let name = identifier(rest).map_err(&fail)?;
                    if is_builtin(name) {
                        return Err(fail("cannot undefine a builtin macro".into()));
                    }
                    self.macros.remove(name);
                    if let Some(origins) = &mut self.file_origins {
                        origins.undefine(name);
                    }
                    if let Some(kind) = FeatureQuery::from_name(name) {
                        self.active_queries &= !kind.bit();
                    }
                }
                "include" | "include_next" => {
                    let (name, quoted) = if let Ok(header) = header_name(rest) {
                        header
                    } else {
                        let expanded = self.expand(&logical_path, rest.to_vec()).map_err(&fail)?;
                        header_name(&expanded).map_err(&fail)?
                    };
                    let next = directive.text == "include_next";
                    let start = if next {
                        include_origin.map_or(0, |index| index + 1)
                    } else {
                        0
                    };
                    if let Some((included, origin, system)) = self.find_include(
                        input.accessed,
                        &name,
                        quoted && !next,
                        start,
                        include_origin,
                        input.system,
                    ) {
                        self.file(&included, depth + 1, origin, system, output)?;
                    } else if let Some(source) = self
                        .config
                        .virtual_headers
                        .get(&name)
                        .filter(|_| start <= self.include_directory_count())
                    {
                        if source.len()
                            > self
                                .config
                                .max_source_bytes
                                .saturating_sub(self.source_bytes)
                        {
                            return Err(fail("source byte limit exceeded".into()));
                        }
                        let source = source.clone();
                        if depth + 1 >= self.config.max_include_depth {
                            return Err(fail("include depth limit exceeded".into()));
                        }
                        let name = PathBuf::from(format!("<builtin>/{name}"));
                        if !self.once.contains(&name) {
                            self.source(
                                InputFile {
                                    system: true,
                                    ..InputFile::named(&name, false)
                                },
                                &source,
                                depth + 1,
                                Some(self.include_directory_count()),
                                output,
                            )?;
                        }
                    } else {
                        return Err(fail(format!(
                            "header `{name}` not found; configure the target include paths"
                        )));
                    }
                }
                "error" => return Err(fail(format!("#error {}", render(rest)))),
                "pragma" => {
                    let output_start = output.len();
                    self.pragma(
                        input,
                        SourceLocation {
                            path: Arc::from(logical_path.as_path()),
                            line: logical_line,
                            column: tokens[0].column,
                            kind: OriginKind::Directive,
                        },
                        rest,
                        output,
                    )?;
                    if let Some(origins) = &mut self.file_origins {
                        origins.append(
                            output_start..output.len(),
                            path,
                            input.accessed,
                            input.system,
                        );
                    }
                }
                "line" => {
                    let expanded = self.expand(&logical_path, rest.to_vec()).map_err(&fail)?;
                    let Some(number) = expanded.first() else {
                        return Err(fail("#line requires a line number".into()));
                    };
                    let number: usize = number
                        .text
                        .parse()
                        .map_err(|_| fail("invalid #line number".into()))?;
                    if number == 0 || number > i32::MAX as usize || expanded.len() > 2 {
                        return Err(fail("invalid #line directive".into()));
                    }
                    if let Some(file) = expanded.get(1) {
                        if file.kind != Kind::String || !file.text.starts_with('"') {
                            return Err(fail("#line filename must be a string literal".into()));
                        }
                        logical_path = PathBuf::from(line_filename(&file.text).map_err(&fail)?);
                    }
                    line_adjustment = number as i64 - source.line_at(offset) as i64;
                    continue;
                }
                _ if directive.kind == Kind::Number => {
                    let marker = line_marker(&tokens[1..]).map_err(&fail)?;
                    match marker.transition {
                        1 => {
                            if marker_paths.len() >= self.config.max_include_depth {
                                return Err(fail("line marker nesting limit exceeded".into()));
                            }
                            marker_paths.push(logical_path.clone());
                        }
                        2 => {
                            let parent = marker_paths.pop().ok_or_else(|| {
                                fail("line marker cannot return without entering a file".into())
                            })?;
                            if marker.filename.as_deref() != Some(parent.as_path()) {
                                return Err(fail(
                                    "line marker return filename does not match".into(),
                                ));
                            }
                        }
                        _ => {}
                    }
                    if let Some(filename) = marker.filename {
                        logical_path = filename;
                    }
                    line_adjustment = marker.number as i64 - source.line_at(offset) as i64;
                }
                _ => {
                    return Err(fail(format!(
                        "unsupported preprocessing directive `#{}`",
                        directive.text
                    )));
                }
            }
        }
        if !conditions.is_empty() {
            let logical_line = (source.line_at(offset) as i64 + line_adjustment) as usize;
            return Err(Error::new(
                &logical_path,
                logical_line,
                "unterminated #if group",
            ));
        }
        self.flush(&logical_path, input, &mut pending, output)
    }

    fn pragma(
        &mut self,
        input: InputFile<'_>,
        origin: SourceLocation,
        tokens: &[Token],
        output: &mut String,
    ) -> Result<(), Error> {
        let fail = |message| Error::at(&origin.path, origin.line, origin.column, message);
        match tokens.first().map(|token| token.text.as_str()) {
            None => {}
            Some("once") if tokens.len() == 1 => {
                if input.main && self.clang_paths() {
                    return Ok(());
                }
                let path = if self.config.allow_filesystem {
                    fs::canonicalize(input.identity).unwrap_or_else(|_| input.identity.to_owned())
                } else {
                    input.identity.to_owned()
                };
                self.once.insert(path);
            }
            Some("pack") => {
                let directive = format!("#pragma {}\n", render(tokens));
                if output.len().saturating_add(directive.len()) > self.config.max_source_bytes {
                    return Err(fail("output byte limit exceeded"));
                }
                self.output_tokens = self
                    .output_tokens
                    .saturating_add(tokens.len())
                    .saturating_add(2);
                if self.output_tokens > self.config.max_tokens {
                    return Err(fail("output token limit exceeded"));
                }
                let start = output.len();
                output.push_str(&directive);
                self.mappings.push(SourceMapping {
                    generated: start..output.len(),
                    origin,
                });
            }
            Some("GCC" | "clang")
                if tokens.get(1).is_some_and(|token| {
                    matches!(token.text.as_str(), "diagnostic" | "system_header")
                }) => {}
            Some("message") => {}
            _ => {
                return Err(Error::at(
                    &origin.path,
                    origin.line,
                    origin.column,
                    format!("unsupported pragma: {}", render(tokens)),
                ));
            }
        }
        Ok(())
    }

    fn flush(
        &mut self,
        path: &Path,
        input: InputFile<'_>,
        pending: &mut Vec<Token>,
        output: &mut String,
    ) -> Result<(), Error> {
        if pending.is_empty() {
            return Ok(());
        }
        let output_start = output.len();
        let line = pending[0].line;
        let column = pending[0].column;
        let tokens = self.expand_at(path, std::mem::take(pending))?;
        let mut start = 0;
        for (index, token) in tokens.iter().enumerate() {
            if token.kind != Kind::Pragma {
                continue;
            }
            if start != index {
                self.output_tokens(path, line, column, &tokens[start..index], output)?;
            }
            let payload = lex_with_scope(&token.text, self.config.scope_punctuator)
                .map_err(|message| Error::at(path, token.line, token.column, message))?;
            self.pragma(
                input,
                SourceLocation {
                    path: Arc::from(path),
                    line: token.line,
                    column: token.column,
                    kind: if token.expanded {
                        OriginKind::MacroInvocation
                    } else {
                        OriginKind::Directive
                    },
                },
                &payload,
                output,
            )?;
            start = index + 1;
        }
        if start < tokens.len() || tokens.is_empty() {
            self.output_tokens(path, line, column, &tokens[start..], output)?;
        }
        if let Some(origins) = &mut self.file_origins {
            origins.append(
                output_start..output.len(),
                input.physical,
                input.accessed,
                input.system,
            );
        }
        Ok(())
    }

    fn output_tokens(
        &mut self,
        path: &Path,
        line: usize,
        column: usize,
        tokens: &[Token],
        output: &mut String,
    ) -> Result<(), Error> {
        self.output_tokens = self.output_tokens.saturating_add(tokens.len());
        if self.output_tokens > self.config.max_tokens {
            return Err(Error::new(path, line, "output token limit exceeded"));
        }
        let bytes = tokens
            .iter()
            .fold(output.len(), |bytes, token| {
                bytes.saturating_add(token.text.len()).saturating_add(1)
            })
            .saturating_add(usize::from(tokens.is_empty()));
        if bytes > self.config.max_source_bytes {
            return Err(Error::new(path, line, "output byte limit exceeded"));
        }
        let path: Arc<Path> = Arc::from(path);
        if tokens.is_empty() {
            self.mappings.push(SourceMapping {
                generated: output.len()..output.len() + 1,
                origin: SourceLocation {
                    path,
                    line,
                    column,
                    kind: OriginKind::MacroInvocation,
                },
            });
        } else {
            for (index, token) in tokens.iter().enumerate() {
                if index > 0 {
                    output.push(' ');
                    self.mappings
                        .last_mut()
                        .expect("previous token mapping")
                        .generated
                        .end += 1;
                }
                let start = output.len();
                output.push_str(&token.text);
                self.mappings.push(SourceMapping {
                    generated: start..output.len(),
                    origin: SourceLocation {
                        path: Arc::clone(&path),
                        line: token.line,
                        column: token.column,
                        kind: if token.expanded {
                            OriginKind::MacroInvocation
                        } else {
                            OriginKind::Token
                        },
                    },
                });
            }
            self.mappings
                .last_mut()
                .expect("final token mapping")
                .generated
                .end += 1;
        }
        output.push('\n');
        Ok(())
    }

    fn expand(&mut self, path: &Path, tokens: Vec<Token>) -> Result<Vec<Token>, String> {
        self.expand_at(path, tokens).map_err(|error| error.message)
    }

    fn expand_at(&mut self, path: &Path, tokens: Vec<Token>) -> Result<Vec<Token>, Error> {
        let fallback = tokens
            .first()
            .map_or((1, 1), |token| (token.line, token.column));
        let mut expansion = Expansion {
            macros: &self.macros,
            active_queries: self.active_queries,
            config: &self.config,
            file: path,
            produced: self.expansion_tokens,
            produced_bytes: self.expansion_bytes,
            recursion: 0,
            location: None,
            counter: Some(self.counter),
        };
        let result = expansion.expand(tokens);
        self.expansion_tokens = expansion.produced;
        self.expansion_bytes = expansion.produced_bytes;
        self.counter = expansion.counter.expect("translation unit counter");
        result.map_err(|message| {
            let (line, column) = expansion.location.unwrap_or(fallback);
            Error::at(path, line, column, message)
        })
    }

    fn condition(
        &mut self,
        path: &Path,
        input: InputFile<'_>,
        include_origin: Option<usize>,
        tokens: &[Token],
        output: &mut String,
    ) -> Result<bool, String> {
        let mut replaced = Vec::new();
        let mut position = 0;
        while position < tokens.len() {
            if tokens[position].text != "defined" {
                replaced.push(tokens[position].clone());
                position += 1;
                continue;
            }
            position += 1;
            let parenthesized = tokens.get(position).is_some_and(|token| token.text == "(");
            position += usize::from(parenthesized);
            let name = tokens
                .get(position)
                .filter(|token| token.kind == Kind::Identifier)
                .ok_or("defined requires an identifier")?;
            let defined = self.macros.contains_key(&name.text)
                || is_builtin(&name.text)
                || query_active(self.active_queries, &name.text).is_some();
            replaced.push(Token::new(Kind::Number, if defined { "1" } else { "0" }));
            position += 1;
            if parenthesized {
                if tokens.get(position).is_none_or(|token| token.text != ")") {
                    return Err("missing `)` after defined".into());
                }
                position += 1;
            }
        }
        let replaced = self.has_include(path, input.accessed, include_origin, replaced)?;
        let expanded = self.expand(path, replaced)?;
        let mut ordinary = Vec::with_capacity(expanded.len());
        for token in expanded {
            if token.kind == Kind::Pragma {
                let clang = self.config.feature_queries.as_ref().map_or_else(
                    || self.macros.contains_key("__clang__"),
                    |queries| queries.dialect == QueryDialect::Clang,
                );
                if !clang {
                    return Err("_Pragma is not supported in GCC preprocessing conditions".into());
                }
                let payload = lex_with_scope(&token.text, self.config.scope_punctuator)?;
                let output_start = output.len();
                self.pragma(
                    input,
                    SourceLocation {
                        path: Arc::from(path),
                        line: token.line,
                        column: token.column,
                        kind: if token.expanded {
                            OriginKind::MacroInvocation
                        } else {
                            OriginKind::Directive
                        },
                    },
                    &payload,
                    output,
                )
                .map_err(|error| error.message)?;
                if let Some(origins) = &mut self.file_origins {
                    origins.append(
                        output_start..output.len(),
                        input.physical,
                        input.accessed,
                        input.system,
                    );
                }
            } else {
                ordinary.push(token);
            }
        }
        let expanded = ordinary;
        let wchar_unsigned = self.macros.get("__WCHAR_TYPE__").map(|definition| {
            self.macros.contains_key("__WCHAR_UNSIGNED__")
                || definition
                    .replacement
                    .split_whitespace()
                    .any(|token| token == "unsigned")
        });
        expression::evaluate(
            &self.has_include(path, input.accessed, include_origin, expanded)?,
            wchar_unsigned,
            self.config.char_unsigned,
        )
    }

    fn has_include(
        &mut self,
        path: &Path,
        include_path: &Path,
        include_origin: Option<usize>,
        expanded: Vec<Token>,
    ) -> Result<Vec<Token>, String> {
        let mut replaced = Vec::new();
        let mut position = 0;
        while position < expanded.len() {
            let builtin = expanded[position].text.as_str();
            if !matches!(builtin, "__has_include" | "__has_include_next") {
                replaced.push(expanded[position].clone());
                position += 1;
                continue;
            }
            // Clang restarts include-next queries produced by macro expansion;
            // direct queries retain the including file's search position.
            let origin =
                if self.macros.contains_key("__clang__") && !expanded[position].hidden.is_empty() {
                    None
                } else {
                    include_origin
                };
            position += 1;
            if expanded.get(position).is_none_or(|token| token.text != "(") {
                return Err(format!("{builtin} requires parenthesized header name"));
            }
            position += 1;
            let start = position;
            while expanded
                .get(position)
                .is_some_and(|token| token.text != ")")
            {
                position += 1;
            }
            if position == expanded.len() {
                return Err(format!("unterminated {builtin} expression"));
            }
            let (name, quoted) = if let Ok(header) = header_name(&expanded[start..position]) {
                header
            } else {
                let tokens = self.expand(path, expanded[start..position].to_vec())?;
                header_name(&tokens)?
            };
            let next = builtin == "__has_include_next";
            let start = if next {
                origin.map_or(0, |index| index + 1)
            } else {
                0
            };
            let exists = self
                .find_include(
                    include_path,
                    &name,
                    quoted && !next,
                    start,
                    include_origin,
                    false,
                )
                .is_some()
                || (start <= self.include_directory_count()
                    && self.config.virtual_headers.contains_key(&name));
            replaced.push(Token::new(Kind::Number, if exists { "1" } else { "0" }));
            position += 1;
        }
        Ok(replaced)
    }

    fn include_directory_count(&self) -> usize {
        self.include_search.as_ref().map_or_else(
            || self.config.include_dirs.len() + self.config.system_include_dirs.len(),
            |search| search.len(),
        )
    }

    /// Search positions stay local to this run; configured paths remain unchanged.
    fn include_directories(&self) -> impl Iterator<Item = (usize, &Path, bool)> {
        (0..self.include_directory_count()).map(|position| {
            let index = self
                .include_search
                .as_ref()
                .map_or(position, |search| search.index(position));
            let regular = self.config.include_dirs.len();
            if index < regular {
                (position, self.config.include_dirs[index].as_path(), false)
            } else {
                (
                    position,
                    self.config.system_include_dirs[index - regular].as_path(),
                    true,
                )
            }
        })
    }

    fn find_include(
        &self,
        from: &Path,
        name: &str,
        quoted: bool,
        start: usize,
        parent_origin: Option<usize>,
        parent_system: bool,
    ) -> Option<(PathBuf, Option<usize>, bool)> {
        if !self.config.allow_filesystem {
            return None;
        }
        // Include paths describe the host filesystem even when the caller's
        // predefined macros describe another target. In particular, a Windows
        // backslash stays an ordinary filename character on POSIX hosts.
        let path = Path::new(name);
        if path.is_absolute() {
            return path
                .is_file()
                .then(|| (path.to_owned(), None, parent_system));
        }
        // Clang preserves the parent's search origin for local quoted includes;
        // GCC restarts include_next at the beginning of its include search list.
        let local_origin = self.clang_paths().then_some(parent_origin).flatten();
        let local = quoted.then(|| {
            (
                accessed_parent(from)
                    .as_deref()
                    .filter(|parent| !self.clang_paths() || !parent.as_os_str().is_empty())
                    .unwrap_or(Path::new("."))
                    .join(name),
                local_origin,
                parent_system,
            )
        });
        local
            .into_iter()
            .chain(
                self.include_directories()
                    .skip(start)
                    .map(|(index, directory, system)| {
                        (directory.join(name), Some(index), system || parent_system)
                    }),
            )
            .find(|(path, _, _)| path.is_file())
    }

    fn define(
        &mut self,
        tokens: &[Token],
        location: Option<DefinitionLocation<'_>>,
    ) -> Result<(), String> {
        let name = tokens
            .first()
            .filter(|token| token.kind == Kind::Identifier)
            .ok_or("#define requires an identifier")?;
        if name.text == "defined" || is_builtin(&name.text) {
            return Err(format!("cannot define reserved macro `{}`", name.text));
        }
        let mut position = 1;
        let mut parameters = None;
        let mut variadic_parameter = None;
        if tokens
            .get(position)
            .is_some_and(|token| token.text == "(" && !token.space)
        {
            position += 1;
            let mut names = Vec::new();
            if tokens.get(position).is_none_or(|token| token.text != ")") {
                loop {
                    let parameter = tokens
                        .get(position)
                        .ok_or("unterminated macro parameters")?;
                    if parameter.text == "..." {
                        variadic_parameter = Some("__VA_ARGS__".to_string());
                        position += 1;
                    } else if parameter.kind == Kind::Identifier {
                        if names.contains(&parameter.text) {
                            return Err(format!("duplicate macro parameter `{}`", parameter.text));
                        }
                        position += 1;
                        if tokens
                            .get(position)
                            .is_some_and(|token| token.text == "...")
                        {
                            variadic_parameter = Some(parameter.text.clone());
                            position += 1;
                        } else {
                            names.push(parameter.text.clone());
                        }
                    } else {
                        return Err("expected macro parameter name".into());
                    }
                    let next = tokens
                        .get(position)
                        .ok_or("unterminated macro parameters")?;
                    if next.text == ")" {
                        break;
                    }
                    if next.text != "," || variadic_parameter.is_some() {
                        return Err("expected `)` or `,` in macro parameters".into());
                    }
                    position += 1;
                }
            }
            if tokens.get(position).is_none_or(|token| token.text != ")") {
                return Err("unterminated macro parameters".into());
            }
            position += 1;
            parameters = Some(names);
        }
        let replacement = &tokens[position..];
        if replacement.first().is_some_and(|token| token.text == "##")
            || replacement.last().is_some_and(|token| token.text == "##")
        {
            return Err("`##` cannot begin or end a macro replacement list".into());
        }
        if let Some(parameters) = &parameters {
            for (index, token) in replacement.iter().enumerate() {
                if token.text == "#"
                    && replacement.get(index + 1).is_none_or(|token| {
                        !parameters.contains(&token.text)
                            && Some(&token.text) != variadic_parameter.as_ref()
                    })
                {
                    return Err("`#` must precede a macro parameter".into());
                }
            }
        }
        // Retain whitespace boundaries for later stringification of nested expansions.
        let mut spelling = String::new();
        for token in replacement {
            if token.space && !spelling.is_empty() {
                spelling.push(' ');
            }
            spelling.push_str(token.spelling());
        }
        let definition = Macro {
            parameters,
            variadic: variadic_parameter.is_some(),
            variadic_parameter,
            replacement: spelling,
        };
        if let Some(previous) = self.macros.get(&name.text)
            && !equivalent(previous, &definition, self.config.scope_punctuator)?
        {
            if let Some(records) = &mut self.macro_redefinitions {
                records.record(&name.text, location, self.config.max_source_bytes)?;
            } else {
                return Err(format!(
                    "incompatible redefinition of macro `{}`",
                    name.text
                ));
            }
        }
        if let Some(kind) = FeatureQuery::from_name(&name.text) {
            self.active_queries &= !kind.bit();
        }
        self.macros.insert(name.text.clone(), definition);
        Ok(())
    }
}

fn identifier(tokens: &[Token]) -> Result<&str, String> {
    match tokens {
        [token] if token.kind == Kind::Identifier => Ok(&token.text),
        _ => Err("directive requires exactly one identifier".into()),
    }
}

fn query_active(active: u8, name: &str) -> Option<FeatureQuery> {
    // Keep ordinary identifier expansion independent of query-name lookup.
    if active == 0 {
        return None;
    }
    FeatureQuery::from_name(name).filter(|kind| active & kind.bit() != 0)
}

fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "__FILE__"
            | "__LINE__"
            | "__DATE__"
            | "__TIME__"
            | "__COUNTER__"
            | "_Pragma"
            | "__has_include"
            | "__has_include_next"
    )
}

fn header_name(tokens: &[Token]) -> Result<(String, bool), String> {
    if tokens.iter().any(|token| token.text.contains('\0')) {
        return Err("include names cannot contain NUL bytes".into());
    }
    if let [token] = tokens
        && token.kind == Kind::String
        && token.text.starts_with('"')
    {
        let name = &token.text[1..token.text.len() - 1];
        if name.is_empty() {
            return Err("include names cannot be empty".into());
        }
        // Header names are not C string values: `\n`, `\t`, and `\\` retain
        // their literal spelling for native filesystem and virtual-header lookup.
        return Ok((name.to_owned(), true));
    }
    if tokens.first().is_some_and(|token| token.text == "<")
        && tokens.last().is_some_and(|token| token.text == ">")
        && tokens.len() > 2
    {
        if tokens[1..].iter().any(|token| token.space) {
            return Err("whitespace inside angle-bracket include names is not supported".into());
        }
        let name = tokens[1..tokens.len() - 1]
            .iter()
            .map(|token| token.text.as_str())
            .collect();
        return Ok((name, false));
    }
    Err("#include requires a quoted or angle-bracket header name".into())
}

/// Decode the string literal used by `#line`, which follows ordinary C escape rules.
/// GNU preprocessor output carries source locations as numeric directives.
/// Its flags describe include transitions, warning policy, and C++ linkage;
/// they do not affect this C frontend's token stream or constraint checking.
struct LineMarker {
    number: usize,
    filename: Option<PathBuf>,
    transition: u8,
}

fn line_marker(tokens: &[Token]) -> Result<LineMarker, String> {
    let number = &tokens[0].text;
    if !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("line marker requires a decimal line number".into());
    }
    let number: usize = number.parse().map_err(|_| "invalid line marker number")?;
    if number > i32::MAX as usize {
        return Err("line marker number exceeds 2147483647".into());
    }
    let filename = tokens
        .get(1)
        .map(|token| {
            if token.kind != Kind::String || !token.text.starts_with('"') {
                return Err("line marker filename must be a string literal".into());
            }
            line_filename(&token.text).map(PathBuf::from)
        })
        .transpose()?;
    let mut previous = 0;
    let mut transition = 0;
    for flag in tokens.iter().skip(2) {
        let value = match flag.text.as_str() {
            "1" => 1,
            "2" => 2,
            "3" => 3,
            "4" => 4,
            _ => return Err("invalid line marker flag".into()),
        };
        if value <= previous || (previous == 1 && value == 2) || (value == 4 && previous != 3) {
            return Err("invalid line marker flag order".into());
        }
        if value <= 2 {
            transition = value;
        }
        previous = value;
    }
    Ok(LineMarker {
        number,
        filename,
        transition,
    })
}

fn line_filename(literal: &str) -> Result<String, String> {
    let mut output = Vec::new();
    let mut chars = literal[1..literal.len() - 1].chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\\' {
            let mut bytes = [0; 4];
            output.extend_from_slice(character.encode_utf8(&mut bytes).as_bytes());
            continue;
        }
        let escape = chars.next().ok_or("incomplete escape in #line filename")?;
        let byte = match escape {
            '\\' | '\'' | '"' | '?' => escape as u8,
            'a' => 7,
            'b' => 8,
            'f' => 12,
            'n' => b'\n',
            'r' => b'\r',
            't' => b'\t',
            'v' => 11,
            '0'..='7' | 'x' => {
                let radix = if escape == 'x' { 16 } else { 8 };
                let mut value = escape.to_digit(radix).unwrap_or(0);
                let mut digits = usize::from(escape != 'x');
                while let Some(digit) = chars.peek().and_then(|c| c.to_digit(radix)) {
                    if radix == 8 && digits == 3 {
                        break;
                    }
                    chars.next();
                    digits += 1;
                    value = value
                        .checked_mul(radix)
                        .and_then(|value| value.checked_add(digit))
                        .filter(|value| *value <= 255)
                        .ok_or("escape exceeds one byte in #line filename")?;
                }
                if digits == 0 {
                    return Err("hexadecimal escape requires digits in #line filename".into());
                }
                value as u8
            }
            'u' | 'U' => {
                let mut value = 0;
                for _ in 0..if escape == 'u' { 4 } else { 8 } {
                    let digit = chars
                        .next()
                        .and_then(|c| c.to_digit(16))
                        .ok_or("invalid universal character name in #line filename")?;
                    value = (value << 4) | digit;
                }
                let character = char::from_u32(value)
                    .filter(|_| value >= 0xa0 || matches!(value, 0x24 | 0x40 | 0x60))
                    .ok_or("invalid universal character name in #line filename")?;
                let mut bytes = [0; 4];
                output.extend_from_slice(character.encode_utf8(&mut bytes).as_bytes());
                continue;
            }
            _ => return Err(format!("unsupported escape `\\{escape}` in #line filename")),
        };
        if byte == 0 {
            return Err("NUL bytes in #line filenames are not supported".into());
        }
        output.push(byte);
    }
    String::from_utf8(output).map_err(|_| "non-UTF-8 #line filenames are not supported".into())
}

fn equivalent(left: &Macro, right: &Macro, scope_punctuator: bool) -> Result<bool, String> {
    if left.parameters != right.parameters || left.variadic_parameter != right.variadic_parameter {
        return Ok(false);
    }
    let left = lex_with_scope(&left.replacement, scope_punctuator)?;
    let right = lex_with_scope(&right.replacement, scope_punctuator)?;
    Ok(left.len() == right.len()
        && left
            .iter()
            .zip(&right)
            .enumerate()
            .all(|(index, (left, right))| {
                left.spelling() == right.spelling() && (index == 0 || left.space == right.space)
            }))
}

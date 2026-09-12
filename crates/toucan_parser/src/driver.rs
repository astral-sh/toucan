//! Preprocess and parse C source file into an abstract syntax tree

use std::collections::HashSet;
use std::error;
use std::fmt;
use std::io;
use std::path::Path;
use std::process::Command;

use arena::ArenaExpression;
use ast::{Expression, TranslationUnit};
use env::Env;
use limits::{ParseLimits, ParseStatistics, ResourceKind, ResourceLimit, MAX_RULE_DEPTH};
use loc;
use parser::{
    expression_arena_with_limits, expression_with_limits, translation_unit_with_limits, ParseError,
};
use span::Node;

/// Parser configuration
#[derive(Clone, Debug)]
pub struct Config {
    /// Command used to invoke C preprocessor
    pub cpp_command: String,
    /// Options to pass to the preprocessor program
    pub cpp_options: Vec<String>,
    /// Language flavor to parse
    pub flavor: Flavor,
    /// Recognize bare asm/typeof keywords; underscored forms follow the flavor.
    /// StdC11 always leaves these identifiers available.
    pub gnu_keywords: bool,
    /// Standard keyword set and implicit-int declaration syntax.
    pub standard: Standard,
    /// Recognize Microsoft extension keywords, independently of GNU keywords.
    pub extensions_msvc: bool,
}

/// C standard syntax, independently of the compiler's extension grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Standard {
    C90,
    C99,
    C11,
    C17,
}

impl Config {
    /// Use `gcc` as a pre-processor and enable gcc extensions
    pub fn with_gcc() -> Config {
        Config {
            cpp_command: "gcc".into(),
            cpp_options: vec!["-E".into()],
            flavor: Flavor::GnuC11,
            gnu_keywords: true,
            standard: Standard::C11,
            extensions_msvc: false,
        }
    }

    /// Use `clang` as a pre-processor and enable Clang extensions
    pub fn with_clang() -> Config {
        Config {
            cpp_command: "clang".into(),
            cpp_options: vec!["-E".into()],
            flavor: Flavor::ClangC11,
            gnu_keywords: true,
            standard: Standard::C11,
            extensions_msvc: false,
        }
    }
}

impl Default for Config {
    #[cfg(target_os = "macos")]
    fn default() -> Config {
        Self::with_clang()
    }

    #[cfg(not(target_os = "macos"))]
    fn default() -> Config {
        Self::with_gcc()
    }
}

/// C language flavors
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Flavor {
    /// Strict standard C11
    StdC11,
    /// Standard C11 with GNU extensions
    GnuC11,
    /// Standard C11 with Clang extensions
    ClangC11,
    /// GNU builtin names with the Clang extension grammar enabled.
    ///
    /// Semantic consumers can validate extension availability after parsing.
    GnuC11WithClangExtensions,
}

/// Result of a successful parse
#[derive(Clone, Debug)]
pub struct Parse {
    /// Pre-processed source text
    pub source: String,
    /// Root of the abstract syntax tree
    pub unit: TranslationUnit,
    /// Resource counters for this parser invocation.
    pub statistics: ParseStatistics,
}

/// One complete expression, with spans relative to the supplied source.
#[derive(Clone, Debug)]
pub struct ExpressionParse {
    /// Preprocessed source text.
    pub source: String,
    /// Root of the expression's abstract syntax tree.
    pub expression: Node<Expression>,
    /// Resource counters for this parser invocation.
    pub statistics: ParseStatistics,
}

/// An experimental expression arena, with spans relative to its owned source.
///
/// Binary, assignment, conditional, and comma expressions use arena IDs. Other
/// syntax is retained in owned AST leaves; see [`ArenaExpression`] for details.
#[derive(Debug)]
pub struct ArenaExpressionParse {
    /// Preprocessed source text.
    pub source: String,
    /// Owned arena and the root expression's ID.
    pub expression: ArenaExpression,
    /// Resource counters for this parser invocation.
    pub statistics: ParseStatistics,
}

#[derive(Debug)]
/// Error type returned from `parse`
pub enum Error {
    PreprocessorError(io::Error),
    SyntaxError(SyntaxError),
}

impl From<SyntaxError> for Error {
    fn from(e: SyntaxError) -> Error {
        Error::SyntaxError(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::PreprocessorError(e) => write!(fmt, "preprocessor error: {}", e),
            Error::SyntaxError(e) => write!(fmt, "syntax error: {}", e),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::PreprocessorError(_) => "preprocessor error",
            Error::SyntaxError(_) => "syntax error",
        }
    }
}

/// Syntax error during parsing
#[derive(Debug, Clone)]
pub struct SyntaxError {
    /// Pre-processed source text
    pub source: String,
    /// Line number in the preprocessed source
    pub line: usize,
    /// Column number in the preprocessed source
    pub column: usize,
    /// Byte position in the preproccessed source
    pub offset: usize,
    /// Tokens expected at the error location
    pub expected: HashSet<&'static str>,
    /// Present when a resource limit stopped parsing.
    pub resource: Option<Box<ResourceLimit>>,
    /// Counters up to the error.
    pub statistics: Box<ParseStatistics>,
}

impl SyntaxError {
    /// Report a resource failure before parsing, at the start of the source.
    ///
    /// The diagnostic has no expected tokens and all resource counters are zero.
    fn before_parsing(source: String, resource: ResourceLimit) -> Self {
        Self {
            source,
            line: 1,
            column: 1,
            offset: 0,
            expected: HashSet::new(),
            resource: Some(Box::new(resource)),
            statistics: Box::new(ParseStatistics::default()),
        }
    }

    /// Quoted and comma-separated list of expected tokens
    pub fn format_expected(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut list = self.expected.iter().collect::<Vec<_>>();
        list.sort();
        for (i, t) in list.iter().enumerate() {
            if i > 0 {
                write!(fmt, ", ")?;
            }
            write!(fmt, "'{}'", t)?;
        }

        Ok(())
    }

    pub fn get_location(&self) -> (loc::Location<'_>, Vec<loc::Location<'_>>) {
        loc::get_location_for_offset(&self.source, self.offset)
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let (loc, inc) = self.get_location();
        if let Some(resource) = &self.resource {
            return write!(
                fmt,
                "{} at \"{}\" line {} column {}",
                resource, loc.file, loc.line, self.column
            );
        }
        write!(
            fmt,
            "unexpected token at \"{}\" line {} column {}, expected ",
            loc.file, loc.line, self.column
        )?;
        self.format_expected(fmt)?;
        for loc in inc {
            write!(fmt, "\n  included from {}:{}", loc.file, loc.line)?;
        }
        Ok(())
    }
}

/// Parse a C file using the configured external preprocessor.
///
/// Filenames beginning with `@` are rejected because GCC can interpret even an
/// absolute source path's basename as a compiler response file.
pub fn parse<P: AsRef<Path>>(config: &Config, source: P) -> Result<Parse, Error> {
    let processed = preprocess(config, source.as_ref()).map_err(Error::PreprocessorError)?;
    parse_preprocessed(config, processed).map_err(Error::SyntaxError)
}

pub fn parse_preprocessed(config: &Config, source: String) -> Result<Parse, SyntaxError> {
    parse_preprocessed_with_limits(config, source, ParseLimits::default())
}

/// Parse with deterministic per-invocation resource limits.
pub fn parse_preprocessed_with_limits(
    config: &Config,
    source: String,
    limits: ParseLimits,
) -> Result<Parse, SyntaxError> {
    parse_with(config, source, limits, translation_unit_with_limits).map(
        |(source, unit, statistics)| Parse {
            source,
            unit,
            statistics,
        },
    )
}

/// Parse exactly one preprocessed expression in an existing typedef environment.
///
/// The lookup is called only for identifier tokens; local declarations can shadow
/// inherited names. Preprocessor directives and trailing tokens are rejected.
pub fn parse_expression(
    config: &Config,
    source: String,
    is_typedef: impl FnMut(&str) -> bool + Send,
) -> Result<ExpressionParse, SyntaxError> {
    parse_expression_with_limits(config, source, is_typedef, ParseLimits::default())
}

/// Parse an expression with the same resource and stack limits as a translation
/// unit. Typedef lookups are charged per identifier byte; work performed inside
/// the caller's lookup is outside the parser's resource accounting.
pub fn parse_expression_with_limits(
    config: &Config,
    source: String,
    is_typedef: impl FnMut(&str) -> bool + Send,
    limits: ParseLimits,
) -> Result<ExpressionParse, SyntaxError> {
    parse_with(config, source, limits, |source, env, limits| {
        expression_with_limits(source, env, limits, is_typedef)
    })
    .map(|(source, expression, statistics)| ExpressionParse {
        source,
        expression,
        statistics,
    })
}

/// Parse exactly one preprocessed expression into experimental arena storage.
///
/// This uses the same grammar and typedef lookup as [`parse_expression`]. Arena
/// nodes can be inspected without materializing a recursive AST. Conversion with
/// [`ArenaExpression::into_owned`] allocates the corresponding owned AST nodes.
pub fn parse_expression_arena(
    config: &Config,
    source: String,
    is_typedef: impl FnMut(&str) -> bool + Send,
) -> Result<ArenaExpressionParse, SyntaxError> {
    parse_expression_arena_with_limits(config, source, is_typedef, ParseLimits::default())
}

/// Parse an arena expression with deterministic resource and stack limits.
///
/// The depth limit covers the equivalent owned AST, including syntax retained
/// in owned leaves, so explicit conversion preserves the cleanup depth bound.
pub fn parse_expression_arena_with_limits(
    config: &Config,
    source: String,
    is_typedef: impl FnMut(&str) -> bool + Send,
    limits: ParseLimits,
) -> Result<ArenaExpressionParse, SyntaxError> {
    parse_with(config, source, limits, |source, env, limits| {
        expression_arena_with_limits(source, env, limits, is_typedef)
    })
    .map(|(source, expression, statistics)| ArenaExpressionParse {
        source,
        expression,
        statistics,
    })
}

fn parse_with<T: Send>(
    config: &Config,
    source: String,
    limits: ParseLimits,
    parse: impl FnOnce(&str, &mut Env, ParseLimits) -> Result<(T, ParseStatistics), ParseError> + Send,
) -> Result<(String, T, ParseStatistics), SyntaxError> {
    let failure = if limits.max_rule_depth > MAX_RULE_DEPTH {
        Some(ResourceLimit {
            kind: ResourceKind::RuleDepth,
            offset: 0,
            limit: MAX_RULE_DEPTH as u64,
            observed: limits.max_rule_depth as u64,
        })
    } else if limits.max_ast_depth > 1024 {
        Some(ResourceLimit {
            kind: ResourceKind::AstDepth,
            offset: 0,
            limit: 1024,
            observed: limits.max_ast_depth as u64,
        })
    } else if source.len() > limits.max_input_bytes {
        Some(ResourceLimit {
            kind: ResourceKind::InputBytes,
            offset: 0,
            limit: limits.max_input_bytes as u64,
            observed: source.len() as u64,
        })
    } else {
        None
    };
    if let Some(resource) = failure {
        return Err(SyntaxError::before_parsing(source, resource));
    }
    // Recursive parser frames are larger in debug builds. A fixed stack makes the
    // recursion ceiling independent of the embedding application's caller stack.
    let parsed = with_parser_stack(|| {
        let mut env = match config.flavor {
            Flavor::StdC11 => Env::with_core(),
            Flavor::GnuC11 => Env::with_gnu(),
            Flavor::ClangC11 => Env::with_clang(),
            Flavor::GnuC11WithClangExtensions => Env::with_gnu_and_clang_extensions(),
        };
        env.set_gnu_keywords(config.gnu_keywords);
        env.set_standard(config.standard);
        env.set_msvc_extensions(config.extensions_msvc);
        parse(&source, &mut env, limits)
    });
    match parsed {
        Ok(Ok((value, statistics))) => Ok((source, value, statistics)),
        Ok(Err(err)) => Err(SyntaxError {
            source,
            line: err.line,
            column: err.column,
            offset: err.offset,
            expected: err.expected,
            resource: err.resource.map(Box::new),
            statistics: err.statistics,
        }),
        Err(_) => Err(SyntaxError::before_parsing(
            source,
            ResourceLimit {
                kind: ResourceKind::WorkerThread,
                offset: 0,
                limit: 16 * 1024 * 1024,
                observed: 0,
            },
        )),
    }
}

/// Runs a group of parser calls on one bounded worker stack.
///
/// Nested sessions reuse that stack. Every parse still receives a fresh lexical
/// environment and independent limits. The worker is joined before returning;
/// no idle background thread is retained. This only bounds the parser's stack
/// use, not arbitrary recursion in `operation`. Panics propagate to the caller.
pub fn with_parser_stack<T: Send>(operation: impl FnOnce() -> T + Send) -> io::Result<T> {
    toucan_stack::with_stack(operation)
}

fn preprocess(config: &Config, source: &Path) -> io::Result<String> {
    // GCC forwards the basename to cc1 as -dumpbase, where a leading @ is
    // expanded as a response file even when the source path is absolute.
    if source
        .file_name()
        .is_some_and(|name| name.as_encoded_bytes().starts_with(b"@"))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source filenames starting with `@` are not supported by the external preprocessor driver",
        ));
    }
    // A source path must not become an option or a compiler response file.
    // Prefix only ambiguous names, preserving other paths and __FILE__ values.
    let prefixed = match source.as_os_str().as_encoded_bytes().first() {
        Some(b'-' | b'@') => Some(Path::new(".").join(source)),
        _ => None,
    };
    let output = Command::new(&config.cpp_command)
        .args(&config.cpp_options)
        .arg(prefixed.as_deref().unwrap_or(source))
        .output()?;

    if output.status.success() {
        String::from_utf8(output.stdout).map_err(io::Error::other)
    } else {
        match String::from_utf8(output.stderr) {
            Ok(s) => Err(io::Error::other(s)),
            Err(_) => Err(io::Error::other("cpp error contains invalid utf-8")),
        }
    }
}

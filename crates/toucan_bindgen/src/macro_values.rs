//! Bounded bindgen 0.72.1 macro values, independent of C constant semantics.
//!
//! Only the flat literal parser from cexpr is used. Arithmetic parsing and all
//! context growth have explicit limits; integer division cannot unwind on zero.

use std::collections::BTreeMap;

use toucan::{CompilerProfile, LanguageMode, Target};
use toucan_preprocessor::Macro;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Integer(i64),
    Float(f64),
    Character(Character),
    Bytes(Vec<u8>),
    Invalid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Character {
    Unicode(char),
    Raw(u64),
}

impl Value {
    fn bytes(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.len(),
            _ => 0,
        }
    }

    fn numeric(&self) -> bool {
        matches!(self, Self::Integer(_) | Self::Float(_))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub source_bytes: usize,
    pub tokens: usize,
    /// A smaller requested limit is honored; recursion never exceeds 128.
    pub depth: usize,
    pub string_bytes: usize,
    pub context_entries: usize,
    pub context_bytes: usize,
    pub total_work: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            source_bytes: 65_536,
            tokens: 4_096,
            depth: 128,
            string_bytes: 1_048_576,
            context_entries: 100_000,
            context_bytes: 64 * 1_048_576,
            total_work: 256 * 1_048_576,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    Syntax,
    InvalidLiteral,
    UnknownIdentifier,
    Keyword,
    DivisionByZero,
    SourceLimit,
    TokenLimit,
    DepthLimit,
    StringLimit,
    ContextLimit,
    WorkLimit,
}

/// Byte offset in the normalized replacement. Synthetic function-parameter
/// tokens have `offset: None`, since they precede rather than belong to it.
/// Whole-definition limits, the macro name, and end-of-input also use None.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Error {
    pub kind: ErrorKind,
    pub offset: Option<usize>,
}

impl Error {
    fn new(kind: ErrorKind, offset: Option<usize>) -> Self {
        Self { kind, offset }
    }

    fn permits_alternative(self) -> bool {
        matches!(
            self.kind,
            ErrorKind::Syntax
                | ErrorKind::InvalidLiteral
                | ErrorKind::UnknownIdentifier
                | ErrorKind::Keyword
        )
    }
}

#[derive(Debug)]
pub(crate) struct Parsed {
    pub value: Value,
    /// Successful duplicates update context but do not emit another item.
    pub first_definition: bool,
}

/// Declaration-order values. `#undef` deliberately has no operation here.
/// Failed parsing leaves values unchanged but still consumes work budget.
#[derive(Debug)]
pub(crate) struct Context {
    values: BTreeMap<String, Value>,
    limits: Limits,
    profile: CompilerProfile,
    retained_bytes: usize,
    remaining_work: usize,
}

impl Context {
    pub(crate) fn new(limits: Limits, profile: CompilerProfile) -> Self {
        Self {
            values: BTreeMap::new(),
            limits,
            profile,
            retained_bytes: 0,
            remaining_work: limits.total_work,
        }
    }

    /// Record an already evaluated value, including Invalid. The caller owns
    /// initial-context policy; bindgen mode starts with an empty context.
    pub(crate) fn note(&mut self, name: &str, value: Value) -> Result<Parsed, Error> {
        if value.bytes() > self.limits.string_bytes {
            return Err(Error::new(ErrorKind::StringLimit, None));
        }
        charge(
            &mut self.remaining_work,
            name.len().saturating_add(value.bytes()),
            None,
        )?;
        let previous = self.values.get(name);
        let first_definition = previous.is_none();
        let entry_bytes = name
            .len()
            .saturating_add(size_of::<(String, Value)>())
            .saturating_add(value.bytes());
        let prior_bytes = previous.map_or(0, |value| {
            name.len() + size_of::<(String, Value)>() + value.bytes()
        });
        let retained = self
            .retained_bytes
            .saturating_sub(prior_bytes)
            .checked_add(entry_bytes)
            .filter(|&bytes| bytes <= self.limits.context_bytes)
            .filter(|_| !first_definition || self.values.len() < self.limits.context_entries)
            .ok_or(Error::new(ErrorKind::ContextLimit, None))?;
        self.values.insert(name.into(), value.clone());
        self.retained_bytes = retained;
        Ok(Parsed {
            value,
            first_definition,
        })
    }

    /// Evaluate one captured, unexpanded macro definition. The caller supplies
    /// bindgen's cursor classification: skip when callbacks exist and the name's
    /// final active macro is function-like. Otherwise parse written parameter
    /// tokens too, even if this particular definition was function-like.
    pub(crate) fn define(
        &mut self,
        name: &str,
        definition: &Macro,
        skip_as_function_like: bool,
    ) -> Result<Option<Parsed>, Error> {
        if skip_as_function_like {
            return Ok(None);
        }
        let mut bytes = name.len().saturating_add(definition.replacement.len());
        if let Some(parameters) = &definition.parameters {
            for parameter in parameters {
                bytes = bytes.saturating_add(parameter.len()).saturating_add(1);
            }
            bytes = bytes.saturating_add(5);
        }
        charge(&mut self.remaining_work, bytes, None)?;
        if bytes > self.limits.source_bytes {
            return Err(Error::new(ErrorKind::SourceLimit, None));
        }
        if is_keyword(name, self.profile) {
            return Err(Error::new(ErrorKind::Keyword, None));
        }
        let mut tokens = Vec::new();
        if let Some(parameters) = &definition.parameters {
            push(&mut tokens, Token::punct("(", None), self.limits.tokens)?;
            for (index, parameter) in parameters.iter().enumerate() {
                if index != 0 {
                    push(&mut tokens, Token::punct(",", None), self.limits.tokens)?;
                }
                push(
                    &mut tokens,
                    Token {
                        kind: Kind::Identifier,
                        text: parameter,
                        offset: None,
                    },
                    self.limits.tokens,
                )?;
            }
            if definition.variadic {
                push(&mut tokens, Token::punct("...", None), self.limits.tokens)?;
            }
            push(&mut tokens, Token::punct(")", None), self.limits.tokens)?;
        }
        lex(
            &definition.replacement,
            &mut tokens,
            self.limits.tokens,
            self.profile.language_mode(),
        )?;
        let mut parser = Parser {
            tokens: &tokens,
            position: 0,
            values: &self.values,
            limits: self.limits,
            profile: self.profile,
            work: &mut self.remaining_work,
        };
        let value = parser.expression(0)?;
        if parser.position != tokens.len() {
            return Err(parser.error(ErrorKind::Syntax));
        }
        self.note(name, value).map(Some)
    }
}

fn charge(work: &mut usize, amount: usize, offset: Option<usize>) -> Result<(), Error> {
    *work = work
        .checked_sub(amount)
        .ok_or(Error::new(ErrorKind::WorkLimit, offset))?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Identifier,
    Literal,
    Punctuation,
}

#[derive(Clone, Copy, Debug)]
struct Token<'a> {
    kind: Kind,
    text: &'a str,
    offset: Option<usize>,
}

impl<'a> Token<'a> {
    fn punct(text: &'a str, offset: Option<usize>) -> Self {
        Self {
            kind: Kind::Punctuation,
            text,
            offset,
        }
    }
}

fn push<'a>(tokens: &mut Vec<Token<'a>>, token: Token<'a>, limit: usize) -> Result<(), Error> {
    if tokens.len() >= limit {
        return Err(Error::new(ErrorKind::TokenLimit, token.offset));
    }
    tokens.push(token);
    Ok(())
}

/// The input is normalized replacement text from the preprocessor: comments and
/// line splices are already removed. Preserve maximal preprocessing tokens.
fn lex<'a>(
    source: &'a str,
    tokens: &mut Vec<Token<'a>>,
    limit: usize,
    mode: LanguageMode,
) -> Result<(), Error> {
    let bytes = source.as_bytes();
    let unicode_literals = matches!(
        mode,
        LanguageMode::C11 | LanguageMode::Gnu11 | LanguageMode::C17 | LanguageMode::Gnu17
    );
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        let start = index;
        let quote = ["u8\"", "L\"", "L'", "U\"", "U'", "u\"", "u'", "\"", "'"]
            .iter()
            .find(|prefix| {
                (unicode_literals || !prefix.starts_with(['u', 'U']))
                    && source[index..].starts_with(**prefix)
            });
        let kind = if let Some(prefix) = quote {
            let delimiter = prefix.as_bytes()[prefix.len() - 1];
            index += prefix.len();
            loop {
                match bytes.get(index) {
                    None => return Err(Error::new(ErrorKind::InvalidLiteral, Some(start))),
                    Some(&byte) if byte == delimiter => {
                        index += 1;
                        break;
                    }
                    Some(b'\\') => {
                        index += 1;
                        if index < bytes.len() {
                            index += 1;
                        }
                    }
                    Some(_) => index += 1,
                }
            }
            Kind::Literal
        } else if bytes[index].is_ascii_digit()
            || (bytes[index] == b'.' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
        {
            index += 1;
            while index < bytes.len() {
                let byte = bytes[index];
                if byte.is_ascii_alphanumeric()
                    || matches!(byte, b'_' | b'.')
                    || (matches!(byte, b'+' | b'-')
                        && matches!(bytes[index - 1], b'e' | b'E' | b'p' | b'P'))
                {
                    index += 1;
                } else {
                    break;
                }
            }
            Kind::Literal
        } else if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            Kind::Identifier
        } else {
            let length = [
                "<<=", ">>=", "...", "++", "--", "<<", ">>", "&&", "||", "<=", ">=", "==", "!=",
                "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "->", "##", "::",
            ]
            .iter()
            .find(|punct| source[index..].starts_with(**punct))
            .map_or_else(
                || source[index..].chars().next().unwrap().len_utf8(),
                |punct| punct.len(),
            );
            index += length;
            Kind::Punctuation
        };
        push(
            tokens,
            Token {
                kind,
                text: &source[start..index],
                offset: Some(start),
            },
            limit,
        )?;
    }
    Ok(())
}

struct Parser<'a, 'b> {
    tokens: &'a [Token<'a>],
    position: usize,
    values: &'a BTreeMap<String, Value>,
    limits: Limits,
    profile: CompilerProfile,
    work: &'b mut usize,
}

impl Parser<'_, '_> {
    fn error(&self, kind: ErrorKind) -> Error {
        Error::new(
            kind,
            self.tokens
                .get(self.position)
                .and_then(|token| token.offset),
        )
    }

    fn enter(&mut self, depth: usize) -> Result<(), Error> {
        charge(
            self.work,
            1,
            self.tokens
                .get(self.position)
                .and_then(|token| token.offset),
        )?;
        if depth >= self.limits.depth.min(128) {
            return Err(self.error(ErrorKind::DepthLimit));
        }
        Ok(())
    }

    fn take(&mut self, text: &str) -> bool {
        if self
            .tokens
            .get(self.position)
            .is_some_and(|token| token.kind == Kind::Punctuation && token.text == text)
        {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expression(&mut self, depth: usize) -> Result<Value, Error> {
        self.enter(depth)?;
        let start = self.position;
        match self.numeric(1, depth) {
            Ok(value) => return Ok(value),
            Err(error) if error.permits_alternative() => self.position = start,
            Err(error) => return Err(error),
        }
        if self.take("(") {
            let value = self.expression(depth + 1)?;
            if !self.take(")") {
                return Err(self.error(ErrorKind::Syntax));
            }
            return Ok(value);
        }
        let value = self.atom()?;
        let Value::Bytes(mut bytes) = value else {
            return Ok(value);
        };
        while self.position < self.tokens.len() {
            let before = self.position;
            match self.atom() {
                Ok(Value::Bytes(next)) => {
                    let length = bytes
                        .len()
                        .checked_add(next.len())
                        .filter(|&length| length <= self.limits.string_bytes)
                        .ok_or(self.error(ErrorKind::StringLimit))?;
                    charge(self.work, next.len(), self.tokens[before].offset)?;
                    bytes.reserve(length - bytes.len());
                    bytes.extend_from_slice(&next);
                }
                Ok(_) => {
                    self.position = before;
                    break;
                }
                Err(error) if error.permits_alternative() => {
                    self.position = before;
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(Value::Bytes(bytes))
    }

    fn atom(&mut self) -> Result<Value, Error> {
        let token = *self
            .tokens
            .get(self.position)
            .ok_or(self.error(ErrorKind::Syntax))?;
        charge(self.work, 1, token.offset)?;
        let value = match token.kind {
            Kind::Literal => {
                // Flat parsing has no input-dependent recursion. A token is
                // bounded by source_bytes before cexpr can allocate its buffers.
                charge(self.work, token.text.len(), token.offset)?;
                let (_, value) = cexpr::literal::parse(token.text.as_bytes())
                    .map_err(|_| Error::new(ErrorKind::InvalidLiteral, token.offset))?;
                match value {
                    cexpr::expr::EvalResult::Int(value) => Value::Integer(value.0),
                    cexpr::expr::EvalResult::Float(value) => Value::Float(value),
                    cexpr::expr::EvalResult::Char(cexpr::literal::CChar::Char(value)) => {
                        Value::Character(Character::Unicode(value))
                    }
                    cexpr::expr::EvalResult::Char(cexpr::literal::CChar::Raw(value)) => {
                        Value::Character(Character::Raw(value))
                    }
                    cexpr::expr::EvalResult::Str(value) => Value::Bytes(value),
                    cexpr::expr::EvalResult::Invalid => Value::Invalid,
                }
            }
            Kind::Identifier => {
                if is_keyword(token.text, self.profile) {
                    return Err(Error::new(ErrorKind::Keyword, token.offset));
                }
                charge(self.work, token.text.len(), token.offset)?;
                let value = self
                    .values
                    .get(token.text)
                    .ok_or(Error::new(ErrorKind::UnknownIdentifier, token.offset))?;
                charge(self.work, value.bytes(), token.offset)?;
                value.clone()
            }
            Kind::Punctuation => return Err(Error::new(ErrorKind::Syntax, token.offset)),
        };
        if value.bytes() > self.limits.string_bytes {
            return Err(Error::new(ErrorKind::StringLimit, token.offset));
        }
        self.position += 1;
        Ok(value)
    }

    fn unary(&mut self, depth: usize) -> Result<Value, Error> {
        self.enter(depth)?;
        if self.take("(") {
            let value = self.numeric(1, depth + 1)?;
            if !self.take(")") {
                return Err(self.error(ErrorKind::Syntax));
            }
            return Ok(value);
        }
        for operator in ["+", "-", "~"] {
            if self.take(operator) {
                let value = self.unary(depth + 1)?;
                return match (operator, value) {
                    ("+", value) => Ok(value),
                    ("-", Value::Integer(value)) => Ok(Value::Integer(value.wrapping_neg())),
                    ("-", Value::Float(value)) => Ok(Value::Float(-value)),
                    ("~", Value::Integer(value)) => Ok(Value::Integer(!value)),
                    _ => Err(self.error(ErrorKind::Syntax)),
                };
            }
        }
        let value = self.atom()?;
        if !value.numeric() {
            return Err(self.error(ErrorKind::Syntax));
        }
        Ok(value)
    }

    fn numeric(&mut self, minimum: u8, depth: usize) -> Result<Value, Error> {
        self.enter(depth)?;
        let mut left = self.unary(depth)?;
        while let Some(token) = self.tokens.get(self.position).copied() {
            let precedence = match token.text {
                "|" => 1,
                "^" => 2,
                "&" => 3,
                "<<" | ">>" => 4,
                "+" | "-" => 5,
                "*" | "/" | "%" => 6,
                _ => 0,
            };
            if token.kind != Kind::Punctuation || precedence < minimum {
                break;
            }
            self.position += 1;
            let right = self.numeric(precedence + 1, depth + 1)?;
            left = binary(token, left, right)?;
        }
        Ok(left)
    }
}

fn binary(token: Token<'_>, left: Value, right: Value) -> Result<Value, Error> {
    use Value::{Float, Integer};
    if let (Integer(a), Integer(b)) = (&left, &right) {
        return Ok(Integer(match token.text {
            "+" => a.wrapping_add(*b),
            "-" => a.wrapping_sub(*b),
            "*" => a.wrapping_mul(*b),
            "/" | "%" if *b == 0 => {
                return Err(Error::new(ErrorKind::DivisionByZero, token.offset));
            }
            "/" => a.wrapping_div(*b),
            "%" => a.wrapping_rem(*b),
            "<<" => a.wrapping_shl(*b as u32),
            ">>" => a.wrapping_shr(*b as u32),
            "&" => a & b,
            "^" => a ^ b,
            "|" => a | b,
            _ => return Err(Error::new(ErrorKind::Syntax, token.offset)),
        }));
    }
    let number = |value| match value {
        Integer(value) => value as f64,
        Float(value) => value,
        _ => unreachable!("numeric parser operands"),
    };
    let a = number(left);
    let b = number(right);
    Ok(Float(match token.text {
        "+" => a + b,
        "-" => a - b,
        "*" => a * b,
        "/" => a / b,
        "%" => a % b,
        _ => return Err(Error::new(ErrorKind::Syntax, token.offset)),
    }))
}

/// Cursor-token keywords for the pinned Clang 18 profile. Compiler family does
/// not change bindgen's lexical policy; target and language mode do. The source
/// roster and all 24 native mode/target probes are archived with this layer.
fn is_keyword(name: &str, profile: CompilerProfile) -> bool {
    let mode = profile.language_mode();
    let c99 = !matches!(mode, LanguageMode::C90 | LanguageMode::Gnu90);
    let microsoft = profile.target() == Target::X86_64PcWindowsMsvc;
    match name {
        "asm" | "typeof" => mode.is_gnu(),
        "inline" => c99 || mode.is_gnu(),
        "restrict" => c99,
        "_Alignas"
        | "_Alignof"
        | "_Atomic"
        | "_BitInt"
        | "_Bool"
        | "_Complex"
        | "_Decimal128"
        | "_Decimal32"
        | "_Decimal64"
        | "_ExtInt"
        | "_Float16"
        | "_Generic"
        | "_Imaginary"
        | "_Nonnull"
        | "_Noreturn"
        | "_Null_unspecified"
        | "_Nullable"
        | "_Nullable_result"
        | "_Static_assert"
        | "_Thread_local"
        | "__FUNCTION__"
        | "__PRETTY_FUNCTION__"
        | "__alignof"
        | "__alignof__"
        | "__asm"
        | "__asm__"
        | "__attribute"
        | "__attribute__"
        | "__auto_type"
        | "__bf16"
        | "__builtin_COLUMN"
        | "__builtin_FILE"
        | "__builtin_FILE_NAME"
        | "__builtin_FUNCTION"
        | "__builtin_LINE"
        | "__builtin_available"
        | "__builtin_bit_cast"
        | "__builtin_choose_expr"
        | "__builtin_convertvector"
        | "__builtin_offsetof"
        | "__builtin_omp_required_simd_align"
        | "__builtin_types_compatible_p"
        | "__builtin_va_arg"
        | "__builtin_vectorelements"
        | "__cdecl"
        | "__complex"
        | "__complex__"
        | "__const"
        | "__const__"
        | "__extension__"
        | "__fastcall"
        | "__float128"
        | "__fp16"
        | "__func__"
        | "__funcref"
        | "__ibm128"
        | "__imag"
        | "__imag__"
        | "__inline"
        | "__inline__"
        | "__int128"
        | "__is_destructible"
        | "__is_nothrow_destructible"
        | "__label__"
        | "__module_private__"
        | "__objc_no"
        | "__objc_yes"
        | "__pascal"
        | "__private_extern__"
        | "__real"
        | "__real__"
        | "__regcall"
        | "__restrict"
        | "__restrict__"
        | "__signed"
        | "__signed__"
        | "__stdcall"
        | "__thiscall"
        | "__thread"
        | "__typeof"
        | "__typeof__"
        | "__vectorcall"
        | "__volatile"
        | "__volatile__"
        | "auto"
        | "break"
        | "case"
        | "char"
        | "const"
        | "continue"
        | "default"
        | "do"
        | "double"
        | "else"
        | "enum"
        | "extern"
        | "float"
        | "for"
        | "goto"
        | "if"
        | "int"
        | "long"
        | "register"
        | "return"
        | "short"
        | "signed"
        | "sizeof"
        | "static"
        | "struct"
        | "switch"
        | "typedef"
        | "union"
        | "unsigned"
        | "void"
        | "volatile"
        | "while" => true,
        "L__FUNCSIG__"
        | "L__FUNCTION__"
        | "__FUNCDNAME__"
        | "__FUNCSIG__"
        | "__builtin_FUNCSIG"
        | "__builtin_alignof"
        | "__declspec"
        | "__finally"
        | "__forceinline"
        | "__if_exists"
        | "__if_not_exists"
        | "__int16"
        | "__int32"
        | "__int64"
        | "__int8"
        | "__interface"
        | "__is_interface_class"
        | "__is_sealed"
        | "__leave"
        | "__multiple_inheritance"
        | "__ptr32"
        | "__ptr64"
        | "__single_inheritance"
        | "__sptr"
        | "__super"
        | "__try"
        | "__unaligned"
        | "__uptr"
        | "__uuidof"
        | "__virtual_inheritance"
        | "__w64"
        | "__wchar_t"
        | "_alignof"
        | "_asm"
        | "_cdecl"
        | "_declspec"
        | "_fastcall"
        | "_finally"
        | "_forceinline"
        | "_inline"
        | "_int16"
        | "_int32"
        | "_int64"
        | "_int8"
        | "_leave"
        | "_multiple_inheritance"
        | "_ptr32"
        | "_ptr64"
        | "_restrict"
        | "_stdcall"
        | "_thiscall"
        | "_try"
        | "_unaligned"
        | "_uptr"
        | "_uuidof"
        | "_vectorcall"
        | "_virtual_inheritance"
        | "_w64"
        | "static_assert" => microsoft,
        _ => false,
    }
}

#[cfg(test)]
mod reference_tests {
    use super::*;
    use std::collections::HashMap;
    use std::num::Wrapping;

    /// The unbounded reference is used only on these small, fixed-depth inputs;
    /// integer zero division is excluded and tested against our checked result.
    #[test]
    fn arithmetic_and_string_grammar_matches_cexpr_on_bounded_inputs() {
        let identifiers = HashMap::from([
            (
                b"Integer".to_vec(),
                cexpr::expr::EvalResult::Int(Wrapping(-7)),
            ),
            (
                b"String".to_vec(),
                cexpr::expr::EvalResult::Str(b"abc".to_vec()),
            ),
            (b"Invalid".to_vec(), cexpr::expr::EvalResult::Invalid),
        ]);
        let reference = cexpr::expr::IdentifierParser::new(&identifiers);
        let mut context = Context::new(
            Limits::default(),
            CompilerProfile::default_for(Target::X86_64UnknownLinuxGnu),
        );
        context.note("Integer", Value::Integer(-7)).unwrap();
        context
            .note("String", Value::Bytes(b"abc".to_vec()))
            .unwrap();
        context.note("Invalid", Value::Invalid).unwrap();
        let mut cases = Vec::new();
        for left in [
            "0",
            "1",
            "-1",
            "0xffffffffffffffff",
            "9223372036854775807",
            "1.5f",
            "Integer",
            "String",
            "Invalid",
            "'x'",
        ] {
            for right in [
                "1", "-1", "63", "64", "1.5f", "Integer", "String", "Invalid", "'x'",
            ] {
                for operator in [
                    "+", "-", "*", "/", "%", "<<", ">>", "&", "|", "^", "&&", "||", "==",
                ] {
                    cases.push(format!("{left} {operator} {right}"));
                    cases.push(format!("({left}) {operator} ({right})"));
                }
            }
        }
        for value in [
            "String", "Invalid", "\"text\"", "'x'", "Integer", "2*3+7>>1", "~1.0",
        ] {
            for depth in 0..24 {
                cases.push(format!("{}{value}{}", "(".repeat(depth), ")".repeat(depth)));
            }
        }
        cases.extend(
            [
                "String \"suffix\"",
                "\"prefix\" String",
                "(String) \"suffix\"",
                "1+",
                "(1",
                "1)",
                "'x'+1",
                "String String Integer",
            ]
            .map(str::to_owned),
        );
        for source in cases {
            let mut tokens = Vec::new();
            lex(&source, &mut tokens, 4096, LanguageMode::Gnu11).unwrap();
            let reference_tokens = std::iter::once(cexpr::token::Token::from((
                cexpr::token::Kind::Identifier,
                &b"result"[..],
            )))
            .chain(tokens.iter().map(|token| {
                cexpr::token::Token::from((
                    match token.kind {
                        Kind::Identifier => cexpr::token::Kind::Identifier,
                        Kind::Literal => cexpr::token::Kind::Literal,
                        Kind::Punctuation => cexpr::token::Kind::Punctuation,
                    },
                    token.text.as_bytes(),
                ))
            }))
            .collect::<Vec<_>>();
            let expected = reference
                .macro_definition(&reference_tokens)
                .ok()
                .map(|(_, (_, value))| format!("{value:?}"));
            let definition = Macro {
                parameters: None,
                variadic: false,
                variadic_parameter: None,
                replacement: source.clone(),
            };
            let actual = context
                .define("result", &definition, false)
                .ok()
                .flatten()
                .map(|parsed| {
                    let value = match parsed.value {
                        Value::Integer(value) => cexpr::expr::EvalResult::Int(Wrapping(value)),
                        Value::Float(value) => cexpr::expr::EvalResult::Float(value),
                        Value::Bytes(value) => cexpr::expr::EvalResult::Str(value),
                        Value::Character(Character::Unicode(value)) => {
                            cexpr::expr::EvalResult::Char(cexpr::literal::CChar::Char(value))
                        }
                        Value::Character(Character::Raw(value)) => {
                            cexpr::expr::EvalResult::Char(cexpr::literal::CChar::Raw(value))
                        }
                        Value::Invalid => cexpr::expr::EvalResult::Invalid,
                    };
                    format!("{value:?}")
                });
            assert_eq!(actual, expected, "{source}");
        }
    }
}

//! Handwritten, token-based C parser.

use std::collections::HashSet;
use std::fmt;

use ast::*;
use env::Env;
use limits::{Budget, ParseLimits, ParseStatistics, ResourceLimit};
use measure::Measure;
use span::{Node, Span};

mod declaration;
mod expression;
mod lexer;
mod statement;

use self::lexer::{Token, TokenKind};

type PResult<T> = Result<T, ()>;

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct ParseError {
    pub resource: Option<ResourceLimit>,
    pub statistics: Box<ParseStatistics>,
    pub line: usize,
    pub column: usize,
    pub offset: usize,
    pub expected: HashSet<&'static str>,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "error at {}:{}: expected ", self.line, self.column)?;
        let mut expected: Vec<_> = self.expected.iter().copied().collect();
        expected.sort_unstable();
        if expected.is_empty() {
            f.write_str("EOF")
        } else {
            write!(f, "{}", expected.join(" or "))
        }
    }
}

impl ::std::error::Error for ParseError {}

struct Parser<'s, 'e> {
    source: &'s str,
    tokens: Vec<Token>,
    cursor: usize,
    last_end: usize,
    env: &'e mut Env,
    budget: Budget,
    syntax_error: Option<(usize, &'static str)>,
}

impl<'s, 'e> Parser<'s, 'e> {
    fn new(source: &'s str, env: &'e mut Env, limits: ParseLimits) -> Self {
        let mut budget = Budget::new(limits);
        let tokens = lexer::lex(source, &mut budget);
        Self {
            source,
            tokens,
            cursor: 0,
            last_end: 0,
            env,
            budget,
            syntax_error: None,
        }
    }

    fn token(&self) -> Token {
        self.peek(0)
    }

    fn peek(&self, distance: usize) -> Token {
        self.tokens
            .get(self.cursor.saturating_add(distance))
            .copied()
            .unwrap_or(Token {
                kind: TokenKind::End,
                span: Span::span(self.source.len(), self.source.len()),
            })
    }

    /// Compare digraph delimiters by their ordinary spelling without rewriting source spans.
    fn token_text(&self, token: Token) -> &'s str {
        let text = &self.source[token.span.start..token.span.end];
        if token.kind == TokenKind::Digraph {
            match text {
                "<:" => "[",
                ":>" => "]",
                "<%" => "{",
                "%>" => "}",
                _ => text,
            }
        } else {
            text
        }
    }

    fn text(&self) -> &'s str {
        self.token_text(self.token())
    }

    fn at(&self, text: &str) -> bool {
        self.text() == text
    }

    fn position(&self) -> usize {
        self.token().span.start
    }

    fn end(&self) -> usize {
        self.last_end
    }

    fn bump(&mut self) -> PResult<Token> {
        let token = self.token();
        if !self.budget.step(token.span.start) {
            return Err(());
        }
        if token.kind == TokenKind::End {
            return self.fail("token");
        }
        self.cursor += 1;
        self.last_end = token.span.end;
        Ok(token)
    }

    fn eat(&mut self, text: &str) -> PResult<bool> {
        if self.at(text) {
            self.bump()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn expect(&mut self, text: &'static str) -> PResult<Token> {
        if self.at(text) {
            self.bump()
        } else {
            self.fail(text)
        }
    }

    /// Record a terminal syntax failure; callers propagate it without trying alternatives.
    fn fail<T>(&mut self, expected: &'static str) -> PResult<T> {
        self.syntax_error = Some((self.position(), expected));
        Err(())
    }

    fn node<T: Measure>(&mut self, value: T, start: usize) -> PResult<Node<T>> {
        self.node_span(value, Span::span(start, self.end()))
    }

    fn node_span<T: Measure>(&mut self, value: T, span: Span) -> PResult<Node<T>> {
        self.budget.node(value, span).map_err(|_| ())
    }

    fn identifier(&mut self) -> PResult<Node<Identifier>> {
        if self.token().kind != TokenKind::Identifier || self.env.reserved.contains(self.text()) {
            return self.fail("identifier");
        }
        let token = self.bump()?;
        let name = self.token_text(token).to_owned();
        self.node_span(Identifier { name }, token.span)
    }

    /// Bounds recursive descent independently of the caller's stack size.
    fn nested<T>(&mut self, parse: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        let start = self.position();
        if !self.budget.enter(start) {
            return Err(());
        }
        let result = parse(self);
        self.budget.leave(
            start,
            if result.is_ok() {
                Some(self.end())
            } else {
                None
            },
        );
        result
    }

    /// Removes lexical scopes even when their contents fail to parse.
    fn scoped<T>(&mut self, parse: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        self.env.enter_scope();
        let result = parse(self);
        self.env.leave_scope();
        result
    }

    fn translation_unit(&mut self) -> PResult<TranslationUnit> {
        let mut declarations = Vec::new();
        while self.token().kind != TokenKind::End {
            let previous = self.cursor;
            if let Some(declaration) = self.external_declaration()? {
                declarations.push(declaration);
            }
            self.progress(previous)?;
        }
        if self.budget.failure.is_some() {
            return Err(());
        }
        Ok(TranslationUnit(declarations))
    }

    /// A successful repetition must consume input; budget failures remain fatal.
    fn progress(&mut self, previous: usize) -> PResult<()> {
        if !self.budget.step(self.position()) {
            return Err(());
        }
        if previous == self.cursor {
            self.fail("parser progress")
        } else {
            Ok(())
        }
    }

    fn error(self) -> ParseError {
        let (error_offset, expected) = self.syntax_error.unzip();
        let offset = self
            .budget
            .failure
            .map_or(error_offset.unwrap_or(0), |resource| resource.offset);
        let before = &self.source[..offset];
        ParseError {
            resource: self.budget.failure,
            statistics: Box::new(self.budget.statistics),
            line: before.bytes().filter(|&b| b == b'\n').count() + 1,
            column: before.chars().rev().take_while(|&c| c != '\n').count() + 1,
            offset,
            expected: expected.into_iter().collect(),
        }
    }
}

fn parse<T>(
    source: &str,
    env: &mut Env,
    limits: ParseLimits,
    parse: impl FnOnce(&mut Parser) -> PResult<T>,
) -> Result<(T, ParseStatistics), ParseError> {
    let depth = env.symbols.len();
    let mut parser = Parser::new(source, env, limits);
    let result = parse(&mut parser).and_then(|value| {
        if parser.budget.failure.is_some() {
            Err(())
        } else if parser.token().kind != TokenKind::End {
            parser.fail("end of input")
        } else {
            Ok(value)
        }
    });
    parser.env.symbols.truncate(depth);
    parser.env.finish_function_definition(None);
    match result {
        Ok(value) => Ok((value, parser.budget.statistics)),
        Err(()) => Err(parser.error()),
    }
}

pub(crate) fn translation_unit_with_limits(
    source: &str,
    env: &mut Env,
    limits: ParseLimits,
) -> Result<(TranslationUnit, ParseStatistics), ParseError> {
    parse(source, env, limits, |p| p.translation_unit())
}

#[cfg(test)]
pub(crate) fn translation_unit(source: &str, env: &mut Env) -> Result<TranslationUnit, ParseError> {
    translation_unit_with_limits(source, env, ParseLimits::default()).map(|(value, _)| value)
}

#[cfg(test)]
pub(crate) fn constant(source: &str, env: &mut Env) -> Result<Constant, ParseError> {
    parse(source, env, ParseLimits::default(), |p| p.constant()).map(|(value, _)| value)
}

#[cfg(test)]
pub(crate) fn string_literal(source: &str, env: &mut Env) -> Result<Node<Vec<String>>, ParseError> {
    parse(source, env, ParseLimits::default(), |p| p.string_literal()).map(|(value, _)| value)
}

#[cfg(test)]
pub(crate) fn expression(source: &str, env: &mut Env) -> Result<Box<Node<Expression>>, ParseError> {
    parse(source, env, ParseLimits::default(), |p| p.expression()).map(|(value, _)| Box::new(value))
}

#[cfg(test)]
pub(crate) fn declaration(source: &str, env: &mut Env) -> Result<Node<Declaration>, ParseError> {
    parse(source, env, ParseLimits::default(), |p| p.declaration()).map(|(value, _)| value)
}

#[cfg(test)]
pub(crate) fn statement(source: &str, env: &mut Env) -> Result<Box<Node<Statement>>, ParseError> {
    parse(source, env, ParseLimits::default(), |p| p.statement()).map(|(value, _)| Box::new(value))
}

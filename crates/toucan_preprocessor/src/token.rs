use std::collections::BTreeSet;

use crate::LineComments;
use crate::comments::CommentState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Identifier,
    Number,
    String,
    Character,
    Punctuation,
    Placemark,
    Paste,
    Pragma,
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub kind: Kind,
    pub text: String,
    pub space: bool,
    /// First half of a lexically adjacent `::` pair in strict C tokenization.
    pub colon_scope: bool,
    pub hidden: BTreeSet<String>,
    pub depth: usize,
    pub line: usize,
    pub offset: usize,
    pub column: usize,
    pub expanded: bool,
    original: Option<&'static str>,
    pub(crate) doc_origin: Option<crate::documentation::OriginId>,
}

impl Token {
    pub(crate) fn new(kind: Kind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            space: false,
            colon_scope: false,
            hidden: BTreeSet::new(),
            depth: 0,
            line: 1,
            offset: 0,
            column: 1,
            expanded: false,
            original: None,
            doc_origin: None,
        }
    }

    pub(crate) fn spelling(&self) -> &str {
        self.original.unwrap_or(&self.text)
    }
}

pub(crate) struct Normalized {
    pub source: String,
    /// Breakpoints at which translation phases changed the original byte offset.
    source_offsets: Vec<(usize, usize)>,
    line_starts: Vec<usize>,
}

impl Normalized {
    pub(crate) fn original_offset(&self, offset: usize) -> usize {
        let (generated, original) = self.source_offsets[self
            .source_offsets
            .partition_point(|(start, _)| *start <= offset)
            - 1];
        original + offset - generated
    }

    pub(crate) fn line_at(&self, offset: usize) -> usize {
        let original = self.original_offset(offset);
        self.line_starts.partition_point(|start| *start <= original)
    }

    pub(crate) fn column_at(&self, offset: usize) -> usize {
        let original = self.original_offset(offset);
        let line = self.line_starts.partition_point(|start| *start <= original) - 1;
        original - self.line_starts[line] + 1
    }
}

/// Apply trigraph replacement before escaped-newline removal, then replace
/// comments. Compact offset breakpoints retain original physical coordinates.
pub(crate) fn normalize(
    source: &str,
    trigraphs: bool,
    comments: &mut CommentState,
) -> Result<Normalized, String> {
    normalize_with_comments(source, trigraphs, comments, |_, _, _, _| Ok(()))
}

/// Observe physical comment ranges while sharing the ordinary translation phases.
pub(crate) fn normalize_with_comments(
    source: &str,
    trigraphs: bool,
    comments: &mut CommentState,
    mut observe: impl FnMut(std::ops::Range<usize>, usize, usize, usize) -> Result<(), String>,
) -> Result<Normalized, String> {
    let bytes = source.as_bytes();
    let mut spliced = String::with_capacity(source.len());
    let mut source_offsets = vec![(0, 0)];
    let mut line_starts = vec![0];
    line_starts.extend(
        bytes
            .iter()
            .enumerate()
            .filter_map(|(index, byte)| (*byte == b'\n').then_some(index + 1)),
    );
    let mut index = 0;
    while index < bytes.len() {
        let trigraph = if trigraphs && bytes[index..].starts_with(b"??") {
            bytes.get(index + 2).and_then(|third| match third {
                b'=' => Some('#'),
                b'/' => Some('\\'),
                b'\'' => Some('^'),
                b'(' => Some('['),
                b')' => Some(']'),
                b'!' => Some('|'),
                b'<' => Some('{'),
                b'>' => Some('}'),
                b'-' => Some('~'),
                _ => None,
            })
        } else {
            None
        };
        let character =
            trigraph.unwrap_or_else(|| source[index..].chars().next().expect("remaining source"));
        let width = if trigraph.is_some() {
            3
        } else {
            character.len_utf8()
        };
        let following = index + width;
        let newline = if bytes[following..].starts_with(b"\r\n") {
            2
        } else {
            usize::from(bytes.get(following) == Some(&b'\n'))
        };
        if character == '\\' && newline != 0 {
            index = following + newline;
            source_offsets.push((spliced.len(), index));
        } else {
            spliced.push(character);
            index = following;
            if trigraph.is_some() {
                source_offsets.push((spliced.len(), index));
            }
        }
    }
    let original = |offset| {
        let (generated, physical) =
            source_offsets[source_offsets.partition_point(|(start, _)| *start <= offset) - 1];
        physical + offset - generated
    };
    let text = replace_comments_observed(&spliced, comments, |range| {
        let range = original(range.start)..original(range.end);
        let line = line_starts.partition_point(|start| *start <= range.start);
        let end_line = line_starts.partition_point(|start| *start < range.end);
        let column = range.start - line_starts[line - 1] + 1;
        observe(range, line, end_line, column)
    })?;
    Ok(Normalized {
        source: text,
        source_offsets,
        line_starts,
    })
}

/// Replace comments without repeating translation phases one and two. `_Pragma`
/// payloads enter preprocessing after those phases have already completed.
pub(crate) fn replace_comments(source: &str, mode: LineComments) -> Result<String, String> {
    replace_comments_with(source, &mut CommentState::new(mode))
}

/// Adjacent slash punctuators remaining after phase three, excluding literals and
/// a slash followed by a block comment. Macro expansion does not use this check.
pub(crate) fn adjacent_slashes(tokens: &[Token]) -> bool {
    tokens.windows(2).any(|pair| {
        pair[0].text == "/" && pair[1].text == "/" && pair[1].offset == pair[0].offset + 1
    })
}

fn replace_comments_with(source: &str, comments: &mut CommentState) -> Result<String, String> {
    replace_comments_observed(source, comments, |_| Ok(()))
}

fn replace_comments_observed(
    source: &str,
    comments: &mut CommentState,
    mut observe: impl FnMut(std::ops::Range<usize>) -> Result<(), String>,
) -> Result<String, String> {
    let mut chars = source.chars().peekable();
    let mut output = String::with_capacity(source.len());
    while let Some(c) = chars.next() {
        let start = output.len();
        match (c, chars.peek().copied()) {
            ('/', Some('/')) if comments.line_comment(chars.clone().nth(1)) => {
                chars.next();
                output.push_str("  ");
                let mut newline = false;
                for c in chars.by_ref() {
                    if c == '\n' {
                        output.push(c);
                        newline = true;
                        break;
                    }
                    for _ in 0..c.len_utf8() {
                        output.push(' ');
                    }
                }
                observe(start..output.len() - usize::from(newline))?;
            }
            ('/', Some('*')) => {
                chars.next();
                output.push_str("  ");
                let mut closed = false;
                while let Some(c) = chars.next() {
                    if c == '\n' {
                        // A block comment is one whitespace separator, including any
                        // physical newlines it spans. Keep byte offsets for provenance.
                        output.push(' ');
                    } else if c == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        output.push_str("  ");
                        closed = true;
                        break;
                    } else {
                        for _ in 0..c.len_utf8() {
                            output.push(' ');
                        }
                    }
                }
                if !closed {
                    return Err("unterminated block comment".into());
                }
                observe(start..output.len())?;
            }
            ('"' | '\'', _) => {
                let quote = c;
                output.push(c);
                while let Some(c) = chars.next() {
                    output.push(c);
                    if c == '\\' {
                        if let Some(c) = chars.next() {
                            output.push(c);
                        }
                    } else if c == quote {
                        break;
                    }
                }
            }
            _ => output.push(c),
        }
    }
    Ok(output)
}

#[cfg(test)]
pub(crate) fn lex(source: &str) -> Result<Vec<Token>, String> {
    lex_with_scope(source, false)
}

pub(crate) fn lex_with_scope(source: &str, scope_punctuator: bool) -> Result<Vec<Token>, String> {
    lex_limited(source, 1_000_000, scope_punctuator)
}

pub(crate) fn lex_limited(
    source: &str,
    max_tokens: usize,
    scope_punctuator: bool,
) -> Result<Vec<Token>, String> {
    let bytes = source.as_bytes();
    let mut output = Vec::new();
    let mut index = 0;
    let mut space = false;
    while index < bytes.len() {
        let start = index;
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            space = true;
            continue;
        }
        if output.len() >= max_tokens {
            return Err("lexed token limit exceeded".into());
        }
        let literal_prefix = if source[index..].starts_with("u8\"") {
            2
        } else if matches!(bytes[index], b'L' | b'u' | b'U')
            && bytes
                .get(index + 1)
                .is_some_and(|c| matches!(c, b'\"' | b'\''))
        {
            1
        } else {
            0
        };
        let kind = if matches!(bytes[index + literal_prefix], b'\"' | b'\'') {
            index += literal_prefix;
            let quote = bytes[index];
            index += 1;
            let mut closed = false;
            while index < bytes.len() {
                let c = bytes[index];
                index += 1;
                if c == b'\\' {
                    if index < bytes.len() && bytes[index] != b'\n' {
                        index += 1;
                    } else {
                        break;
                    }
                } else if c == quote {
                    closed = true;
                    break;
                } else if c == b'\n' {
                    break;
                }
            }
            if !closed {
                return Err("unterminated string or character literal".into());
            }
            if quote == b'\"' {
                Kind::String
            } else {
                Kind::Character
            }
        } else if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            Kind::Identifier
        } else if bytes[index].is_ascii_digit()
            || (bytes[index] == b'.' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
        {
            index += 1;
            while index < bytes.len() {
                let c = bytes[index];
                if c.is_ascii_alphanumeric()
                    || matches!(c, b'_' | b'.')
                    || (matches!(c, b'+' | b'-')
                        && matches!(bytes[index - 1], b'e' | b'E' | b'p' | b'P'))
                {
                    index += 1;
                } else {
                    break;
                }
            }
            Kind::Number
        } else if bytes[index].is_ascii() && bytes[index] != 0 {
            const PUNCTUATORS: &[&str] = &[
                "%:%:", ">>=", "<<=", "...", "##", "->", "++", "--", "<<", ">>", "<=", ">=", "==",
                "!=", "&&", "||", "*=", "/=", "%=", "+=", "-=", "&=", "^=", "|=", "<:", ":>", "<%",
                "%>", "%:", "::",
            ];
            if let Some(punctuator) = PUNCTUATORS
                .iter()
                .find(|p| (**p != "::" || scope_punctuator) && source[index..].starts_with(**p))
            {
                index += punctuator.len();
            } else {
                index += 1;
            }
            Kind::Punctuation
        } else {
            return Err("non-ASCII identifiers and NUL bytes are not supported".into());
        };
        let (spelling, original) = match &source[start..index] {
            "%:" => ("#", Some("%:")),
            "%:%:" => ("##", Some("%:%:")),
            "<:" => ("[", Some("<:")),
            ":>" => ("]", Some(":>")),
            "<%" => ("{", Some("<%")),
            "%>" => ("}", Some("%>")),
            spelling => (spelling, None),
        };
        let mut token = Token::new(kind, spelling);
        token.original = original;
        token.space = space;
        token.colon_scope = spelling == ":" && bytes.get(index) == Some(&b':');
        token.offset = start;
        output.push(token);
        space = false;
    }
    Ok(output)
}

pub(crate) fn render(tokens: &[Token]) -> String {
    tokens
        .iter()
        .filter(|token| token.kind != Kind::Placemark)
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

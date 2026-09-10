use std::collections::BTreeSet;

use char_str::CharStr;
use memchr::{memchr, memchr2, memchr2_iter, memchr3, memmem};

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
    /// Short spellings stay inline; cloning long spellings shares their allocation.
    pub text: CharStr,
    pub space: bool,
    /// First half of a lexically adjacent `::` pair in strict C tokenization.
    pub colon_scope: bool,
    pub hidden: BTreeSet<CharStr>,
    pub depth: usize,
    pub line: usize,
    pub offset: usize,
    pub column: usize,
    pub expanded: bool,
    original: Option<&'static str>,
    pub(crate) doc_origin: Option<crate::documentation::OriginId>,
}

impl Token {
    pub(crate) fn new(kind: Kind, text: impl Into<CharStr>) -> Self {
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

/// Normalize physical newlines and trigraphs before escaped-newline removal,
/// then replace comments. Offset breakpoints retain original physical coordinates.
pub(crate) fn normalize(
    source: &str,
    trigraphs: bool,
    comments: &mut CommentState,
) -> Result<Normalized, String> {
    normalize_with_comments(source, trigraphs, comments, true, |_, _, _, _| Ok(()))
}

/// Observe physical comment ranges while sharing the ordinary translation phases.
pub(crate) fn normalize_with_comments(
    source: &str,
    trigraphs: bool,
    comments: &mut CommentState,
    strip_bom: bool,
    mut observe: impl FnMut(std::ops::Range<usize>, usize, usize, usize) -> Result<(), String>,
) -> Result<Normalized, String> {
    let bytes = source.as_bytes();
    let mut spliced = String::with_capacity(source.len());
    // A BOM marks a source's encoding, but is an ordinary character in -D text.
    let start = if strip_bom && source.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let mut source_offsets = vec![(0, start)];
    let mut line_starts = vec![0];
    line_starts.extend(memchr2_iter(b'\r', b'\n', bytes).filter_map(|index| {
        (bytes[index] == b'\n' || bytes.get(index + 1) != Some(&b'\n')).then_some(index + 1)
    }));
    let mut index = start;
    let mut copied = start;
    while index < bytes.len() {
        // UTF-8 continuation bytes cannot contain the ASCII phase-one/two markers.
        let next = if trigraphs {
            memchr3(b'\\', b'?', b'\r', &bytes[index..])
        } else {
            memchr2(b'\\', b'\r', &bytes[index..])
        };
        index += next.unwrap_or(bytes.len() - index);
        if index == bytes.len() {
            break;
        }
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
        let mut character = trigraph.unwrap_or(bytes[index] as char);
        let mut width = if trigraph.is_some() { 3 } else { 1 };
        if character == '\r' {
            character = '\n';
            width = 1 + usize::from(bytes.get(index + 1) == Some(&b'\n'));
        }
        let following = index + width;
        let newline = if bytes[following..].starts_with(b"\r\n") {
            2
        } else {
            usize::from(
                bytes
                    .get(following)
                    .is_some_and(|byte| matches!(byte, b'\r' | b'\n')),
            )
        };
        if character == '\\' && newline != 0 {
            spliced.push_str(&source[copied..index]);
            index = following + newline;
            copied = index;
            source_offsets.push((spliced.len(), index));
        } else {
            if trigraph.is_some() || bytes[index] == b'\r' {
                spliced.push_str(&source[copied..index]);
                spliced.push(character);
                copied = following;
                if width != 1 {
                    source_offsets.push((spliced.len(), following));
                }
            }
            index = following;
        }
    }
    spliced.push_str(&source[copied..]);
    let original = |offset| {
        let (generated, physical) =
            source_offsets[source_offsets.partition_point(|(start, _)| *start <= offset) - 1];
        physical + offset - generated
    };
    let text = replace_comments_observed(spliced, comments, |range| {
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
    replace_comments_observed(source.to_owned(), comments, |_| Ok(()))
}

fn replace_comments_observed(
    source: String,
    comments: &mut CommentState,
    mut observe: impl FnMut(std::ops::Range<usize>) -> Result<(), String>,
) -> Result<String, String> {
    let mut bytes = source.into_bytes();
    let mut index = 0;
    while index < bytes.len() {
        index += memchr3(b'/', b'"', b'\'', &bytes[index..]).unwrap_or(bytes.len() - index);
        let Some(&byte) = bytes.get(index) else {
            break;
        };
        let start = index;
        match (byte, bytes.get(index + 1).copied()) {
            (b'/', Some(b'/'))
                if comments.line_comment(bytes.get(index + 2).map(|byte| *byte as char)) =>
            {
                index += 2;
                index += memchr(b'\n', &bytes[index..]).unwrap_or(bytes.len() - index);
                bytes[start..index].fill(b' ');
                observe(start..index)?;
            }
            (b'/', Some(b'*')) => {
                index += 2;
                let Some(end) = memmem::find(&bytes[index..], b"*/") else {
                    return Err("unterminated block comment".into());
                };
                index += end + 2;
                // Block comments become one whitespace separator, including their
                // physical newlines. Blank every UTF-8 byte to preserve provenance.
                bytes[start..index].fill(b' ');
                observe(start..index)?;
            }
            (b'"' | b'\'', _) => {
                let quote = byte;
                index += 1;
                loop {
                    let Some(next) = memchr2(quote, b'\\', &bytes[index..]) else {
                        index = bytes.len();
                        break;
                    };
                    index += next;
                    let byte = bytes[index];
                    index += 1;
                    if byte == b'\\' {
                        // Skipping one byte is enough: any UTF-8 continuation
                        // bytes following it cannot be a quote or backslash.
                        index += usize::from(index < bytes.len());
                    } else {
                        break;
                    }
                }
            }
            _ => index += 1,
        }
    }
    // Only complete comment ranges delimited by ASCII bytes were replaced.
    Ok(String::from_utf8(bytes).expect("comment replacement preserves UTF-8"))
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
        } else if bytes[index].is_ascii_alphabetic() || matches!(bytes[index], b'_' | b'$') {
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'$'))
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
                    || matches!(c, b'_' | b'$' | b'.')
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

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::Token;

    #[test]
    fn token_size() {
        assert_eq!(size_of::<Token>(), 96);
    }
}

#[cfg(test)]
#[path = "scanner_tests.rs"]
mod scanner_tests;

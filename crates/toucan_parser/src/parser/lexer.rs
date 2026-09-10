//! Token boundaries in preprocessed C, retaining offsets into the original input.

use limits::{Budget, ResourceKind};
use span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TokenKind {
    Identifier,
    Number,
    String,
    Character,
    Punct,
    Digraph,
    End,
    Invalid,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Token {
    pub(super) kind: TokenKind,
    pub(super) span: Span,
}

pub(super) fn lex(source: &str, budget: &mut Budget, line_markers: bool) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut pos = 0;
    let mut line_start = true;
    if !budget.check(
        ResourceKind::InputBytes,
        0,
        source.len() as u64,
        budget.limits.max_input_bytes as u64,
    ) {
        return tokens;
    }
    // Charge the single scan before examining any bytes, including whitespace
    // and long literals. A small work limit must stop before scanning a large
    // input, even if that input contains no complete tokens.
    if !budget.work(0, source.len() as u64) {
        return tokens;
    }
    while pos < bytes.len() {
        let start = pos;
        if bytes[pos].is_ascii_whitespace() {
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                line_start |= bytes[pos] == b'\n';
                pos += 1;
            }
            continue;
        }
        if line_markers && line_start && bytes[pos] == b'#' {
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }
        line_start = false;
        let quote = match bytes[pos] {
            b'\'' | b'"' => Some(pos),
            b'L' | b'u' | b'U' if matches!(bytes.get(pos + 1), Some(b'\'' | b'"')) => Some(pos + 1),
            b'u' if bytes.get(pos + 1) == Some(&b'8') && bytes.get(pos + 2) == Some(&b'"') => {
                Some(pos + 2)
            }
            _ => None,
        };
        let kind = if let Some(quote_pos) = quote {
            let quote = bytes[quote_pos];
            pos = quote_pos + 1;
            let mut closed = false;
            while pos < bytes.len() {
                let byte = bytes[pos];
                if byte == quote {
                    pos += 1;
                    closed = true;
                    break;
                }
                if byte == b'\n' || byte == b'\r' {
                    break;
                }
                if byte == b'\\' {
                    pos += 1;
                    if pos == bytes.len() {
                        break;
                    }
                    if bytes[pos] == b'\n' || bytes[pos] == b'\r' {
                        break;
                    }
                }
                pos += 1;
            }
            if !closed {
                TokenKind::Invalid
            } else if quote == b'"' {
                TokenKind::String
            } else {
                TokenKind::Character
            }
        } else if bytes[pos].is_ascii_alphabetic() || matches!(bytes[pos], b'_' | b'$') {
            pos += 1;
            while pos < bytes.len()
                && (bytes[pos].is_ascii_alphanumeric() || matches!(bytes[pos], b'_' | b'$'))
            {
                pos += 1;
            }
            TokenKind::Identifier
        } else if bytes[pos].is_ascii_digit()
            || (bytes[pos] == b'.' && bytes.get(pos + 1).is_some_and(u8::is_ascii_digit))
        {
            pos += 1;
            while pos < bytes.len() {
                let byte = bytes[pos];
                if byte.is_ascii_alphanumeric()
                    || matches!(byte, b'_' | b'$' | b'.')
                    || (matches!(byte, b'+' | b'-')
                        && matches!(bytes[pos - 1], b'e' | b'E' | b'p' | b'P'))
                {
                    pos += 1;
                } else {
                    break;
                }
            }
            TokenKind::Number
        } else if matches!(
            &bytes[pos..],
            [b'<', b':' | b'%', ..] | [b':' | b'%', b'>', ..]
        ) {
            pos += 2;
            TokenKind::Digraph
        } else if bytes[pos].is_ascii() {
            let width = match &bytes[pos..] {
                [b'.', b'.', b'.', ..] | [b'<', b'<', b'=', ..] | [b'>', b'>', b'=', ..] => 3,
                [b'-', b'>', ..]
                | [b'+', b'+', ..]
                | [b'-', b'-', ..]
                | [b'<', b'<', ..]
                | [b'>', b'>', ..]
                | [b'&', b'&', ..]
                | [b'|', b'|', ..]
                | [b'<' | b'>' | b'=' | b'!' | b'*' | b'/' | b'%' | b'+' | b'-' | b'&' | b'^'
                | b'|', b'=', ..] => 2,
                _ => 1,
            };
            pos += width;
            TokenKind::Punct
        } else {
            pos += source[pos..].chars().next().unwrap().len_utf8();
            TokenKind::Invalid
        };
        // Bound retained capacity, rather than just initialized tokens. Reserve
        // explicitly so Vec's growth cannot exceed the caller's memory limit.
        if !budget.work(start, ::std::mem::size_of::<Token>() as u64) {
            break;
        }
        if tokens.len() == tokens.capacity() {
            let capacity = tokens
                .capacity()
                .saturating_mul(2)
                .max(128)
                .min(source.len());
            let bytes = (capacity as u64).saturating_mul(::std::mem::size_of::<Token>() as u64);
            if !budget.check(
                ResourceKind::CacheBytes,
                start,
                bytes,
                budget.limits.max_cache_bytes,
            ) {
                break;
            }
            tokens.reserve_exact(capacity - tokens.len());
            budget.statistics.cache_bytes =
                (tokens.capacity() * ::std::mem::size_of::<Token>()) as u64;
        }
        tokens.push(Token {
            kind,
            span: Span::span(start, pos),
        });
    }
    tokens
}

use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Identifier,
    Number,
    String,
    Character,
    Punctuation,
    Placemark,
    Paste,
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub kind: Kind,
    pub text: String,
    pub space: bool,
    pub hidden: BTreeSet<String>,
    pub depth: usize,
    pub line: usize,
    pub offset: usize,
    pub column: usize,
    pub expanded: bool,
    original: Option<&'static str>,
}

impl Token {
    pub(crate) fn new(kind: Kind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            space: false,
            hidden: BTreeSet::new(),
            depth: 0,
            line: 1,
            offset: 0,
            column: 1,
            expanded: false,
            original: None,
        }
    }

    pub(crate) fn spelling(&self) -> &str {
        self.original.unwrap_or(&self.text)
    }
}

pub(crate) struct Normalized {
    pub source: String,
    line_changes: Vec<(usize, usize)>,
}

impl Normalized {
    pub(crate) fn line_at(&self, offset: usize) -> usize {
        self.line_changes[self
            .line_changes
            .partition_point(|(start, _)| *start <= offset)
            - 1]
        .1
    }

    pub(crate) fn column_at(&self, offset: usize) -> usize {
        let line_start = self.line_changes[self
            .line_changes
            .partition_point(|(start, _)| *start <= offset)
            - 1]
        .0;
        offset - line_start + 1
    }
}

/// Remove escaped newlines and comments, retaining physical source line locations.
pub(crate) fn normalize(source: &str) -> Result<Normalized, String> {
    if source.contains("??") {
        for suffix in ['=', '/', '\'', '(', ')', '!', '<', '>', '-'] {
            if source.contains(&format!("??{suffix}")) {
                return Err("trigraphs are not supported".into());
            }
        }
    }
    let mut spliced = String::with_capacity(source.len());
    let mut line_changes = vec![(0, 1)];
    let mut line = 1;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let mut lookahead = chars.clone();
            if lookahead.peek() == Some(&'\r') {
                lookahead.next();
            }
            if lookahead.next() == Some('\n') {
                chars = lookahead;
                line += 1;
                line_changes.push((spliced.len(), line));
                continue;
            }
        }
        spliced.push(c);
        if c == '\n' {
            line += 1;
            line_changes.push((spliced.len(), line));
        }
    }
    let source = spliced;
    let mut chars = source.chars().peekable();
    let mut output = String::with_capacity(source.len());
    while let Some(c) = chars.next() {
        match (c, chars.peek().copied()) {
            ('/', Some('/')) => {
                chars.next();
                output.push_str("  ");
                for c in chars.by_ref() {
                    if c == '\n' {
                        output.push(c);
                        break;
                    }
                    for _ in 0..c.len_utf8() {
                        output.push(' ');
                    }
                }
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
    Ok(Normalized {
        source: output,
        line_changes,
    })
}

pub(crate) fn lex(source: &str) -> Result<Vec<Token>, String> {
    lex_limited(source, 1_000_000)
}

pub(crate) fn lex_limited(source: &str, max_tokens: usize) -> Result<Vec<Token>, String> {
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
                "%>", "%:",
            ];
            if let Some(punctuator) = PUNCTUATORS
                .iter()
                .find(|p| source[index..].starts_with(**p))
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

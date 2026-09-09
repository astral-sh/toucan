//! Differential oracle retained from the character scanner before byte scanning.

use super::{CommentState, LineComments, Normalized, normalize_with_comments};

fn original_normalize(
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
    let text = original_replace_comments(&spliced, comments, |range| {
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

fn original_replace_comments(
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

#[test]
fn scanner_preserves_text_offsets_and_comment_ranges() {
    let atoms = [
        "name", " ", "\n", "\r\n", "\\", "??", "??/", "??=", "??<", "??>", "//", "/*", "*/",
        "//**/", "\"", "'", "\\\"", "\\'", "é", "🦜", "\0",
    ];
    let modes = [
        LineComments::Enabled,
        LineComments::GnuC90,
        LineComments::GnuC90Preprocessing,
        LineComments::ClangC90,
        LineComments::ClangC90Preprocessing,
    ];
    let mut state = 0xa563_c198_2635_346du64;
    for case in 0..4096 {
        let mut source = String::new();
        for _ in 0..case % 12 + 1 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            source.push_str(atoms[state as usize % atoms.len()]);
        }
        for mode in modes {
            for trigraphs in [false, true] {
                let mut actual_comments = Vec::new();
                let actual = normalize_with_comments(
                    &source,
                    trigraphs,
                    &mut CommentState::new(mode),
                    |range, line, end_line, column| {
                        actual_comments.push((range, line, end_line, column));
                        Ok(())
                    },
                );
                let mut expected_comments = Vec::new();
                let expected = original_normalize(
                    &source,
                    trigraphs,
                    &mut CommentState::new(mode),
                    |range, line, end_line, column| {
                        expected_comments.push((range, line, end_line, column));
                        Ok(())
                    },
                );
                assert_eq!(
                    actual_comments, expected_comments,
                    "{source:?} {mode:?} {trigraphs}"
                );
                match (actual, expected) {
                    (Ok(actual), Ok(expected)) => {
                        assert_eq!(
                            actual.source, expected.source,
                            "{source:?} {mode:?} {trigraphs}"
                        );
                        assert_eq!(actual.source_offsets, expected.source_offsets);
                        assert_eq!(actual.line_starts, expected.line_starts);
                        for offset in 0..=actual.source.len() {
                            assert_eq!(
                                actual.original_offset(offset),
                                expected.original_offset(offset)
                            );
                            assert_eq!(actual.line_at(offset), expected.line_at(offset));
                            assert_eq!(actual.column_at(offset), expected.column_at(offset));
                        }
                    }
                    (Err(actual), Err(expected)) => assert_eq!(actual, expected),
                    _ => panic!("scanner acceptance changed for {source:?} {mode:?} {trigraphs}"),
                }
            }
        }
    }
}

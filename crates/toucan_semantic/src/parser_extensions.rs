//! Narrow GNU grammar adapters for lang-c, with original-source diagnostics.

use crate::Error;

const MAX_ADAPTATIONS: usize = 100_000;

#[derive(Clone, Copy)]
struct Segment {
    parsed: usize,
    original: usize,
    copied: bool,
}

/// Maps copied/reordered source runs and inserted tokens back to the original text.
#[derive(Default)]
pub(crate) struct SourceMap {
    segments: Vec<Segment>,
    insertions: Vec<usize>,
}

impl SourceMap {
    pub(crate) fn original_offset(&self, offset: usize) -> usize {
        let index = self
            .segments
            .partition_point(|segment| segment.parsed <= offset);
        let Some(segment) = index.checked_sub(1).map(|index| self.segments[index]) else {
            return offset;
        };
        segment.original
            + if segment.copied {
                offset - segment.parsed
            } else {
                0
            }
    }

    /// Pragma locations are outside reordered attribute tokens, so only insertions
    /// affect their parser offsets. Each insertion adds one empty pair of braces.
    pub(crate) fn pragma_offset(&self, offset: usize) -> usize {
        offset
            + 2 * self
                .insertions
                .partition_point(|insertion| *insertion <= offset)
    }
}

pub(crate) struct Adapted {
    pub(crate) source: String,
    pub(crate) offsets: SourceMap,
    pub(crate) empty_initializers: std::collections::HashSet<usize>,
}

struct Builder<'a> {
    source: &'a str,
    output: String,
    offsets: SourceMap,
}

impl Builder<'_> {
    fn copy(&mut self, start: usize, end: usize) {
        if start == end {
            return;
        }
        self.offsets.segments.push(Segment {
            parsed: self.output.len(),
            original: start,
            copied: true,
        });
        self.output.push_str(&self.source[start..end]);
    }

    fn empty_initializer(&mut self, original: usize) -> usize {
        let parsed = self.output.len();
        self.offsets.segments.push(Segment {
            parsed,
            original,
            copied: false,
        });
        self.offsets.insertions.push(original);
        self.output.push_str("{}");
        parsed
    }
}

/// Inserts a marked empty initializer into otherwise unparseable compound
/// literals. In an empty control/function body the same spelling adds an empty
/// nested block, which has no declarations or effects. Attribute lists before a
/// nested pointer declarator are moved after its star; the semantic checker admits
/// only annotations whose effect is unchanged by that parser placement.
pub(crate) fn adapt(source: &str) -> Result<Adapted, Error> {
    let bytes = source.as_bytes();
    let mut builder = Builder {
        source,
        output: String::with_capacity(source.len()),
        offsets: SourceMap::default(),
    };
    let mut empty_initializers = std::collections::HashSet::new();
    let mut index = 0;
    let mut copied = 0;
    let mut previous = None;
    let mut adaptations = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if matches!(bytes[index], b'\'' | b'"') {
            index = quoted_end(bytes, index);
            previous = None;
            continue;
        }
        if bytes[index] == b'{' && previous == Some(b')') {
            let closing = skip_space(bytes, index + 1);
            if bytes.get(closing) == Some(&b'}') {
                check_adaptations(&mut adaptations, index)?;
                builder.copy(copied, closing);
                empty_initializers.insert(builder.empty_initializer(closing));
                copied = closing;
                index = closing + 1;
                previous = Some(b'}');
                continue;
            }
        }
        if previous == Some(b'(') && attribute_end(bytes, index).is_some() {
            let start = index;
            let mut end = index;
            while let Some(next) = attribute_end(bytes, end) {
                end = skip_space(bytes, next);
            }
            if bytes.get(end) == Some(&b'*') {
                check_adaptations(&mut adaptations, index)?;
                builder.copy(copied, start);
                builder.copy(end, end + 1);
                builder.copy(start, end);
                copied = end + 1;
                index = end + 1;
                previous = Some(b'*');
                continue;
            }
        }
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            while bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                index += 1;
            }
            if matches!(&source[start..index], "asm" | "__asm" | "__asm__") {
                crate::asm::check_qualifiers(source, index)?;
            }
            previous = Some(bytes[index - 1]);
            continue;
        }
        previous = Some(bytes[index]);
        index += 1;
    }
    builder.copy(copied, source.len());
    // EOF may follow a reordered segment; anchor it explicitly.
    builder.offsets.segments.push(Segment {
        parsed: builder.output.len(),
        original: source.len(),
        copied: true,
    });
    if builder.output.len() > 16 * 1024 * 1024 {
        return Err(Error::new(
            0,
            "adapted parser input exceeds the 16 MiB limit",
        ));
    }
    Ok(Adapted {
        source: builder.output,
        offsets: builder.offsets,
        empty_initializers,
    })
}

fn skip_space(bytes: &[u8], mut index: usize) -> usize {
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    index
}

fn quoted_end(bytes: &[u8], mut index: usize) -> usize {
    let quote = bytes[index];
    index += 1;
    while index < bytes.len() {
        if bytes[index] == quote {
            return index + 1;
        }
        if bytes[index] == b'\\' {
            index += 1;
        }
        index += 1;
    }
    bytes.len()
}

/// Returns a complete GNU attribute specifier, including nested argument lists.
fn attribute_end(bytes: &[u8], start: usize) -> Option<usize> {
    let remaining = bytes.get(start..)?;
    let length = if remaining.starts_with(b"__attribute__") {
        13
    } else if remaining.starts_with(b"__attribute") {
        11
    } else {
        return None;
    };
    let opening = skip_space(bytes, start + length);
    if bytes.get(opening) != Some(&b'(') {
        return None;
    }
    let second = skip_space(bytes, opening + 1);
    if bytes.get(second) != Some(&b'(') {
        return None;
    }
    let mut index = second + 1;
    let mut depth = 2;
    while index < bytes.len() {
        match bytes[index] {
            b'\'' | b'"' => {
                index = quoted_end(bytes, index);
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// Limits source-map storage independently of the parser's eventual AST size.
fn check_adaptations(count: &mut usize, offset: usize) -> Result<(), Error> {
    *count += 1;
    if *count > MAX_ADAPTATIONS {
        return Err(Error::new(
            offset,
            "GNU parser adapters exceed the 100000-edit limit",
        ));
    }
    Ok(())
}

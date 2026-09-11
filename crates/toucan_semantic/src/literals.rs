use toucan_target::{Compiler, CompilerProfile, Target};

use crate::{Error, IntegerKind, IntegerValue};

const MAX_LITERAL_BYTES: usize = 16 * 1024 * 1024;

/// The encoding selected by C11 string-literal prefixes and concatenation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub enum StringEncoding {
    Ordinary,
    Utf8,
    Utf16,
    Utf32,
    Wide,
}

/// A C string object's code units, including its implicit terminating NUL.
/// Numeric escapes retain their code-unit values even when they do not form valid
/// Unicode. The target determines the width and signedness of wide characters.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct DecodedString {
    pub encoding: StringEncoding,
    pub element_type: IntegerKind,
    pub code_units: Vec<u32>,
}

impl DecodedString {
    /// Copies ordinary/UTF-8 code units as bytes, including the final NUL.
    /// Returns None for wide encodings or code units outside the byte range.
    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        if !matches!(
            self.encoding,
            StringEncoding::Ordinary | StringEncoding::Utf8
        ) {
            return None;
        }
        self.code_units
            .iter()
            .map(|unit| u8::try_from(*unit).ok())
            .collect()
    }
}

/// Decodes adjacent preprocessed C11 string tokens using UTF-8 for the execution
/// character set. `offset` anchors diagnostics in the caller's source.
pub fn decode_string_literals(
    strings: &[String],
    target: Target,
    offset: usize,
) -> Result<DecodedString, Error> {
    decode_string_literals_with_profile(strings, CompilerProfile::default_for(target), offset)
}

/// Decodes adjacent string tokens with the selected compiler's character types.
pub fn decode_string_literals_with_profile(
    strings: &[String],
    profile: CompilerProfile,
    offset: usize,
) -> Result<DecodedString, Error> {
    let target = profile.target();
    if strings.is_empty() {
        return Err(Error::new(offset, "expected a string literal"));
    }
    let mut bytes = 0usize;
    let mut encoding = StringEncoding::Ordinary;
    for literal in strings {
        bytes = bytes.saturating_add(literal.len());
        if bytes > MAX_LITERAL_BYTES {
            return Err(Error::new(
                offset,
                "string literals exceed the 16 MiB limit",
            ));
        }
        let (prefix, _) = literal_body(literal, '"', offset)?;
        if prefix != StringEncoding::Ordinary {
            if encoding != StringEncoding::Ordinary && encoding != prefix {
                return Err(Error::new(
                    offset,
                    "incompatible adjacent string literal prefixes",
                ));
            }
            encoding = prefix;
        }
    }
    let mut code_units = Vec::new();
    for literal in strings {
        let (_, body) = literal_body(literal, '"', offset)?;
        decode_body(
            body,
            '"',
            encoding,
            target,
            profile.compiler() == Compiler::Gnu,
            offset,
            &mut code_units,
        )?;
    }
    code_units.push(0);
    Ok(DecodedString {
        encoding,
        element_type: element_type(encoding, target, profile.compiler()),
        code_units,
    })
}

/// Evaluates a C11 character token under the target's execution character profile.
/// GNU targets retain GCC's implementation-defined multicharacter values; the
/// Clang profiles reject wide characters that require more than one code unit.
pub fn decode_character_literal(
    literal: &str,
    target: Target,
    offset: usize,
) -> Result<IntegerValue, Error> {
    decode_character_literal_with_profile(literal, CompilerProfile::default_for(target), offset)
}

/// Decodes a character token with the selected compiler's multicharacter rules.
pub fn decode_character_literal_with_profile(
    literal: &str,
    profile: CompilerProfile,
    offset: usize,
) -> Result<IntegerValue, Error> {
    let target = profile.target();
    let gnu = profile.compiler() == Compiler::Gnu;
    if literal.len() > MAX_LITERAL_BYTES {
        return Err(Error::new(
            offset,
            "character literal exceeds the 16 MiB limit",
        ));
    }
    let (encoding, body) = literal_body(literal, '\'', offset)?;
    if encoding == StringEncoding::Utf8 {
        return Err(Error::new(
            offset,
            "UTF-8 character literals are not part of C11",
        ));
    }
    let mut units = Vec::new();
    decode_body(body, '\'', encoding, target, gnu, offset, &mut units)?;
    if units.is_empty() {
        return Err(Error::new(offset, "empty character constant"));
    }
    if encoding == StringEncoding::Ordinary {
        if units.len() == 1 {
            let byte = units[0] as u8;
            return Ok(IntegerValue::int(if target.char_is_signed() {
                i128::from(byte as i8)
            } else {
                i128::from(byte)
            }));
        }
        let value = units
            .iter()
            .fold(0u32, |value, unit| value.wrapping_shl(8) | unit);
        return Ok(IntegerValue::int(i128::from(value as i32)));
    }
    if units.len() != 1 && !gnu {
        return Err(Error::new(
            offset,
            "wide character constant requires exactly one code unit",
        ));
    }
    let value = u128::from(*units.last().expect("nonempty character constant"));
    let (bits, signed, rank) = match element_type(encoding, target, profile.compiler()) {
        IntegerKind::UnsignedShort => (16, false, 2),
        IntegerKind::UnsignedInt => (32, false, 3),
        IntegerKind::Int => (32, true, 3),
        IntegerKind::Long => (32, true, 4),
        _ => unreachable!("wide encoding has a fixed integer representation"),
    };
    Ok(IntegerValue::new(value, bits, signed, rank))
}

fn element_type(encoding: StringEncoding, target: Target, compiler: Compiler) -> IntegerKind {
    match encoding {
        StringEncoding::Ordinary | StringEncoding::Utf8 => IntegerKind::Char,
        StringEncoding::Utf16 => IntegerKind::UnsignedShort,
        StringEncoding::Utf32 => IntegerKind::UnsignedInt,
        StringEncoding::Wide if target.wchar_width() == 16 => IntegerKind::UnsignedShort,
        StringEncoding::Wide
            if target == Target::I686UnknownLinuxGnu && compiler == Compiler::Gnu =>
        {
            IntegerKind::Long
        }
        StringEncoding::Wide if target.wchar_is_signed() => IntegerKind::Int,
        StringEncoding::Wide => IntegerKind::UnsignedInt,
    }
}

fn unit_width(encoding: StringEncoding, target: Target) -> u32 {
    match encoding {
        StringEncoding::Ordinary | StringEncoding::Utf8 => 8,
        StringEncoding::Utf16 => 16,
        StringEncoding::Utf32 => 32,
        StringEncoding::Wide => target.wchar_width() as u32,
    }
}

fn literal_body(
    literal: &str,
    quote: char,
    offset: usize,
) -> Result<(StringEncoding, &str), Error> {
    let (encoding, rest) = if let Some(rest) = literal.strip_prefix("u8") {
        (StringEncoding::Utf8, rest)
    } else if let Some(rest) = literal.strip_prefix('u') {
        (StringEncoding::Utf16, rest)
    } else if let Some(rest) = literal.strip_prefix('U') {
        (StringEncoding::Utf32, rest)
    } else if let Some(rest) = literal.strip_prefix('L') {
        (StringEncoding::Wide, rest)
    } else {
        (StringEncoding::Ordinary, literal)
    };
    let body = rest
        .strip_prefix(quote)
        .and_then(|rest| rest.strip_suffix(quote))
        .ok_or_else(|| Error::new(offset, "invalid string or character literal"))?;
    Ok((encoding, body))
}

/// Numeric escapes denote code units; source characters and universal character
/// names denote scalars that must be encoded in the selected literal encoding.
fn decode_body(
    body: &str,
    quote: char,
    encoding: StringEncoding,
    target: Target,
    gnu: bool,
    offset: usize,
    output: &mut Vec<u32>,
) -> Result<(), Error> {
    let mut chars = body.chars().peekable();
    while let Some(character) = chars.next() {
        let character = if character == '\\' {
            let escape = chars
                .next()
                .ok_or_else(|| Error::new(offset, "incomplete literal escape"))?;
            match escape {
                '\\' | '\'' | '"' | '?' => escape,
                'a' => '\x07',
                'b' => '\x08',
                // Both GCC and Clang recognize the GNU escape-character spelling.
                'e' | 'E' => '\x1b',
                'f' => '\x0c',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                'v' => '\x0b',
                '0'..='7' | 'x' => {
                    let radix = if escape == 'x' { 16 } else { 8 };
                    let mut value = escape.to_digit(radix).unwrap_or(0);
                    let mut digits = usize::from(escape != 'x');
                    while let Some(digit) =
                        chars.peek().and_then(|character| character.to_digit(radix))
                    {
                        if radix == 8 && digits == 3 {
                            break;
                        }
                        chars.next();
                        digits += 1;
                        value = value
                            .checked_mul(radix)
                            .and_then(|value| value.checked_add(digit))
                            .ok_or_else(|| {
                                Error::new(offset, "numeric escape exceeds the code-unit range")
                            })?;
                    }
                    if digits == 0 {
                        return Err(Error::new(offset, "hexadecimal escape requires a digit"));
                    }
                    if u64::from(value) >= 1u64 << unit_width(encoding, target) {
                        return Err(Error::new(
                            offset,
                            "numeric escape exceeds the code-unit range",
                        ));
                    }
                    output.push(value);
                    continue;
                }
                'u' | 'U' => {
                    let mut value = 0u32;
                    for _ in 0..if escape == 'u' { 4 } else { 8 } {
                        let digit = chars
                            .next()
                            .and_then(|character| character.to_digit(16))
                            .ok_or_else(|| {
                                Error::new(offset, "invalid universal character name")
                            })?;
                        value = (value << 4) | digit;
                    }
                    char::from_u32(value)
                        .filter(|_| value >= 0xa0 || matches!(value, 0x24 | 0x40 | 0x60))
                        .ok_or_else(|| Error::new(offset, "invalid universal character name"))?
                }
                _ => {
                    return Err(Error::new(
                        offset,
                        format!("unsupported literal escape `\\{escape}`"),
                    ));
                }
            }
        } else {
            if character == quote || matches!(character, '\n' | '\r' | '\0') {
                return Err(Error::new(
                    offset,
                    "unescaped quote, newline, or NUL in literal",
                ));
            }
            character
        };
        match unit_width(encoding, target) {
            8 => {
                if quote == '\'' && character.len_utf8() != 1 && !gnu {
                    return Err(Error::new(
                        offset,
                        "character is not representable in one execution byte",
                    ));
                }
                let mut buffer = [0u8; 4];
                output.extend(character.encode_utf8(&mut buffer).bytes().map(u32::from));
            }
            16 => {
                let mut buffer = [0u16; 2];
                output.extend(
                    character
                        .encode_utf16(&mut buffer)
                        .iter()
                        .copied()
                        .map(u32::from),
                );
            }
            32 => output.push(u32::from(character)),
            _ => unreachable!("supported code-unit width"),
        }
    }
    Ok(())
}

impl crate::analyze::Analyzer {
    pub(crate) fn decode_string_literal(
        &self,
        literal: &lang_c::span::Node<lang_c::ast::StringLiteral>,
        offset: usize,
    ) -> Result<DecodedString, Error> {
        decode_string_literals_with_profile(&literal.node, self.unit.profile()?, offset)
    }
}

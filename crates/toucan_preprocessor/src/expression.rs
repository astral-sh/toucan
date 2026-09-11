use crate::token::{Kind, Token};

#[derive(Clone, Copy, Debug)]
struct Value {
    bits: u64,
    unsigned: bool,
}

impl Value {
    fn signed(value: i64) -> Self {
        Self {
            bits: value as u64,
            unsigned: false,
        }
    }

    fn boolean(value: bool) -> Self {
        Self::signed(i64::from(value))
    }

    fn truth(self) -> bool {
        self.bits != 0
    }
}

/// Evaluate C preprocessing expressions using intmax_t and uintmax_t arithmetic.
pub(crate) fn evaluate(
    tokens: &[Token],
    wchar_unsigned: Option<bool>,
    char_unsigned: bool,
    ms_extensions: bool,
) -> Result<bool, String> {
    let mut parser = Parser {
        tokens,
        position: 0,
        depth: 0,
        wchar_unsigned,
        char_unsigned,
        ms_extensions,
    };
    let result = parser.expression(0, true)?;
    if parser.position != tokens.len() {
        return Err(format!(
            "unexpected token `{}` in #if",
            tokens[parser.position].text
        ));
    }
    Ok(result.truth())
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    depth: usize,
    wchar_unsigned: Option<bool>,
    char_unsigned: bool,
    ms_extensions: bool,
}

impl Parser<'_> {
    fn take(&mut self, spelling: &str) -> bool {
        if self
            .tokens
            .get(self.position)
            .is_some_and(|token| token.text == spelling)
        {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expression(&mut self, minimum: u8, evaluate: bool) -> Result<Value, String> {
        if self.depth >= 128 {
            return Err("#if expression nesting limit exceeded".into());
        }
        self.depth += 1;
        let result = self.expression_inner(minimum, evaluate);
        self.depth -= 1;
        result
    }

    fn expression_inner(&mut self, minimum: u8, evaluate: bool) -> Result<Value, String> {
        let mut left = self.unary(evaluate)?;
        loop {
            if minimum == 0 && self.take("?") {
                let then = self.expression(0, evaluate && left.truth())?;
                if !self.take(":") {
                    return Err("expected `:` in conditional expression".into());
                }
                let otherwise = self.expression(0, evaluate && !left.truth())?;
                left = Value {
                    bits: if left.truth() {
                        then.bits
                    } else {
                        otherwise.bits
                    },
                    unsigned: then.unsigned || otherwise.unsigned,
                };
                continue;
            }
            let Some(token) = self.tokens.get(self.position) else {
                break;
            };
            let precedence = match token.text.as_str() {
                "||" => 1,
                "&&" => 2,
                "|" => 3,
                "^" => 4,
                "&" => 5,
                "==" | "!=" => 6,
                "<" | "<=" | ">" | ">=" => 7,
                "<<" | ">>" => 8,
                "+" | "-" => 9,
                "*" | "/" | "%" => 10,
                _ => break,
            };
            if precedence < minimum {
                break;
            }
            self.position += 1;
            let active = evaluate
                && match token.text.as_str() {
                    "&&" => left.truth(),
                    "||" => !left.truth(),
                    _ => true,
                };
            let right = self.expression(precedence + 1, active)?;
            left = binary(&token.text, left, right, evaluate)?;
        }
        Ok(left)
    }

    fn unary(&mut self, evaluate: bool) -> Result<Value, String> {
        if self.depth >= 128 {
            return Err("#if expression nesting limit exceeded".into());
        }
        self.depth += 1;
        let result = self.unary_inner(evaluate);
        self.depth -= 1;
        result
    }

    fn unary_inner(&mut self, evaluate: bool) -> Result<Value, String> {
        if self.take("+") {
            return self.unary(evaluate);
        }
        if self.take("-") {
            let value = self.unary(evaluate)?;
            return if value.unsigned {
                Ok(Value {
                    bits: value.bits.wrapping_neg(),
                    unsigned: true,
                })
            } else if !evaluate {
                Ok(Value::signed(0))
            } else {
                (value.bits as i64)
                    .checked_neg()
                    .map(Value::signed)
                    .ok_or_else(|| "signed overflow in #if".into())
            };
        }
        if self.take("!") {
            return Ok(Value::boolean(!self.unary(evaluate)?.truth()));
        }
        if self.take("~") {
            let value = self.unary(evaluate)?;
            return Ok(Value {
                bits: !value.bits,
                unsigned: value.unsigned,
            });
        }
        if self.take("(") {
            let value = self.expression(0, evaluate)?;
            if !self.take(")") {
                return Err("expected `)` in #if".into());
            }
            return Ok(value);
        }
        let token = self
            .tokens
            .get(self.position)
            .ok_or("expected expression in #if")?;
        self.position += 1;
        match token.kind {
            Kind::Identifier => Ok(Value::signed(0)),
            Kind::Number => integer(&token.text, self.ms_extensions),
            Kind::Character => {
                let (text, unsigned, ordinary) = if let Some(text) = token.text.strip_prefix('L') {
                    (
                        text,
                        self.wchar_unsigned.ok_or(
                            "wide character constants require the target's __WCHAR_TYPE__ macro",
                        )?,
                        false,
                    )
                } else if let Some(text) = token
                    .text
                    .strip_prefix('u')
                    .or_else(|| token.text.strip_prefix('U'))
                {
                    (text, true, false)
                } else {
                    (token.text.as_str(), self.char_unsigned, true)
                };
                let value = character(text, ordinary)?;
                // Preprocessing widens all integer types before promotion.
                // GCC and Clang therefore use uintmax_t for a character when
                // plain char is unsigned, including ordinary ASCII characters.
                let value = if ordinary && !self.char_unsigned {
                    i64::from(value as u8 as i8)
                } else {
                    value
                };
                Ok(Value {
                    bits: value as u64,
                    unsigned,
                })
            }
            _ => Err(format!("invalid token `{}` in #if expression", token.text)),
        }
    }
}

fn integer(text: &str, ms_extensions: bool) -> Result<Value, String> {
    let msvc_start = text
        .rfind(['i', 'I'])
        .filter(|&start| ms_extensions && matches!(&text[start + 1..], "8" | "16" | "32" | "64"))
        .map(|start| {
            start - usize::from(start > 0 && matches!(text.as_bytes()[start - 1], b'u' | b'U'))
        });
    let suffix_start =
        msvc_start.unwrap_or_else(|| text.trim_end_matches(['u', 'U', 'l', 'L']).len());
    let suffix = &text[suffix_start..];
    let size_suffix = suffix
        .strip_prefix(['u', 'U'])
        .or_else(|| suffix.strip_suffix(['u', 'U']))
        .unwrap_or(suffix);
    if msvc_start.is_none() && !matches!(size_suffix, "" | "l" | "L" | "ll" | "LL") {
        return Err(format!("invalid integer suffix in `{text}`"));
    }
    // Preprocessing interprets even Microsoft's fixed-width literals as
    // intmax_t or uintmax_t, without truncating to the written suffix width.
    let digits = &text[..suffix_start];
    let (radix, digits) = if let Some(digits) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (16, digits)
    } else if let Some(digits) = digits
        .strip_prefix("0b")
        .or_else(|| digits.strip_prefix("0B"))
    {
        (2, digits)
    } else if digits.starts_with('0') {
        (8, digits)
    } else {
        (10, digits)
    };
    let bits = u64::from_str_radix(digits, radix)
        .map_err(|_| format!("invalid or overflowing integer constant `{text}`"))?;
    let unsigned = suffix.contains(['u', 'U']) || (radix != 10 && bits > i64::MAX as u64);
    if !unsigned && bits > i64::MAX as u64 {
        return Err(format!("signed integer constant `{text}` exceeds intmax_t"));
    }
    Ok(Value { bits, unsigned })
}

fn character(text: &str, ordinary: bool) -> Result<i64, String> {
    let body = &text[1..text.len() - 1];
    let mut chars = body.chars();
    let value = match chars.next() {
        Some('\\') => match chars.next() {
            Some('n') => 10,
            Some('r') => 13,
            Some('t') => 9,
            Some('v') => 11,
            Some('a') => 7,
            Some('b') => 8,
            Some('f') => 12,
            Some(c @ ('\\' | '\'' | '"' | '?')) => i64::from(u32::from(c)),
            Some('x') => {
                let digits: String = chars.by_ref().collect();
                i64::from_str_radix(&digits, 16)
                    .map_err(|_| "invalid hexadecimal character escape")?
            }
            Some(c @ '0'..='7') => {
                let mut digits = String::from(c);
                for _ in 0..2 {
                    if chars.clone().next().is_some_and(|c| matches!(c, '0'..='7')) {
                        digits.push(chars.next().expect("peeked character"));
                    }
                }
                i64::from_str_radix(&digits, 8).map_err(|_| "invalid octal character escape")?
            }
            _ => return Err("unsupported character escape in #if".into()),
        },
        Some(c) if c.is_ascii() => i64::from(u32::from(c)),
        _ => return Err("non-ASCII or empty character constant in #if".into()),
    };
    if chars.next().is_some() {
        return Err("multicharacter constants in #if are not supported".into());
    }
    if value > if ordinary { 255 } else { 127 } {
        return Err(if ordinary {
            "numeric character escape in #if exceeds one byte".into()
        } else {
            "wide non-ASCII character constants in #if are not supported".into()
        });
    }
    Ok(value)
}

fn binary(operator: &str, left: Value, right: Value, evaluate: bool) -> Result<Value, String> {
    let unsigned = left.unsigned || right.unsigned;
    let signed_left = left.bits as i64;
    let signed_right = right.bits as i64;
    if matches!(
        operator,
        "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
    ) {
        let ordering = if unsigned {
            left.bits.cmp(&right.bits)
        } else {
            signed_left.cmp(&signed_right)
        };
        return Ok(Value::boolean(match operator {
            "==" => ordering.is_eq(),
            "!=" => !ordering.is_eq(),
            "<" => ordering.is_lt(),
            "<=" => ordering.is_le(),
            ">" => ordering.is_gt(),
            ">=" => ordering.is_ge(),
            "&&" => left.truth() && right.truth(),
            "||" => left.truth() || right.truth(),
            _ => unreachable!(),
        }));
    }
    if !evaluate {
        return Ok(Value {
            bits: 0,
            unsigned: if matches!(operator, "<<" | ">>") {
                left.unsigned
            } else {
                unsigned
            },
        });
    }
    if matches!(operator, "<<" | ">>") {
        let count = u32::try_from(right.bits)
            .ok()
            .filter(|count| *count < 64)
            .ok_or("shift count is outside 0..64 in #if")?;
        let bits = if operator == ">>" {
            if left.unsigned {
                left.bits >> count
            } else {
                (signed_left >> count) as u64
            }
        } else if left.unsigned {
            left.bits << count
        } else {
            let result = i128::from(signed_left) << count;
            if signed_left < 0 || result > i128::from(i64::MAX) {
                return Err("invalid signed left shift in #if".into());
            }
            result as u64
        };
        return Ok(Value {
            bits,
            unsigned: left.unsigned,
        });
    }
    let bits = match operator {
        "&" => left.bits & right.bits,
        "|" => left.bits | right.bits,
        "^" => left.bits ^ right.bits,
        "+" if unsigned => left.bits.wrapping_add(right.bits),
        "-" if unsigned => left.bits.wrapping_sub(right.bits),
        "*" if unsigned => left.bits.wrapping_mul(right.bits),
        "/" if unsigned => left
            .bits
            .checked_div(right.bits)
            .ok_or("division by zero in #if")?,
        "%" if unsigned => left
            .bits
            .checked_rem(right.bits)
            .ok_or("division by zero in #if")?,
        operator => {
            let result = match operator {
                "+" => signed_left.checked_add(signed_right),
                "-" => signed_left.checked_sub(signed_right),
                "*" => signed_left.checked_mul(signed_right),
                "/" => signed_left.checked_div(signed_right),
                "%" => signed_left.checked_rem(signed_right),
                _ => unreachable!(),
            };
            result.ok_or("signed overflow or division by zero in #if")? as u64
        }
    };
    Ok(Value { bits, unsigned })
}

#[cfg(test)]
mod tests {
    use super::evaluate;
    use crate::token::lex;

    #[test]
    fn long_long_suffixes_require_matching_case() {
        for ms_extensions in [false, true] {
            for suffix in [
                "ll", "LL", "ull", "uLL", "Ull", "ULL", "llu", "llU", "LLu", "LLU",
            ] {
                let expression = format!("1{suffix} == 1");
                assert!(
                    evaluate(&lex(&expression).unwrap(), None, false, ms_extensions).unwrap(),
                    "{expression}"
                );
            }
            for suffix in [
                "lL", "Ll", "ulL", "uLl", "UlL", "ULl", "lLu", "Llu", "lLU", "LlU",
            ] {
                let expression = format!("1{suffix}");
                assert!(
                    evaluate(&lex(&expression).unwrap(), None, false, ms_extensions)
                        .unwrap_err()
                        .contains("invalid integer suffix"),
                    "{expression}"
                );
            }
        }
    }

    #[test]
    fn c_integer_conversions_and_short_circuiting() {
        for expression in [
            "1 + 2 * 3 == 7",
            "-1 > 1U",
            "~0U == 0xffffffffffffffff",
            "0 || 1 && 2",
            "1 || 1 / 0",
            "!(0 && 1 / 0)",
            "1 ? 1 : 1 / 0",
            "0 ? 1 / 0 : 1",
            "(1 ? -1 : 0U) > 1",
            "'\\n' == 10",
            "010 == 8",
            "0b10 == 2",
        ] {
            assert!(
                evaluate(&lex(expression).unwrap(), None, false, false).unwrap(),
                "{expression}"
            );
        }
        assert!(!evaluate(&lex("-1 < 1U").unwrap(), None, false, false).unwrap());
        for expression in [
            "1 / 0",
            "1 << 64",
            "9223372036854775807 + 1",
            "1.2",
            "1 ? 2",
            "1 2",
        ] {
            assert!(
                evaluate(&lex(expression).unwrap(), None, false, false).is_err(),
                "{expression}"
            );
        }
    }
}

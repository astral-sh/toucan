use ast::*;

pub fn ts18661_float(binary: bool, width: usize, extended: bool) -> TS18661FloatType {
    TS18661FloatType {
        format: match (binary, extended) {
            (true, false) => TS18661FloatFormat::BinaryInterchange,
            (true, true) => TS18661FloatFormat::BinaryExtended,
            (false, false) => TS18661FloatFormat::DecimalInterchange,
            (false, true) => TS18661FloatFormat::DecimalExtended,
        },
        width,
    }
}

pub fn int_suffix(mut s: &str) -> Result<IntegerSuffix, &'static str> {
    let mut l = IntegerSize::Int;
    let mut u = false;
    let mut i = false;

    while !s.is_empty() {
        if l == IntegerSize::Int && (s.starts_with("ll") || s.starts_with("LL")) {
            l = IntegerSize::LongLong;
            s = &s[2..];
        } else if l == IntegerSize::Int && (s.starts_with("l") || s.starts_with("L")) {
            l = IntegerSize::Long;
            s = &s[1..];
        } else if !u && (s.starts_with("u") || s.starts_with("U")) {
            u = true;
            s = &s[1..];
        } else if !i
            && (s.starts_with("i")
                || s.starts_with("I")
                || s.starts_with("j")
                || s.starts_with("J"))
        {
            i = true;
            s = &s[1..];
        } else {
            return Err("integer suffix");
        }
    }

    Ok(IntegerSuffix {
        size: l,
        unsigned: u,
        imaginary: i,
    })
}

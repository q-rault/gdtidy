//! The values of literal tokens, decoded the way Godot's tokenizer does.
//!
//! Each function takes a token's text and returns `None` when the text is
//! not a valid literal of that kind (the parser reports those as errors).

/// A decoded literal.
#[derive(Debug, Clone, PartialEq)]
pub enum LiteralValue {
    Int(i64),
    Float(f64),
    String(String),
    /// `&"name"`.
    StringName(String),
    /// `^"path"`.
    NodePath(String),
    Bool(bool),
    Null,
}

/// The value of an `INT` token: decimal, `0x` hexadecimal or `0b` binary,
/// with `_` separators and an optional sign. `None` if it does not fit in
/// 64 bits.
pub fn int_value(text: &str) -> Option<i64> {
    let (negative, digits) = split_sign(text);
    let digits: String = digits.chars().filter(|&c| c != '_').collect();
    let (radix, digits) = if let Some(hex) = strip_prefix_ignore_case(&digits, "0x") {
        (16, hex)
    } else if let Some(bin) = strip_prefix_ignore_case(&digits, "0b") {
        (2, bin)
    } else {
        (10, digits.as_str())
    };
    if digits.is_empty() {
        return None;
    }
    // Parse with the sign so that `-9223372036854775808` fits.
    let signed = if negative {
        format!("-{digits}")
    } else {
        digits.to_string()
    };
    i64::from_str_radix(&signed, radix).ok()
}

/// The value of a `FLOAT` token, such as `1.5`, `.5`, `1.`, `1e-3` or
/// `1_000.0`.
pub fn float_value(text: &str) -> Option<f64> {
    let cleaned: String = text.chars().filter(|&c| c != '_').collect();
    cleaned.parse().ok()
}

/// The contents of a `STRING`, `STRING_NAME` or `NODE_PATH` token, with its
/// prefix and quotes removed and escapes decoded. Raw strings (`r"..."`) keep
/// their backslashes, except before a quote or a backslash, as in Godot.
pub fn string_value(text: &str) -> Option<String> {
    let (raw, rest) = match text.as_bytes().first()? {
        b'r' => (true, &text[1..]),
        b'&' | b'^' => (false, &text[1..]),
        _ => (false, text),
    };
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let triple: String = std::iter::repeat_n(quote, 3).collect();
    let body = if rest.len() >= 6 && rest.starts_with(&triple) && rest.ends_with(&triple) {
        &rest[3..rest.len() - 3]
    } else if rest.len() >= 2 && rest.ends_with(quote) {
        &rest[1..rest.len() - 1]
    } else {
        return None;
    };

    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    // A lead surrogate from a `\u` escape waiting for its trail.
    let mut lead: Option<u32> = None;
    while let Some(c) = chars.next() {
        if c != '\\' {
            lead = None;
            out.push(c);
            continue;
        }
        let code = chars.next()?;
        if raw {
            if code != quote && code != '\\' {
                out.push('\\');
            }
            out.push(code);
            continue;
        }
        let value = match code {
            'a' => 0x07,
            'b' => 0x08,
            'f' => 0x0c,
            'n' => 0x0a,
            'r' => 0x0d,
            't' => 0x09,
            'v' => 0x0b,
            '\'' | '"' | '\\' => code as u32,
            'u' | 'U' => {
                let count = if code == 'U' { 6 } else { 4 };
                let mut value = 0u32;
                for _ in 0..count {
                    value = (value << 4) | chars.next()?.to_digit(16)?;
                }
                value
            }
            // An escaped line break joins the lines.
            '\n' => continue,
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                } else {
                    out.push('\\');
                    out.push('\r');
                }
                continue;
            }
            _ => return None,
        };
        match value {
            0xd800..=0xdbff => lead = Some(value),
            0xdc00..=0xdfff => {
                let high = lead.take()?;
                let combined = 0x10000 + ((high - 0xd800) << 10) + (value - 0xdc00);
                out.push(char::from_u32(combined)?);
            }
            _ => {
                if lead.take().is_some() {
                    return None;
                }
                out.push(char::from_u32(value)?);
            }
        }
    }
    if lead.is_some() {
        return None;
    }
    Some(out)
}

fn split_sign(text: &str) -> (bool, &str) {
    match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    }
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    (text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix))
        .then(|| &text[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers() {
        assert_eq!(int_value("42"), Some(42));
        assert_eq!(int_value("1_000_000"), Some(1_000_000));
        assert_eq!(int_value("0xFF"), Some(255));
        assert_eq!(int_value("0b1010"), Some(10));
        assert_eq!(int_value("-2"), Some(-2));
        assert_eq!(int_value("-0x10"), Some(-16));
        assert_eq!(int_value("-9223372036854775808"), Some(i64::MIN));
        assert_eq!(int_value("9223372036854775808"), None);
    }

    #[test]
    fn floats() {
        assert_eq!(float_value("1.5"), Some(1.5));
        assert_eq!(float_value(".5"), Some(0.5));
        assert_eq!(float_value("1."), Some(1.0));
        assert_eq!(float_value("1e-3"), Some(0.001));
        assert_eq!(float_value("1_000.5"), Some(1000.5));
        assert_eq!(float_value("-2.5"), Some(-2.5));
    }

    #[test]
    fn strings() {
        assert_eq!(string_value(r#""a\tb\n""#).unwrap(), "a\tb\n");
        assert_eq!(string_value(r#"'it\'s'"#).unwrap(), "it's");
        assert_eq!(string_value(r#""\u00e9\U01F600""#).unwrap(), "é😀");
        assert_eq!(string_value(r#""\ud83d\ude00""#).unwrap(), "😀");
        assert_eq!(string_value("\"\"\"a\n\"b\"\"\"\"").unwrap(), "a\n\"b\"");
        assert_eq!(string_value("\"a\\\nb\"").unwrap(), "ab");
        assert_eq!(string_value(r#"r"C:\path\"q\"""#).unwrap(), r#"C:\path"q""#);
        assert_eq!(string_value(r#"&"name""#).unwrap(), "name");
        assert_eq!(string_value(r#"^"Path/To:prop""#).unwrap(), "Path/To:prop");
        assert_eq!(string_value(r#""bad \q""#), None);
        assert_eq!(string_value(r#""\ud83d""#), None);
    }
}

//! The tokens of a file as the printer sees them, after the spelling and
//! layout rewrites of RFC 0003:
//! - `and`, `or`, `not` instead of `&&`, `||`, `!`;
//! - double quotes unless single quotes need fewer escapes;
//! - lowercase hexadecimal digits, floats with both zeros;
//! - no parentheses around a whole one-line `if`/`elif`/`while` condition;
//! - one statement per line (`;` split or removed);
//! - enum members one per line;
//! - trailing commas in multi-line arrays, dictionaries and enums, none in
//!   one-line ones.

use std::collections::HashSet;

use gdsyntax::ast::{code_range, is_code};
use gdsyntax::{SyntaxKind as K, SyntaxNode, SyntaxToken};

use super::{Item, Options};

/// One element of the token stream.
#[derive(Debug)]
pub(super) enum Piece {
    /// A line break, with its byte range in the source (empty for a break
    /// the formatter adds).
    Break {
        start: usize,
        end: usize,
    },
    /// Whitespace within a line.
    Space(String),
    /// A `\` that continues the line.
    Continuation,
    Item(Item),
}

/// What the rewrites do to particular tokens, by offset.
#[derive(Default)]
struct Plan {
    drop: HashSet<usize>,
    break_before: HashSet<usize>,
    comma_after: HashSet<usize>,
}

pub(super) fn pieces(root: &SyntaxNode, options: &Options) -> Vec<Piece> {
    let plan = plan(root, options);
    let mut out = Vec::new();
    for token in root
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        let kind = token.kind();
        let range = token.text_range();
        let offset: usize = range.start().into();
        match kind {
            K::NEWLINE | K::LINE_BREAK => {
                if token.text().contains('\n') {
                    out.push(Piece::Break {
                        start: offset,
                        end: range.end().into(),
                    });
                }
                continue;
            }
            K::WHITESPACE => {
                out.push(Piece::Space(token.text().to_string()));
                continue;
            }
            K::LINE_CONTINUATION => {
                out.push(Piece::Continuation);
                continue;
            }
            K::INDENT | K::DEDENT | K::EOF | K::BYTE_ORDER_MARK => continue,
            _ => {}
        }
        if plan.drop.contains(&offset) {
            continue;
        }
        if plan.break_before.contains(&offset) {
            out.push(Piece::Break {
                start: offset,
                end: offset,
            });
        }
        let (kind, text) = respell(&token);
        out.push(Piece::Item(Item {
            text,
            kind,
            token: token.clone(),
            space: String::new(),
            inserted: false,
        }));
        if plan.comma_after.contains(&offset) {
            out.push(Piece::Item(Item {
                text: ",".to_string(),
                kind: K::COMMA,
                token,
                space: String::new(),
                inserted: true,
            }));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Spelling

/// A token's printed kind and text.
fn respell(token: &SyntaxToken) -> (K, String) {
    let text = token.text();
    match token.kind() {
        K::AMPERSAND_AMPERSAND => (K::AND_KW, "and".to_string()),
        K::PIPE_PIPE => (K::OR_KW, "or".to_string()),
        K::BANG if token.parent().is_some_and(|p| p.kind() == K::UNARY_EXPR) => {
            (K::NOT_KW, "not".to_string())
        }
        K::INT => (K::INT, int_text(text)),
        K::FLOAT => (K::FLOAT, float_text(text)),
        K::STRING | K::STRING_NAME | K::NODE_PATH => (
            token.kind(),
            requote(text).unwrap_or_else(|| text.to_string()),
        ),
        kind => (kind, text.trim_end().to_string()),
    }
}

/// Lowercase hexadecimal digits.
fn int_text(text: &str) -> String {
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.starts_with("0x") || digits.starts_with("0X") {
        text.to_ascii_lowercase()
    } else {
        text.to_string()
    }
}

/// Floats with both zeros: `.5` is `0.5`, `1.` is `1.0`.
fn float_text(text: &str) -> String {
    let (sign, body) = match text.as_bytes().first() {
        Some(b'-' | b'+') => text.split_at(1),
        _ => ("", text),
    };
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => body.split_at(i),
        None => (body, ""),
    };
    let mut mantissa = mantissa.to_string();
    if mantissa.starts_with('.') {
        mantissa.insert(0, '0');
    }
    if mantissa.ends_with('.') {
        mantissa.push('0');
    }
    format!("{sign}{mantissa}{exponent}")
}

/// A string literal in double quotes, unless single quotes need fewer
/// escapes. `None` keeps the literal as written.
fn requote(text: &str) -> Option<String> {
    let (prefix, rest) = match text.as_bytes().first()? {
        b'&' | b'^' => text.split_at(1),
        _ => ("", text),
    };
    let (raw, rest) = match rest.strip_prefix('r') {
        Some(rest) => ("r", rest),
        None => ("", rest),
    };
    for triple in ["\"\"\"", "'''"] {
        if let Some(body) = rest.strip_prefix(triple) {
            let body = body.strip_suffix(triple)?;
            // Only ''' to """, and only when nothing in it could matter.
            return (triple == "'''" && !body.contains(['"', '\\']))
                .then(|| format!("{prefix}{raw}\"\"\"{body}\"\"\""));
        }
    }
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let body = rest.strip_prefix(quote)?.strip_suffix(quote)?;
    if !raw.is_empty() {
        return (!body.contains(['"', '\'', '\\'])).then(|| format!("{prefix}r\"{body}\""));
    }

    // Split the body into quotes (escaped or not) and everything else, so it
    // can be written with either delimiter.
    enum Unit<'a> {
        Single,
        Double,
        Text(&'a str),
    }
    let mut units = Vec::new();
    let mut chars = body.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some((_, '\'')) => units.push(Unit::Single),
                Some((_, '"')) => units.push(Unit::Double),
                Some((j, n)) => units.push(Unit::Text(&body[i..j + n.len_utf8()])),
                None => return None,
            },
            '\'' => units.push(Unit::Single),
            '"' => units.push(Unit::Double),
            _ => units.push(Unit::Text(&body[i..i + c.len_utf8()])),
        }
    }
    let singles = units.iter().filter(|u| matches!(u, Unit::Single)).count();
    let doubles = units.iter().filter(|u| matches!(u, Unit::Double)).count();
    let target = if singles < doubles { '\'' } else { '"' };
    let mut out = format!("{prefix}{target}");
    for unit in &units {
        match unit {
            Unit::Single if target == '\'' => out.push_str("\\'"),
            Unit::Single => out.push('\''),
            Unit::Double if target == '"' => out.push_str("\\\""),
            Unit::Double => out.push('"'),
            Unit::Text(t) => out.push_str(t),
        }
    }
    out.push(target);
    Some(out)
}

// ---------------------------------------------------------------------------
// Layout

fn plan(root: &SyntaxNode, options: &Options) -> Plan {
    let mut plan = Plan::default();
    for node in root.descendants() {
        match node.kind() {
            K::PAREN_EXPR => condition_parens(&node, &mut plan),
            K::ENUM => {
                enum_members(&node, &mut plan);
                commas(&node, options, &mut plan);
            }
            K::ARRAY | K::DICT => commas(&node, options, &mut plan),
            _ => {}
        }
    }
    for token in root
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        if token.kind() == K::SEMICOLON {
            semicolon(&token, &mut plan);
        }
    }
    plan
}

fn offset(token: &SyntaxToken) -> usize {
    token.text_range().start().into()
}

fn code_children(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + use<> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(is_code)
}

/// Whether there is a line break between two offsets.
fn line_break_between(node: &SyntaxNode, from: usize, to: usize) -> bool {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .any(|t| {
            let at = offset(&t);
            from <= at
                && at < to
                && (matches!(t.kind(), K::NEWLINE | K::LINE_BREAK) && t.text().contains('\n')
                    || t.text().contains('\n'))
        })
}

/// `if (a and b):` becomes `if a and b:`.
fn condition_parens(paren: &SyntaxNode, plan: &mut Plan) {
    let Some(parent) = paren.parent() else { return };
    if !matches!(parent.kind(), K::IF_STMT | K::ELIF_CLAUSE | K::WHILE_STMT) {
        return;
    }
    // A ternary or a lambda reads differently without them; leave those.
    if paren
        .children()
        .any(|c| matches!(c.kind(), K::TERNARY_EXPR | K::LAMBDA))
    {
        return;
    }
    let range = paren.text_range();
    if line_break_between(paren, range.start().into(), range.end().into()) {
        return;
    }
    for token in code_children(paren) {
        if matches!(token.kind(), K::PARENTHESIS_OPEN | K::PARENTHESIS_CLOSE) {
            plan.drop.insert(offset(&token));
        }
    }
}

/// Each enum member on its own line.
fn enum_members(node: &SyntaxNode, plan: &mut Plan) {
    let mut breaks = Vec::new();
    let mut previous: Option<SyntaxToken> = None;
    for element in node.children_with_tokens() {
        let first = match &element {
            gdsyntax::rowan::NodeOrToken::Node(n) if n.kind() == K::ENUM_VALUE => {
                n.first_token().filter(is_code).or_else(|| {
                    code_range(n).and_then(|r| n.token_at_offset(r.start()).right_biased())
                })
            }
            gdsyntax::rowan::NodeOrToken::Token(t) if t.kind() == K::BRACE_CLOSE => Some(t.clone()),
            gdsyntax::rowan::NodeOrToken::Token(t) if t.kind() == K::BRACE_OPEN => {
                previous = Some(t.clone());
                continue;
            }
            gdsyntax::rowan::NodeOrToken::Token(t) if is_code(t) => {
                previous = Some(t.clone());
                continue;
            }
            _ => continue,
        };
        let Some(first) = first else { continue };
        if let Some(prev) = &previous
            && !line_break_between(node, offset(prev), offset(&first))
            && !(first.kind() == K::BRACE_CLOSE && prev.kind() == K::BRACE_OPEN)
        {
            breaks.push(offset(&first));
        }
        previous = match &element {
            gdsyntax::rowan::NodeOrToken::Node(n) => n.last_token(),
            gdsyntax::rowan::NodeOrToken::Token(t) => Some(t.clone()),
        };
    }
    plan.break_before.extend(breaks);
}

/// Trailing commas: added when the closing bracket is on its own line,
/// removed when the collection is on one line.
fn commas(node: &SyntaxNode, options: &Options, plan: &mut Plan) {
    let tokens: Vec<SyntaxToken> = code_children(node).collect();
    let (Some(open), Some(close)) = (tokens.first(), tokens.last()) else {
        return;
    };
    if !matches!(close.kind(), K::BRACKET_CLOSE | K::BRACE_CLOSE) {
        return;
    }
    // The last code token before the closer, anywhere inside the node.
    let Some(last) = node
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| is_code(t) && offset(t) < offset(close))
        .last()
    else {
        return;
    };
    if last.kind() == open.kind() {
        return; // Empty.
    }
    let closer_alone = line_break_between(node, offset(&last), offset(close))
        || plan.break_before.contains(&offset(close));
    let one_line = !line_break_between(node, offset(open), offset(close))
        && !plan
            .break_before
            .iter()
            .any(|&b| offset(open) < b && b <= offset(close));
    if last.kind() == K::COMMA {
        if one_line {
            plan.drop.insert(offset(&last));
        }
    } else if closer_alone && options.trailing_commas {
        plan.comma_after.insert(offset(&last));
    }
}

/// One statement per line: a `;` is removed, and a statement after it on
/// the same line moves to its own line. A one-line block holding several
/// statements becomes a block. One-line lambdas are left alone.
fn semicolon(token: &SyntaxToken, plan: &mut Plan) {
    let Some(statement) = token.parent() else {
        return;
    };
    let Some(block) = statement.parent() else {
        return;
    };
    if block.kind() == K::SUITE && block.parent().is_some_and(|p| p.kind() == K::LAMBDA) {
        return;
    }
    if !matches!(block.kind(), K::SUITE | K::CLASS_BODY | K::SOURCE_FILE) {
        return;
    }
    plan.drop.insert(offset(token));
    // The next code token, if it is on the same line.
    let mut next = token.next_token();
    while let Some(t) = &next {
        if is_code(t) && !matches!(t.kind(), K::INDENT | K::DEDENT) {
            break;
        }
        if t.text().contains('\n') || t.kind() == K::NEWLINE {
            return;
        }
        next = t.next_token();
    }
    let Some(next) = next else { return };
    if next.kind() == K::SEMICOLON || next.kind() == K::EOF {
        return;
    }
    plan.break_before.insert(offset(&next));
    // A one-line block: its first statement goes on its own line too.
    if block.kind() == K::SUITE
        && !block.children_with_tokens().any(|c| c.kind() == K::INDENT)
        && let Some(first) = code_range(&block)
    {
        plan.break_before.insert(first.start().into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes() {
        assert_eq!(requote("'a'").unwrap(), "\"a\"");
        assert_eq!(requote("'say \"hi\"'").unwrap(), "'say \"hi\"'");
        assert_eq!(requote("\"say \\\"hi\\\"\"").unwrap(), "'say \"hi\"'");
        assert_eq!(requote("'it\\'s'").unwrap(), "\"it's\"");
        assert_eq!(requote("'\\n'").unwrap(), "\"\\n\"");
        assert_eq!(requote("&'name'").unwrap(), "&\"name\"");
        assert_eq!(requote("^'path'").unwrap(), "^\"path\"");
        assert_eq!(requote("r'a'").unwrap(), "r\"a\"");
        assert!(requote("r'a\\'b'").is_none());
        assert_eq!(requote("'''a'''").unwrap(), "\"\"\"a\"\"\"");
        assert!(requote("'''a\"b'''").is_none());
        // A tie keeps double quotes.
        assert_eq!(requote("'\"\\''").unwrap(), "\"\\\"'\"");
    }

    #[test]
    fn numbers() {
        assert_eq!(int_text("0xFF"), "0xff");
        assert_eq!(int_text("-0xAB_CD"), "-0xab_cd");
        assert_eq!(int_text("0b1010"), "0b1010");
        assert_eq!(float_text(".5"), "0.5");
        assert_eq!(float_text("1."), "1.0");
        assert_eq!(float_text("-.5e3"), "-0.5e3");
        assert_eq!(float_text("1.e5"), "1.0e5");
        assert_eq!(float_text("+.5"), "+0.5");
        assert_eq!(float_text("1e5"), "1e5");
        assert_eq!(float_text("1_000.25"), "1_000.25");
    }
}

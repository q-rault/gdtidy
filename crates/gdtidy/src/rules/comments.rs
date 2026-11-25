//! Rules about comments: `doc-tag` and `comment-space`.

use gdsyntax::SyntaxKind::{COMMENT, DOC_COMMENT};
use gdsyntax::SyntaxToken;

use super::{Context, Edit, Finding, Fix, Rule, token_range};

pub const DOC_TAG: Rule = Rule {
    id: "doc-tag",
    concern: "doc-comment-tags",
    summary: "A `##` doc comment tag that Godot ignores or shows as plain text.",
    default_enabled: true,
    fixable: true,
    options: &[],
    check: Some(doc_tag),
};

pub const COMMENT_SPACE: Rule = Rule {
    id: "comment-space",
    concern: "comment-spacing",
    summary: "A comment starts without a space after `#`.",
    default_enabled: false,
    fixable: true,
    options: &[],
    check: Some(comment_space),
};

fn tokens(cx: &Context, kind: gdsyntax::SyntaxKind) -> impl Iterator<Item = SyntaxToken> {
    cx.tree
        .root()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(move |t| t.kind() == kind)
}

// ---- doc-tag ----

/// What is wrong with a doc comment line, following how Godot 4.7 reads
/// tags (`GDScriptParser::parse_class_doc_comment`), or `None`.
fn doc_tag_problem(line: &str) -> Option<String> {
    let tag = line.strip_prefix('@')?;
    let word: String = tag
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if word.is_empty() || !word.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let rest = &tag[word.len()..];
    match word.as_str() {
        "deprecated" | "experimental" => (!rest.is_empty() && !rest.starts_with(':')).then(|| {
            format!(
                "Godot reads `@{word}` only as `## @{word}` or `## @{word}: reason`; this line is shown as text."
            )
        }),
        "tutorial" => (!valid_tutorial(rest)).then(|| {
            "Godot drops this `@tutorial` line; write `## @tutorial: url` or `## @tutorial(Title): url`."
                .to_string()
        }),
        _ => Some(format!(
            "`@{word}` is not a doc tag Godot knows (`@deprecated`, `@experimental`, `@tutorial`); it is shown as text."
        )),
    }
}

/// Whether the text after `@tutorial` is `: url` or `(Title): url`, with
/// spaces or tabs allowed around the parentheses, as Godot accepts.
fn valid_tutorial(rest: &str) -> bool {
    if rest.starts_with(':') {
        return true;
    }
    let Some(rest) = rest.trim_start_matches([' ', '\t']).strip_prefix('(') else {
        return false;
    };
    let Some(close) = rest.find(')') else {
        return false;
    };
    rest[close + 1..]
        .trim_start_matches([' ', '\t'])
        .starts_with(':')
}

/// The colon a tag lacks, or has too far, as `(range, text)` within `line`:
/// `@deprecated use b()` is `@deprecated: use b()`, and `@tutorial(T) url` is
/// `@tutorial(T): url`. `None` when that would not make it a tag Godot reads.
fn doc_tag_fix(line: &str) -> Option<(std::ops::Range<usize>, &'static str)> {
    let word_len = line[1..]
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(line.len() - 1);
    let mut end = 1 + word_len;
    if &line[1..end] == "tutorial" {
        let title = line[end..].trim_start_matches([' ', '\t']);
        if title.starts_with('(') {
            end = line.len() - title.len() + title.find(')')? + 1;
        }
    }
    let rest = &line[end..];
    let spaces = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    // Text glued to the tag, as in `@deprecated(use b)`, is no reason.
    if spaces == 0 {
        return None;
    }
    let fix = if rest[spaces..].starts_with(':') {
        (end..end + spaces + 1, ":")
    } else {
        (end..end, ":")
    };
    let mut fixed = line.to_string();
    fixed.replace_range(fix.0.clone(), fix.1);
    doc_tag_problem(&fixed).is_none().then_some(fix)
}

fn doc_tag(cx: &Context, out: &mut Vec<Finding>) {
    // Tags are not read inside a [codeblock].
    let mut in_code = false;
    for comment in tokens(cx, DOC_COMMENT) {
        let line = comment.text().trim_start_matches('#').trim();
        if line.contains("[codeblock") {
            in_code = true;
        }
        if in_code {
            if line.contains("[/codeblock]") {
                in_code = false;
            }
            continue;
        }
        if let Some(message) = doc_tag_problem(line) {
            let range = token_range(&comment);
            let after_marks = comment.text().trim_start_matches('#').trim_start();
            let line_start = range.start + (comment.text().len() - after_marks.len());
            let fix = doc_tag_fix(line).map(|(at, text)| Fix {
                edits: vec![Edit {
                    range: line_start + at.start..line_start + at.end,
                    text: text.to_string(),
                }],
            });
            out.push(Finding {
                range,
                message,
                fix,
            });
        }
    }
}

// ---- comment-space ----

/// Whether the text of a comment reads as commented-out code, which the
/// style guide writes without a space after `#`.
fn is_code(text: &str) -> bool {
    let text = text.trim();
    // A lone word parses as a name, but it is a word, not code.
    if text.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return false;
    }
    let parses = |source: String| gdsyntax::parse(source).errors().is_empty();
    parses(format!("{text}\n"))
        || parses(format!("{text}\n\tpass\n"))
        || parses(format!("func _f():\n\t{text}\n"))
        || parses(format!("func _f():\n\t{text}\n\t\tpass\n"))
}

fn comment_space(cx: &Context, out: &mut Vec<Finding>) {
    for comment in tokens(cx, COMMENT) {
        let text = &comment.text()[1..];
        // Only words: `#---` separators, `#!` and the like are left alone.
        if !text.starts_with(char::is_alphanumeric) {
            continue;
        }
        let word: String = text.chars().take_while(|c| c.is_alphanumeric()).collect();
        if matches!(word.as_str(), "region" | "endregion") || is_code(text) {
            continue;
        }
        let start = usize::from(comment.text_range().start());
        out.push(Finding {
            range: token_range(&comment),
            message: "Put a space after `#` in a comment.".to_string(),
            fix: Some(Fix {
                edits: vec![Edit {
                    range: start + 1..start + 1,
                    text: " ".to_string(),
                }],
            }),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_tags() {
        assert_eq!(doc_tag_problem("@deprecated"), None);
        assert_eq!(doc_tag_problem("@deprecated: use b()"), None);
        assert!(doc_tag_problem("@deprecated : use b()").is_some());
        assert!(doc_tag_problem("@deprecated use b()").is_some());
        assert_eq!(doc_tag_problem("@experimental"), None);
        assert_eq!(doc_tag_problem("@tutorial: https://a.b"), None);
        assert_eq!(doc_tag_problem("@tutorial(Title): https://a.b"), None);
        assert_eq!(doc_tag_problem("@tutorial ( Title )  : https://a.b"), None);
        assert!(doc_tag_problem("@tutorial https://a.b").is_some());
        assert!(doc_tag_problem("@tutorial(Title) https://a.b").is_some());
        assert!(doc_tag_problem("@param x the value").is_some());
        assert_eq!(doc_tag_problem("Some text with @param inside"), None);
        assert_eq!(doc_tag_problem("@ alone"), None);
    }

    #[test]
    fn doc_tag_fixes() {
        let fixed = |line: &str| {
            let (range, text) = doc_tag_fix(line)?;
            let mut fixed = line.to_string();
            fixed.replace_range(range, text);
            Some(fixed)
        };
        let some = |text: &str| Some(text.to_string());
        assert_eq!(fixed("@deprecated use b()"), some("@deprecated: use b()"));
        assert_eq!(fixed("@deprecated : use b()"), some("@deprecated: use b()"));
        assert_eq!(
            fixed("@tutorial https://a.b"),
            some("@tutorial: https://a.b")
        );
        assert_eq!(
            fixed("@tutorial(T) https://a.b"),
            some("@tutorial(T): https://a.b")
        );
        assert_eq!(fixed("@tutorial"), None);
        assert_eq!(fixed("@tutorial(T"), None);
        assert_eq!(fixed("@deprecated(use b)"), None);
        assert_eq!(fixed("@param x the value"), None);
    }

    #[test]
    fn commented_out_code() {
        assert!(is_code("print(x)"));
        assert!(is_code("var x = 1"));
        assert!(is_code("func f():"));
        assert!(is_code("if x > 1:"));
        assert!(is_code("x += 1"));
        assert!(!is_code("comment"));
        assert!(!is_code("TODO: fix this"));
        assert!(!is_code("this is a comment"));
    }
}

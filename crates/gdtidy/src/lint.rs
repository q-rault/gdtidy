//! `gdtidy lint` for one file.

use serde::Serialize;

use crate::config::Config;
use crate::rules::{self, Context, Finding, Fix, Rule, Severity};
use crate::suppress::Suppressions;

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub severity: Severity,
    pub rule: &'static str,
    pub concern: &'static str,
    pub fixable: bool,
    pub message: String,
    /// The repair `--fix` applies, if the rule offers one here.
    #[serde(skip)]
    pub fix: Option<Fix>,
}

/// A file gdtidy could not check. This is not a lint problem: a syntax error
/// is Godot's to report, so gdtypecheck owns it. gdtidy only says the file was
/// left out, with the reason, so a run is never silently incomplete.
#[derive(Debug, Clone, Serialize)]
pub struct Skipped {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub reason: SkipReason,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkipReason {
    /// The file has a syntax error. The message is Godot's.
    SyntaxError,
    /// The file is not UTF-8, which Godot refuses too.
    NotUtf8,
    /// `gdtidy fmt` only: the formatted text failed the safety check.
    Unsafe,
}

/// What linting one file gave.
pub enum Outcome {
    Checked(Vec<Diagnostic>),
    Skipped(Skipped),
}

/// Lint one file's contents with the rules `config` turns on.
pub fn lint_file(shown: String, bytes: &[u8], config: &Config) -> Outcome {
    let tree = gdsyntax::parse_bytes(bytes);
    // Rules would only add noise on a broken tree, so a file that does not
    // parse is left out, with Godot's first message as the reason.
    if let Some(error) = tree.errors().into_iter().next() {
        let reason = if error.message == gdsyntax::INVALID_UTF8_MESSAGE {
            SkipReason::NotUtf8
        } else {
            SkipReason::SyntaxError
        };
        return Outcome::Skipped(Skipped {
            path: shown,
            line: error.start.line,
            column: error.start.column,
            reason,
            message: error.message,
        });
    }

    let suppressions = Suppressions::read(&tree);
    let mut diagnostics = Vec::new();
    let mut report = |rule: &'static Rule, finding: Finding| {
        let start = tree.position(finding.range.start);
        if suppressions.is_suppressed(rule.id, start.line) {
            return;
        }
        let end = tree.position(finding.range.end);
        diagnostics.push(Diagnostic {
            path: shown.clone(),
            line: start.line,
            column: start.column,
            end_line: end.line,
            end_column: end.column,
            severity: Severity::Warning,
            rule: rule.id,
            concern: rule.concern,
            fixable: rule.fixable,
            message: finding.message,
            fix: finding.fix,
        });
    };

    for (rule, options) in &config.rules {
        let Some(check) = rule.check else { continue };
        let mut findings = Vec::new();
        check(
            &Context {
                tree: &tree,
                options,
                file_name: shown.rsplit(['/', '\\']).next().unwrap_or(&shown),
            },
            &mut findings,
        );
        for finding in findings {
            report(rule, finding);
        }
    }
    if config.is_enabled(rules::UNKNOWN_RULE_IN_COMMENT.id) {
        for (range, name) in &suppressions.unknown {
            report(
                &rules::UNKNOWN_RULE_IN_COMMENT,
                Finding::new(
                    range.clone(),
                    format!("`{name}` is not a rule gdtidy knows (see `gdtidy rules`)."),
                ),
            );
        }
    }
    diagnostics.sort_by(|a, b| (a.line, a.column, a.rule).cmp(&(b.line, b.column, b.rule)));
    Outcome::Checked(diagnostics)
}

/// The source with every safe fix applied, and how many problems that
/// fixed; `None` when there is nothing to fix, or when the fixed text fails
/// the safety check, in which case the file is left as it is.
///
/// The check: the fixed text parses without errors, and its code tokens
/// are the original's minus those the fixes remove. A fix can change
/// comments and blank space, and delete code, never add or alter it.
pub fn fix_file(shown: String, bytes: &[u8], config: &Config) -> Option<(String, usize)> {
    let Outcome::Checked(diagnostics) = lint_file(shown, bytes, config) else {
        return None;
    };
    let source = std::str::from_utf8(bytes).ok()?;
    let mut fixes: Vec<&Fix> = diagnostics.iter().filter_map(|d| d.fix.as_ref()).collect();
    fixes.sort_by_key(|fix| fix.edits.iter().map(|e| e.range.start).min());
    fixes.dedup();
    // Keep fixes whose edits overlap no earlier fix's.
    let mut edits: Vec<&rules::Edit> = Vec::new();
    let mut count = 0;
    for fix in fixes {
        let overlaps = fix.edits.iter().any(|edit| {
            edits.iter().any(|kept| {
                edit.range.start < kept.range.end.max(kept.range.start + 1)
                    && kept.range.start < edit.range.end.max(edit.range.start + 1)
            })
        });
        if !overlaps {
            edits.extend(&fix.edits);
            count += 1;
        }
    }
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.range.start));
    let mut fixed = source.to_string();
    for edit in &edits {
        fixed.replace_range(edit.range.clone(), &edit.text);
    }

    let code = |tree: &gdsyntax::SyntaxTree,
                removed: &[&rules::Edit]|
     -> Vec<(gdsyntax::SyntaxKind, String)> {
        gdsyntax::ast::code_tokens(tree.root())
            .filter(|t| {
                let (start, end) = (
                    usize::from(t.text_range().start()),
                    usize::from(t.text_range().end()),
                );
                !removed
                    .iter()
                    .any(|e| e.range.start <= start && end <= e.range.end)
            })
            .map(|t| (t.kind(), t.text().to_string()))
            .collect()
    };
    let before = gdsyntax::parse(source);
    let after = gdsyntax::parse(fixed.clone());
    (after.errors().is_empty() && code(&after, &[]) == code(&before, &edits))
        .then_some((fixed, count))
}

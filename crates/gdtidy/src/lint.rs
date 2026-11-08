//! `gdtidy lint` for one file.

use serde::Serialize;

use crate::config::Config;
use crate::rules::{self, Context, Finding, Rule, Severity};
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
        });
    };

    for (rule, options) in &config.rules {
        let Some(check) = rule.check else { continue };
        let mut findings = Vec::new();
        check(
            &Context {
                tree: &tree,
                options,
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
                Finding {
                    range: range.clone(),
                    message: format!("`{name}` is not a rule gdtidy knows (see `gdtidy rules`)."),
                },
            );
        }
    }
    diagnostics.sort_by(|a, b| (a.line, a.column, a.rule).cmp(&(b.line, b.column, b.rule)));
    Outcome::Checked(diagnostics)
}

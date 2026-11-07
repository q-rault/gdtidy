//! `gdtidy lint`.

use std::path::Path;

use serde::Serialize;

use crate::rules::{PARSE_ERROR, Rule, Severity};

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub severity: Severity,
    pub rule: &'static str,
    pub message: String,
}

impl Diagnostic {
    fn new(path: &str, rule: Rule, error: &gdsyntax::SyntaxError) -> Self {
        Diagnostic {
            path: path.to_string(),
            line: error.start.line,
            column: error.start.column,
            end_line: error.end.line,
            end_column: error.end.column,
            severity: rule.severity,
            rule: rule.id,
            message: error.message.clone(),
        }
    }
}

/// Lint one file's source.
pub fn lint_source(path: &Path, source: String) -> Vec<Diagnostic> {
    let shown = crate::discover::display_path(path);
    let tree = gdsyntax::parse(source);
    // A file that does not parse is reported once and not checked further:
    // rules would only add noise on a broken tree.
    tree.errors()
        .first()
        .map(|error| vec![Diagnostic::new(&shown, PARSE_ERROR, error)])
        .unwrap_or_default()
}

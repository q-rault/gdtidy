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

/// Lint one file's contents. A file that is not UTF-8 is reported and skipped,
/// so one bad file does not stop the run.
pub fn lint_file(path: &Path, bytes: Vec<u8>) -> Vec<Diagnostic> {
    let shown = crate::discover::display_path(path);
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(error) => {
            let valid = &error.as_bytes()[..error.utf8_error().valid_up_to()];
            let before = String::from_utf8_lossy(valid);
            let line = before.matches('\n').count() + 1;
            let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
            return vec![Diagnostic {
                path: shown,
                line,
                column,
                end_line: line,
                end_column: column + 1,
                severity: PARSE_ERROR.severity,
                rule: PARSE_ERROR.id,
                message: "the file is not valid UTF-8".to_string(),
            }];
        }
    };
    let tree = gdsyntax::parse(source);
    // A file that does not parse is reported once and not checked further:
    // rules would only add noise on a broken tree.
    tree.errors()
        .first()
        .map(|error| vec![Diagnostic::new(&shown, PARSE_ERROR, error)])
        .unwrap_or_default()
}

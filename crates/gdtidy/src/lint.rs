//! `gdtidy lint`.

use std::path::Path;

use serde::Serialize;

use crate::rules::Severity;

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

/// Lint one file's contents.
pub fn lint_file(path: &Path, bytes: Vec<u8>) -> Outcome {
    let shown = crate::discover::display_path(path);
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(error) => {
            let valid = &error.as_bytes()[..error.utf8_error().valid_up_to()];
            let before = String::from_utf8_lossy(valid);
            return Outcome::Skipped(Skipped {
                path: shown,
                line: before.matches('\n').count() + 1,
                column: before.rsplit('\n').next().unwrap_or("").chars().count() + 1,
                reason: SkipReason::NotUtf8,
                message: "the file is not valid UTF-8".to_string(),
            });
        }
    };
    let tree = gdsyntax::parse(source);
    // Rules would only add noise on a broken tree, so a file that does not
    // parse is left out, with Godot's first message as the reason.
    if let Some(error) = tree.errors().first() {
        return Outcome::Skipped(Skipped {
            path: shown,
            line: error.start.line,
            column: error.start.column,
            reason: SkipReason::SyntaxError,
            message: error.message.clone(),
        });
    }
    Outcome::Checked(Vec::new())
}

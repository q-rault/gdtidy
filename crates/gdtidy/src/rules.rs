//! The rule registry. Every finding gdtidy reports comes from a rule listed
//! here, and `gdtidy rules --format json` publishes this list so gdsuite can
//! check that no two tools claim the same concern.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
// No rule exists yet, so neither severity is used.
#[allow(dead_code)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Rule {
    /// Stable identifier, shown in reports and used to disable the rule.
    pub id: &'static str,
    /// What the rule is about. Two tools of the suite must never share one.
    pub concern: &'static str,
    pub severity: Severity,
    pub summary: &'static str,
}

/// No rule exists yet. Files that do not parse are not a rule: they are
/// skipped and listed apart (see `lint::Skipped`), since syntax errors are
/// Godot's to report and so belong to gdtypecheck.
pub const RULES: &[Rule] = &[];

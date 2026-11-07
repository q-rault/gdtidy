//! The rule registry. Every finding gdtidy reports comes from a rule listed
//! here, and `gdtidy rules --format json` publishes this list so gdsuite can
//! check that no two tools claim the same concern.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    // Style rules will be warnings; none exists yet.
    #[allow(dead_code)]
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

pub const PARSE_ERROR: Rule = Rule {
    id: "parse-error",
    concern: "unparsable-file",
    severity: Severity::Error,
    summary: "The file has a syntax error, so it was not checked. The message is Godot's; type errors and other compile errors are gdtypecheck's to report.",
};

pub const RULES: &[Rule] = &[PARSE_ERROR];

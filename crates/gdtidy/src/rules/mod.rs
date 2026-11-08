//! The rule registry. Every finding gdtidy reports comes from a rule listed
//! here, and `gdtidy rules --format json` publishes this list (with each
//! rule's concern and options) so gdsuite can check that no two tools claim
//! the same concern.

mod limits;

use std::ops::Range;

use serde::Serialize;

use crate::config::RuleOptions;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Warning => "warning",
        }
    }
}

/// What a rule is given for one file.
pub struct Context<'a> {
    pub tree: &'a gdsyntax::SyntaxTree,
    pub options: &'a RuleOptions,
}

/// A problem a rule found, as a byte range of the source.
#[derive(Debug, Clone)]
pub struct Finding {
    pub range: Range<usize>,
    pub message: String,
}

pub type Check = fn(&Context, &mut Vec<Finding>);

/// The type and default of one rule option.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "type", content = "default", rename_all = "lowercase")]
// The naming rules (M2) and member order (M3) use the string kinds.
#[allow(dead_code)]
pub enum OptionKind {
    Integer(i64),
    String(&'static str),
    #[serde(rename = "string-list")]
    StringList(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RuleOption {
    pub name: &'static str,
    #[serde(flatten)]
    pub kind: OptionKind,
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Rule {
    /// Stable identifier, shown in reports and used to configure the rule.
    pub id: &'static str,
    /// What the rule is about. Two tools of the suite must never share one.
    pub concern: &'static str,
    pub summary: &'static str,
    /// Whether the rule runs without being enabled in `gdtidy.toml`.
    pub default_enabled: bool,
    /// Whether `--fix` can repair what the rule reports.
    pub fixable: bool,
    pub options: &'static [RuleOption],
    #[serde(skip)]
    pub check: Option<Check>,
}

impl Rule {
    pub fn option(&self, name: &str) -> Option<&RuleOption> {
        self.options.iter().find(|o| o.name == name)
    }
}

/// Reported for a silencing comment that names no known rule. It has no
/// check of its own: comments are read in `suppress`.
pub const UNKNOWN_RULE_IN_COMMENT: Rule = Rule {
    id: "unknown-rule-in-comment",
    concern: "suppression-comments",
    summary: "A `# gdtidy:` (or gdlint) comment names a rule that does not exist.",
    default_enabled: true,
    fixable: false,
    options: &[],
    check: None,
};

pub const RULES: &[Rule] = &[limits::MAX_LINE_LENGTH, UNKNOWN_RULE_IN_COMMENT];

pub fn find(id: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.id == id)
}

/// gdlint rules (and their equivalents in other linters) that gdtidy lint
/// leaves to another tool, with where they went. Naming one in `gdtidy.toml`
/// is an error that says so; naming one in a silencing comment is accepted
/// silently, so old comments do not turn into noise.
pub const MOVED: &[(&str, &str)] = &[
    (
        "unused-argument",
        "Godot reports it as the UNUSED_PARAMETER warning (gdtypecheck)",
    ),
    (
        "expression-not-assigned",
        "Godot reports it as the STANDALONE_EXPRESSION warning (gdtypecheck)",
    ),
    (
        "standalone-expression",
        "Godot reports it as the STANDALONE_EXPRESSION warning (gdtypecheck)",
    ),
    (
        "mixed-tabs-and-spaces",
        "Godot reports mixed indentation as a syntax error (gdtypecheck)",
    ),
    (
        "trailing-whitespace",
        "`gdtidy fmt` removes trailing whitespace",
    ),
    (
        "tab-characters",
        "it is the `tab-width` option of `max-line-length` in gdtidy",
    ),
];

pub fn moved(id: &str) -> Option<&'static str> {
    MOVED
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, to)| *to)
}

/// Rules RFC 0002 adds in later milestones. Until each lands, silencing
/// comments that name it are accepted, and `gdtidy.toml` says it is not
/// implemented yet. Each milestone removes the rules it implements.
pub const PLANNED: &[&str] = &[
    // M2: naming
    "function-name",
    "class-name",
    "sub-class-name",
    "signal-name",
    "class-variable-name",
    "class-load-variable-name",
    "function-variable-name",
    "function-preload-variable-name",
    "function-argument-name",
    "loop-variable-name",
    "enum-name",
    "enum-element-name",
    "constant-name",
    "load-constant-name",
    // M3: order and limits
    "class-definitions-order",
    "max-returns",
    "function-arguments-number",
    "max-public-methods",
    "max-file-lines",
    "max-function-lines",
    "max-branches",
    "max-nesting-depth",
    // M4: the rest
    "duplicated-load",
    "comparison-with-itself",
    "no-elif-return",
    "no-else-return",
    "unnecessary-pass",
    "self-assignment",
    "loop-variable-assignment",
    "doc-tag",
    "comment-space",
    "file-name",
    "private-call",
];

/// Names GDQuest's linter uses for gdtidy rules. A silencing comment that
/// names one silences the gdtidy rules it stands for.
pub const ALIASES: &[(&str, &[&str])] = &[
    ("private-access", &["private-call"]),
    ("enum-member-name", &["enum-element-name"]),
    (
        "variable-name",
        &["class-variable-name", "function-variable-name"],
    ),
];

/// The gdtidy rules a name in a silencing comment stands for: itself, or
/// what it is an alias of.
pub fn resolve_alias(name: &str) -> &[&str] {
    ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map_or(&[][..], |(_, ids)| ids)
}

/// Whether a silencing comment may name `name` without being reported.
pub fn known_in_comment(name: &str) -> bool {
    find(name).is_some()
        || moved(name).is_some()
        || PLANNED.contains(&name)
        || !resolve_alias(name).is_empty()
}

//! The rule registry. Every finding gdtidy reports comes from a rule listed
//! here, and `gdtidy rules --format json` publishes this list (with each
//! rule's concern and options) so gdsuite can check that no two tools claim
//! the same concern.

mod checks;
mod comments;
mod limits;
mod naming;
mod order;
mod returns;

use std::ops::Range;

use gdsyntax::SyntaxKind::ANNOTATION_NODE;
use gdsyntax::{SyntaxNode, SyntaxToken};

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
    /// The file's name, such as `player.gd`.
    pub file_name: &'a str,
}

/// A problem a rule found, as a byte range of the source.
#[derive(Debug, Clone)]
pub struct Finding {
    pub range: Range<usize>,
    pub message: String,
    /// How to repair it, for `--fix`. Only rules marked `fixable` give one,
    /// and only when the repair is safe in that spot.
    pub fix: Option<Fix>,
}

impl Finding {
    pub fn new(range: Range<usize>, message: impl Into<String>) -> Self {
        Finding {
            range,
            message: message.into(),
            fix: None,
        }
    }
}

/// Replacements in the source that repair one finding. A fix may remove
/// code tokens and change trivia; `--fix` checks that it does nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub edits: Vec<Edit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

pub type Check = fn(&Context, &mut Vec<Finding>);

/// The type and default of one rule option.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "type", content = "default", rename_all = "lowercase")]
#[allow(dead_code)] // No rule has a plain string option yet.
pub enum OptionKind {
    Integer(i64),
    String(&'static str),
    /// A regular expression that must match the whole text, as in gdlint.
    Pattern(&'static str),
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

pub const RULES: &[Rule] = &[
    naming::FUNCTION_NAME,
    naming::CLASS_NAME,
    naming::SUB_CLASS_NAME,
    naming::SIGNAL_NAME,
    naming::CLASS_VARIABLE_NAME,
    naming::CLASS_LOAD_VARIABLE_NAME,
    naming::FUNCTION_VARIABLE_NAME,
    naming::FUNCTION_PRELOAD_VARIABLE_NAME,
    naming::FUNCTION_ARGUMENT_NAME,
    naming::LOOP_VARIABLE_NAME,
    naming::ENUM_NAME,
    naming::ENUM_ELEMENT_NAME,
    naming::CONSTANT_NAME,
    naming::LOAD_CONSTANT_NAME,
    naming::FILE_NAME,
    order::CLASS_DEFINITIONS_ORDER,
    limits::MAX_RETURNS,
    limits::FUNCTION_ARGUMENTS_NUMBER,
    limits::MAX_PUBLIC_METHODS,
    limits::MAX_FILE_LINES,
    limits::MAX_LINE_LENGTH,
    limits::MAX_FUNCTION_LINES,
    limits::MAX_BRANCHES,
    limits::MAX_NESTING_DEPTH,
    checks::DUPLICATED_LOAD,
    checks::COMPARISON_WITH_ITSELF,
    returns::NO_ELIF_RETURN,
    returns::NO_ELSE_RETURN,
    checks::UNNECESSARY_PASS,
    checks::SELF_ASSIGNMENT,
    checks::LOOP_VARIABLE_ASSIGNMENT,
    comments::DOC_TAG,
    comments::COMMENT_SPACE,
    checks::PRIVATE_CALL,
    UNKNOWN_RULE_IN_COMMENT,
];

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
pub const PLANNED: &[&str] = &[];

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

/// The byte range of a token.
pub fn token_range(token: &SyntaxToken) -> Range<usize> {
    usize::from(token.text_range().start())..usize::from(token.text_range().end())
}

/// Where a declaration or statement is reported: its first token after its
/// annotations, such as `var` in `@export var x`. Annotations can be on the
/// lines above, and the problem is with what follows them.
pub fn start_after_annotations(node: &SyntaxNode) -> Range<usize> {
    let first =
        node.children_with_tokens()
            .filter(|e| e.kind() != ANNOTATION_NODE)
            .find_map(|e| match e {
                gdsyntax::rowan::NodeOrToken::Token(t) => {
                    gdsyntax::ast::is_code(&t).then(|| token_range(&t))
                }
                gdsyntax::rowan::NodeOrToken::Node(n) => gdsyntax::ast::code_range(&n)
                    .map(|r| usize::from(r.start())..usize::from(r.end())),
            });
    first.unwrap_or_else(|| {
        let range = node.text_range();
        usize::from(range.start())..usize::from(range.end())
    })
}

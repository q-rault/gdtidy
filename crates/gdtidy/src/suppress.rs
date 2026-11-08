//! Comments that silence rules.
//!
//! gdtidy reads its own comments and those of gdlint and GDQuest's linter,
//! so a project switching to gdtidy needs no edits:
//!
//! ```gdscript
//! x = 1  # gdtidy: ignore=rule-a, rule-b   (this line and the next)
//! # gdlint: ignore=rule-a                  (the same, gdlint's spelling)
//! # gdtidy: disable=rule-a                 (until enable, or the end of the file)
//! # gdtidy: enable=rule-a
//! x = 1  # gdlint-ignore rule-a            (GDQuest: this line)
//! # gdlint-ignore-next-line rule-a         (GDQuest: the next line)
//! # gdlint-ignore-next-line                (GDQuest: every rule, next line)
//! ```
//!
//! `ignore` covers its own line and the next, and a `disable` written after
//! code starts on the next line, as in gdlint. Comments are found as tokens,
//! so the same text inside a string never counts.

use std::collections::HashMap;
use std::ops::{Range, RangeInclusive};

use gdsyntax::{SyntaxKind, SyntaxTree};

use crate::rules;

/// Which lines each rule is silenced on in one file.
#[derive(Debug, Default)]
pub struct Suppressions {
    by_rule: HashMap<String, Vec<RangeInclusive<usize>>>,
    /// Lines on which every rule is silenced.
    all: Vec<usize>,
    /// Rule names in comments that gdtidy does not know: byte range of the
    /// comment, and the name.
    pub unknown: Vec<(Range<usize>, String)>,
}

impl Suppressions {
    pub fn is_suppressed(&self, rule: &str, line: usize) -> bool {
        self.all.contains(&line)
            || self
                .by_rule
                .get(rule)
                .is_some_and(|ranges| ranges.iter().any(|r| r.contains(&line)))
    }

    pub fn read(tree: &SyntaxTree) -> Self {
        let mut found = Suppressions::default();
        // (rule, line) of each disable, then each enable, to pair up after.
        let mut disables: Vec<(String, usize)> = Vec::new();
        let mut enables: Vec<(String, usize)> = Vec::new();
        let comments = tree
            .root()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == SyntaxKind::COMMENT);
        for comment in comments {
            let range =
                usize::from(comment.text_range().start())..usize::from(comment.text_range().end());
            let line = tree.position(range.start).line;
            let Some(directive) = Directive::parse(comment.text()) else {
                continue;
            };
            for name in &directive.rules {
                if !rules::known_in_comment(name) {
                    found.unknown.push((range.clone(), name.clone()));
                }
            }
            // GDQuest's names stand for gdtidy's rules.
            let directive = Directive {
                rules: directive
                    .rules
                    .iter()
                    .flat_map(|name| match rules::resolve_alias(name) {
                        [] => vec![name.clone()],
                        ids => ids.iter().map(|id| id.to_string()).collect(),
                    })
                    .collect(),
                ..directive
            };
            match directive.action {
                Action::Ignore => found.add(&directive.rules, line..=line + 1),
                Action::IgnoreLine => found.add(&directive.rules, line..=line),
                Action::IgnoreNextLine => found.add(&directive.rules, line + 1..=line + 1),
                Action::Disable => {
                    let own_line = tree
                        .line_text(line)
                        .is_some_and(|text| text.trim_start().starts_with('#'));
                    let from = if own_line { line } else { line + 1 };
                    disables.extend(directive.rules.iter().map(|r| (r.clone(), from)));
                }
                Action::Enable => {
                    enables.extend(directive.rules.iter().map(|r| (r.clone(), line)));
                }
            }
        }
        for (rule, from) in disables {
            let until = enables
                .iter()
                .filter(|(r, at)| *r == rule && *at >= from)
                .map(|(_, at)| *at)
                .min()
                .unwrap_or(usize::MAX);
            found.by_rule.entry(rule).or_default().push(from..=until);
        }
        found
    }

    /// Silence `rules` on `lines`; no rules means every rule.
    fn add(&mut self, rules: &[String], lines: RangeInclusive<usize>) {
        if rules.is_empty() {
            self.all.extend(lines);
            return;
        }
        for rule in rules {
            self.by_rule
                .entry(rule.clone())
                .or_default()
                .push(lines.clone());
        }
    }
}

#[derive(Debug, PartialEq)]
enum Action {
    /// gdtidy's or gdlint's `ignore=`: this line and the next.
    Ignore,
    /// GDQuest's `gdlint-ignore`: this line.
    IgnoreLine,
    /// GDQuest's `gdlint-ignore-next-line`: the next line.
    IgnoreNextLine,
    Disable,
    Enable,
}

#[derive(Debug, PartialEq)]
struct Directive {
    action: Action,
    rules: Vec<String>,
}

impl Directive {
    fn parse(comment: &str) -> Option<Self> {
        let text = comment.strip_prefix('#')?.trim();
        if let Some(rest) = text
            .strip_prefix("gdtidy")
            .or_else(|| text.strip_prefix("gdlint"))
        {
            if let Some(rest) = rest.trim_start().strip_prefix(':') {
                let rest = rest.trim_start();
                let (action, rest) = [
                    ("ignore", Action::Ignore),
                    ("disable", Action::Disable),
                    ("enable", Action::Enable),
                ]
                .into_iter()
                .find_map(|(word, action)| rest.strip_prefix(word).map(|r| (action, r)))?;
                let list = rest.trim_start().strip_prefix('=')?;
                let rules = rule_list(list);
                return (!rules.is_empty()).then_some(Directive { action, rules });
            }
            // GDQuest's spelling: `gdlint-ignore[-next-line] [rules]`.
            if let Some(rest) = rest.strip_prefix("-ignore") {
                let (action, list) = match rest.strip_prefix("-next-line") {
                    Some(list) => (Action::IgnoreNextLine, list),
                    None => (Action::IgnoreLine, rest),
                };
                if !list.is_empty() && !list.starts_with(char::is_whitespace) {
                    return None;
                }
                return Some(Directive {
                    action,
                    rules: rule_list(list),
                });
            }
        }
        None
    }
}

/// `a, b,c` → `["a", "b", "c"]`. Anything after a name's first word (such
/// as a reason) is ignored.
fn rule_list(text: &str) -> Vec<String> {
    text.split(',')
        .filter_map(|item| item.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The action and the rules joined with commas.
    fn directive(text: &str) -> Option<(Action, String)> {
        Directive::parse(text).map(|d| (d.action, d.rules.join(",")))
    }

    #[test]
    fn spellings() {
        let expect = |action, rules: &str| Some((action, rules.to_string()));
        assert_eq!(
            directive("# gdtidy: ignore=a, b"),
            expect(Action::Ignore, "a,b")
        );
        assert_eq!(directive("#gdlint:ignore=a"), expect(Action::Ignore, "a"));
        assert_eq!(
            directive("# gdlint : disable = a,b"),
            expect(Action::Disable, "a,b")
        );
        assert_eq!(directive("# gdtidy: enable=a"), expect(Action::Enable, "a"));
        assert_eq!(
            directive("# gdlint-ignore a"),
            expect(Action::IgnoreLine, "a")
        );
        assert_eq!(
            directive("# gdlint-ignore-next-line a,b"),
            expect(Action::IgnoreNextLine, "a,b")
        );
        assert_eq!(
            directive("# gdlint-ignore-next-line"),
            expect(Action::IgnoreNextLine, "")
        );
        assert_eq!(directive("# gdtidy: ignore="), None);
        assert_eq!(directive("# gdlint-ignored a"), None);
        assert_eq!(directive("# just a comment"), None);
    }

    fn suppressed(source: &str, rule: &str) -> Vec<usize> {
        let tree = gdsyntax::parse(source);
        let s = Suppressions::read(&tree);
        (1..=tree.line_count())
            .filter(|&line| s.is_suppressed(rule, line))
            .collect()
    }

    #[test]
    fn ranges() {
        let src = "var a = 1  # gdtidy: ignore=r\nvar b = 2\nvar c = 3\n";
        assert_eq!(suppressed(src, "r"), [1, 2]);
        let src = "var a = 1\n# gdtidy: disable=r\nvar b = 2\n# gdtidy: enable=r\nvar c = 3\n";
        assert_eq!(suppressed(src, "r"), [2, 3, 4]);
        let src = "var a = 1  # gdtidy: disable=r\nvar b = 2\n";
        assert_eq!(suppressed(src, "r"), [2]);
        let src = "var a = 1  # gdlint-ignore r\nvar b = 2\n";
        assert_eq!(suppressed(src, "r"), [1]);
        let src = "# gdlint-ignore-next-line\nvar b = 2\n";
        assert_eq!(suppressed(src, "anything"), [2]);
    }

    #[test]
    fn text_in_strings_does_not_count() {
        let src = "var a = \"# gdtidy: ignore=r\"\nvar b = 2\n";
        assert!(suppressed(src, "r").is_empty());
    }

    #[test]
    fn unknown_names_are_collected_and_moved_ones_accepted() {
        let tree = gdsyntax::parse("# gdlint: ignore=unused-argument, nope\nfunc f(x):\n\tpass\n");
        let s = Suppressions::read(&tree);
        let names: Vec<_> = s.unknown.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, ["nope"]);
    }

    #[test]
    fn planned_rules_and_gdquest_names_are_accepted() {
        let tree = gdsyntax::parse(
            "# gdlint-ignore-next-line private-access, max-returns, variable-name\nvar x = 1\n",
        );
        let s = Suppressions::read(&tree);
        assert!(s.unknown.is_empty());
        assert!(s.is_suppressed("private-call", 2));
        assert!(s.is_suppressed("class-variable-name", 2));
        assert!(s.is_suppressed("max-returns", 2));
    }
}

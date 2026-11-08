//! Size limits: lines, functions, files.

use super::{Context, Finding, OptionKind, Rule, RuleOption};

pub const MAX_LINE_LENGTH: Rule = Rule {
    id: "max-line-length",
    concern: "limit:line-length",
    summary: "A line is longer than the limit. Tabs count as `tab-width` columns.",
    default_enabled: true,
    fixable: false,
    options: &[
        RuleOption {
            name: "max",
            kind: OptionKind::Integer(100),
            description: "The longest line allowed, in columns.",
        },
        RuleOption {
            name: "tab-width",
            kind: OptionKind::Integer(4),
            description: "How many columns a tab counts for. gdlint counts 1.",
        },
    ],
    check: Some(max_line_length),
};

fn max_line_length(cx: &Context, out: &mut Vec<Finding>) {
    let max = cx.options.integer("max").max(0) as usize;
    let tab_width = cx.options.integer("tab-width").max(0) as usize;
    for (line, text) in cx.tree.lines() {
        let width: usize = text
            .chars()
            .map(|c| if c == '\t' { tab_width } else { 1 })
            .sum();
        if width > max {
            out.push(Finding {
                range: cx.tree.line_range(line).unwrap_or(0..0),
                message: format!("Line is {width} columns long; the limit is {max}."),
            });
        }
    }
}

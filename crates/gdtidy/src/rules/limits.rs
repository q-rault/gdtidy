//! Size limits: lines, files, functions, classes.
//!
//! gdlint's limits keep its IDs, defaults and messages. One deliberate
//! difference: gdlint leaves static functions out of `max-returns`,
//! `function-arguments-number` and `max-public-methods`; gdtidy counts them
//! like any other function.

use std::collections::BTreeSet;

use gdsyntax::SyntaxKind::{
    CLASS_BODY, ELIF_CLAUSE, ELSE_CLAUSE, FUNC, IF_STMT, LAMBDA, MATCH_BRANCH, RETURN_STMT,
    SOURCE_FILE,
};
use gdsyntax::ast::{self, AstNode, Stmt};
use gdsyntax::rowan::WalkEvent;
use gdsyntax::{SyntaxNode, SyntaxToken};

use super::{Context, Finding, OptionKind, Rule, RuleOption, start_after_annotations, token_range};

/// A rule whose one option is `max`.
macro_rules! limit_rule {
    ($const:ident, $id:literal, $concern:literal, $summary:literal, $on:literal, $max:literal, $what:literal, $check:ident) => {
        pub const $const: Rule = Rule {
            id: $id,
            concern: $concern,
            summary: $summary,
            default_enabled: $on,
            fixable: false,
            options: &[RuleOption {
                name: "max",
                kind: OptionKind::Integer($max),
                description: $what,
            }],
            check: Some($check),
        };
    };
}

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

limit_rule!(
    MAX_FILE_LINES,
    "max-file-lines",
    "limit:file-lines",
    "A file has more lines than the limit.",
    true,
    1000,
    "The most lines a file may have.",
    max_file_lines
);
limit_rule!(
    MAX_RETURNS,
    "max-returns",
    "limit:function-returns",
    "A function has more `return` statements than the limit.",
    true,
    6,
    "The most `return` statements a function may have, not counting lambdas in it.",
    max_returns
);
limit_rule!(
    FUNCTION_ARGUMENTS_NUMBER,
    "function-arguments-number",
    "limit:function-parameters",
    "A function has more parameters than the limit.",
    true,
    10,
    "The most parameters a function may have.",
    function_arguments_number
);
limit_rule!(
    MAX_PUBLIC_METHODS,
    "max-public-methods",
    "limit:class-public-methods",
    "A class has more public methods than the limit. Inner classes count apart.",
    true,
    20,
    "The most public methods (not starting with `_`) a class may have.",
    max_public_methods
);
limit_rule!(
    MAX_FUNCTION_LINES,
    "max-function-lines",
    "limit:function-lines",
    "A function body has more lines of code than the limit.",
    false,
    50,
    "The most lines of code a function body may have, not counting blank and comment lines.",
    max_function_lines
);
limit_rule!(
    MAX_BRANCHES,
    "max-branches",
    "limit:function-branches",
    "A function has more branches than the limit.",
    false,
    8,
    "The most branches (each `if`, `elif`, `else` and match branch) a function may have.",
    max_branches
);
limit_rule!(
    MAX_NESTING_DEPTH,
    "max-nesting-depth",
    "limit:nesting-depth",
    "Blocks in a function are nested deeper than the limit.",
    false,
    4,
    "The deepest blocks may be nested in a function; the function's own body is 0.",
    max_nesting_depth
);

fn limit(cx: &Context) -> usize {
    cx.options.integer("max").max(0) as usize
}

fn max_line_length(cx: &Context, out: &mut Vec<Finding>) {
    let max = limit(cx);
    let tab_width = cx.options.integer("tab-width").max(0) as usize;
    for (line, text) in cx.tree.lines() {
        let width: usize = text
            .chars()
            .map(|c| if c == '\t' { tab_width } else { 1 })
            .sum();
        if width > max {
            out.push(Finding {
                fix: None,
                range: cx.tree.line_range(line).unwrap_or(0..0),
                message: format!("Line is {width} columns long; the limit is {max}."),
            });
        }
    }
}

fn max_file_lines(cx: &Context, out: &mut Vec<Finding>) {
    let max = limit(cx);
    let lines = cx.tree.line_count();
    if lines > max {
        out.push(Finding {
            fix: None,
            range: cx.tree.line_range(lines).unwrap_or(0..0),
            message: format!("The file has {lines} lines; the limit is {max}."),
        });
    }
}

/// Every function, with its name token.
fn functions(cx: &Context) -> impl Iterator<Item = (ast::Func, SyntaxToken)> {
    cx.tree
        .root()
        .descendants()
        .filter_map(ast::Func::cast)
        .filter_map(|f| f.name().map(|name| (f, name)))
}

/// The nodes in a function's body, without those inside lambdas: a lambda's
/// statements are not the function's.
fn body_nodes(func: &ast::Func) -> Vec<SyntaxNode> {
    let Some(body) = func.body() else {
        return Vec::new();
    };
    let mut nodes = Vec::new();
    let mut preorder = body.syntax().preorder();
    while let Some(event) = preorder.next() {
        if let WalkEvent::Enter(node) = event {
            if node.kind() == LAMBDA {
                preorder.skip_subtree();
                continue;
            }
            nodes.push(node);
        }
    }
    nodes
}

fn max_returns(cx: &Context, out: &mut Vec<Finding>) {
    let max = limit(cx);
    for (func, name) in functions(cx) {
        let returns: Vec<_> = body_nodes(&func)
            .into_iter()
            .filter(|n| n.kind() == RETURN_STMT)
            .collect();
        if returns.len() > max {
            let last = returns.last().expect("more than zero returns");
            out.push(Finding {
                fix: None,
                range: start_after_annotations(last),
                message: format!(
                    "Function \"{}\" has more than {max} return statements ({})",
                    name.text(),
                    returns.len()
                ),
            });
        }
    }
}

fn function_arguments_number(cx: &Context, out: &mut Vec<Finding>) {
    let max = limit(cx);
    for (func, name) in functions(cx) {
        let count = func.params().map_or(0, |p| p.params().count());
        if count > max {
            out.push(Finding {
                fix: None,
                range: token_range(&name),
                message: format!(
                    "Function \"{}\" has more than {max} arguments ({count})",
                    name.text()
                ),
            });
        }
    }
}

fn max_public_methods(cx: &Context, out: &mut Vec<Finding>) {
    let max = limit(cx);
    for scope in cx.tree.root().descendants() {
        let (label, range) = match scope.kind() {
            SOURCE_FILE => ("Class global scope".to_string(), 0..0),
            CLASS_BODY => {
                let Some(name) = scope
                    .parent()
                    .and_then(ast::Class::cast)
                    .and_then(|c| c.name())
                else {
                    continue;
                };
                (format!("Class {}", name.text()), token_range(&name))
            }
            _ => continue,
        };
        let public = scope
            .children()
            .filter(|n| n.kind() == FUNC)
            .filter_map(|n| ast::Func::cast(n)?.name())
            .filter(|name| !name.text().starts_with('_'))
            .count();
        if public > max {
            out.push(Finding {
                fix: None,
                range,
                message: format!(
                    "\"{label}\" has more than {max} public methods (functions) ({public})"
                ),
            });
        }
    }
}

fn max_function_lines(cx: &Context, out: &mut Vec<Finding>) {
    let max = limit(cx);
    for (func, name) in functions(cx) {
        let Some(body) = func.body() else { continue };
        let lines: BTreeSet<usize> = body
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(ast::is_code)
            .map(|t| cx.tree.position(usize::from(t.text_range().start())).line)
            .collect();
        if lines.len() > max {
            out.push(Finding {
                fix: None,
                range: token_range(&name),
                message: format!(
                    "Function \"{}\" has {} lines of code; the limit is {max}.",
                    name.text(),
                    lines.len()
                ),
            });
        }
    }
}

fn max_branches(cx: &Context, out: &mut Vec<Finding>) {
    let max = limit(cx);
    for (func, name) in functions(cx) {
        let branches = body_nodes(&func)
            .iter()
            .filter(|n| matches!(n.kind(), IF_STMT | ELIF_CLAUSE | ELSE_CLAUSE | MATCH_BRANCH))
            .count();
        if branches > max {
            out.push(Finding {
                fix: None,
                range: token_range(&name),
                message: format!(
                    "Function \"{}\" has {branches} branches; the limit is {max}.",
                    name.text()
                ),
            });
        }
    }
}

fn max_nesting_depth(cx: &Context, out: &mut Vec<Finding>) {
    let max = limit(cx);
    for (func, _) in functions(cx) {
        if let Some(body) = func.body() {
            nesting(&body, 0, max, out);
        }
    }
}

/// The blocks a statement opens.
fn blocks(stmt: &Stmt) -> Vec<ast::Suite> {
    match stmt {
        Stmt::If(s) => s
            .body()
            .into_iter()
            .chain(s.elif_clauses().filter_map(|c| c.body()))
            .chain(s.else_clause().and_then(|c| c.body()))
            .collect(),
        Stmt::For(s) => s.body().into_iter().collect(),
        Stmt::While(s) => s.body().into_iter().collect(),
        // A match and its branches count as one level, as in an if/elif.
        Stmt::Match(s) => s.branches().filter_map(|b| b.body()).collect(),
        _ => Vec::new(),
    }
}

/// Report the statements in `suite` (at `depth`) whose blocks go past
/// `max`, once each: what is nested inside one is not reported again.
fn nesting(suite: &ast::Suite, depth: usize, max: usize, out: &mut Vec<Finding>) {
    for stmt in suite.statements() {
        let inner = blocks(&stmt);
        if inner.is_empty() {
            continue;
        }
        if depth + 1 > max {
            out.push(Finding {
                fix: None,
                range: start_after_annotations(stmt.syntax()),
                message: format!(
                    "This block is nested {} deep; the limit is {max}.",
                    depth + 1
                ),
            });
            continue;
        }
        for block in inner {
            nesting(&block, depth + 1, max, out);
        }
    }
}

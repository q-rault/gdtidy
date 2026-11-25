//! `no-elif-return` and `no-else-return`, as in gdlint.
//!
//! A block "always returns" when one of its own statements is a `return`,
//! an `if` with an `else` whose branches all always return, or a `match`
//! with a `_` branch whose branches all always return. As in gdlint, the
//! `return` does not have to be last.

use std::collections::HashSet;

use gdsyntax::SyntaxKind::{ANNOTATION_NODE, COMMENT, IDENTIFIER, LAMBDA};
use gdsyntax::ast::{self, AstNode, Pattern, Stmt};

use super::{Context, Edit, Finding, Fix, Rule};

pub const NO_ELIF_RETURN: Rule = Rule {
    id: "no-elif-return",
    concern: "control-flow:elif-after-return",
    summary: "`elif` after a branch that always returns; it can be a plain `if`.",
    default_enabled: true,
    fixable: true,
    options: &[],
    check: Some(no_elif_return),
};

pub const NO_ELSE_RETURN: Rule = Rule {
    id: "no-else-return",
    concern: "control-flow:else-after-return",
    summary: "`else` after branches that all return; its body can follow the `if`.",
    default_enabled: true,
    fixable: true,
    options: &[],
    check: Some(no_else_return),
};

fn always_returns(block: Option<ast::Suite>) -> bool {
    let Some(block) = block else { return false };
    block.statements().any(|stmt| match stmt {
        Stmt::Return(_) => true,
        Stmt::If(s) => {
            s.else_clause().is_some()
                && always_returns(s.body())
                && s.elif_clauses().all(|c| always_returns(c.body()))
                && always_returns(s.else_clause().and_then(|c| c.body()))
        }
        Stmt::Match(s) => {
            let branches: Vec<_> = s.branches().collect();
            let has_wildcard = branches.iter().any(|b| {
                !b.has_guard() && matches!(b.patterns().next(), Some(Pattern::Wildcard(_)))
            });
            has_wildcard && branches.iter().all(|b| always_returns(b.body()))
        }
        _ => false,
    })
}

fn keyword_range(node: &gdsyntax::SyntaxNode) -> std::ops::Range<usize> {
    let token = ast::code_tokens(node).next();
    let range = token.map_or(node.text_range(), |t| t.text_range());
    usize::from(range.start())..usize::from(range.end())
}

fn no_elif_return(cx: &Context, out: &mut Vec<Finding>) {
    for if_stmt in cx.tree.root().descendants().filter_map(ast::IfStmt::cast) {
        let mut previous = if_stmt.body();
        for elif in if_stmt.elif_clauses() {
            if !always_returns(previous) {
                break;
            }
            let range = keyword_range(elif.syntax());
            out.push(Finding {
                range: range.clone(),
                message: "Unnecessary \"elif\" after \"return\"".to_string(),
                fix: (!annotated(&if_stmt)).then(|| Fix {
                    edits: vec![Edit {
                        range,
                        text: "if".to_string(),
                    }],
                }),
            });
            previous = elif.body();
        }
    }
}

/// The names of the variables a block declares itself.
fn declared(block: Option<ast::Suite>) -> HashSet<String> {
    block
        .into_iter()
        .flat_map(|b| b.statements().collect::<Vec<_>>())
        .filter_map(|stmt| match stmt {
            Stmt::Var(var) => var.name().map(|t| t.text().to_string()),
            _ => None,
        })
        .collect()
}

fn no_else_return(cx: &Context, out: &mut Vec<Finding>) {
    for if_stmt in cx.tree.root().descendants().filter_map(ast::IfStmt::cast) {
        let Some(else_clause) = if_stmt.else_clause() else {
            continue;
        };
        let returns = always_returns(if_stmt.body())
            && if_stmt.elif_clauses().all(|c| always_returns(c.body()));
        if !returns {
            continue;
        }
        // Moving the else body out would declare its variables a second
        // time in the enclosing block.
        let enclosing = if_stmt.syntax().parent().and_then(ast::Suite::cast);
        let outer = declared(enclosing);
        if declared(else_clause.body())
            .iter()
            .any(|name| outer.contains(name))
        {
            continue;
        }
        out.push(Finding {
            range: keyword_range(else_clause.syntax()),
            message: "Unnecessary \"else\" after \"return\"".to_string(),
            fix: unnest(cx, &if_stmt, &else_clause),
        });
    }
}

/// Whether an annotation, such as `@warning_ignore`, applies to the whole
/// `if`: what moves out of it would no longer be covered.
fn annotated(if_stmt: &ast::IfStmt) -> bool {
    let node = if_stmt.syntax();
    node.children().any(|n| n.kind() == ANNOTATION_NODE)
        || node
            .prev_sibling()
            .is_some_and(|n| n.kind() == ANNOTATION_NODE)
}

/// Removes the `else:` and unindents its body, or `None` where that could
/// change what the code does.
fn unnest(cx: &Context, if_stmt: &ast::IfStmt, else_clause: &ast::ElseClause) -> Option<Fix> {
    let enclosing = if_stmt.syntax().parent().and_then(ast::Suite::cast)?;
    let in_lambda = if_stmt.syntax().ancestors().any(|n| n.kind() == LAMBDA);
    if in_lambda || annotated(if_stmt) {
        return None;
    }
    let body = else_clause.body()?;
    // The variables the body declares become the enclosing block's: none of
    // its other code may use their names, for a member or another variable.
    let mut names = declared(Some(body.clone()));
    names.extend(body.statements().filter_map(|stmt| match stmt {
        Stmt::Const(constant) => constant.name().map(|t| t.text().to_string()),
        _ => None,
    }));
    let moved = else_clause.syntax().text_range();
    let used_elsewhere = enclosing
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .any(|t| {
            t.kind() == IDENTIFIER
                && !moved.contains_range(t.text_range())
                && names.contains(t.text())
        });
    if used_elsewhere {
        return None;
    }

    let source = cx.tree.source();
    let keyword = keyword_range(else_clause.syntax());
    let else_line = cx.tree.position(keyword.start).line;
    let indent = &source[cx.tree.line_range(else_line)?.start..keyword.start];
    if !indent.trim().is_empty() {
        return None;
    }
    let content = ast::code_range(body.syntax())?;
    let start = usize::from(content.start());
    let end = usize::from(content.end());
    let first = cx.tree.position(start).line;
    if first == else_line {
        // `else: return 2`
        return Some(Fix {
            edits: vec![Edit {
                range: keyword.start..start,
                text: String::new(),
            }],
        });
    }

    let after_else = &source[keyword.end..cx.tree.line_range(else_line)?.end];
    let after_colon = after_else.trim_start().strip_prefix(':')?.trim_start();
    let mut edits = vec![if after_colon.is_empty() {
        Edit {
            range: cx.tree.line_range(else_line)?.start..cx.tree.line_range(else_line + 1)?.start,
            text: String::new(),
        }
    } else {
        // Its comment stays, on a line of its own.
        Edit {
            range: keyword.start..keyword.end + (after_else.len() - after_colon.len()),
            text: String::new(),
        }
    }];

    let body_indent = &source[cx.tree.line_range(first)?.start..start];
    if !body_indent.trim().is_empty() || !body_indent.starts_with(indent) {
        return None;
    }
    // The lines that start within a string: their indent is its text.
    let mut in_string = HashSet::new();
    for token in ast::code_tokens(body.syntax()) {
        let from = cx.tree.position(token.text_range().start().into()).line;
        let to = cx.tree.position(token.text_range().end().into()).line;
        in_string.extend(from + 1..=to);
    }
    let mut last = cx.tree.position(end).line;
    // The comments that end the body.
    while let Some(text) = cx.tree.line_text(last + 1)
        && text.starts_with(body_indent)
        && text.trim_start().starts_with('#')
    {
        last += 1;
    }
    for line in else_line + 1..=last {
        let text = cx.tree.line_text(line)?;
        if in_string.contains(&line) || text.trim().is_empty() {
            continue;
        }
        let line_start = cx.tree.line_range(line)?.start;
        if text.starts_with(body_indent) {
            edits.push(Edit {
                range: line_start + indent.len()..line_start + body_indent.len(),
                text: String::new(),
            });
        } else if !is_comment_line(cx, line_start, text) {
            return None;
        }
    }
    Some(Fix { edits })
}

/// Whether all the line at `start` holds is a comment.
fn is_comment_line(cx: &Context, start: usize, text: &str) -> bool {
    let at = start + (text.len() - text.trim_start().len());
    cx.tree
        .root()
        .token_at_offset(gdsyntax::rowan::TextSize::new(at as u32))
        .right_biased()
        .is_some_and(|t| t.kind() == COMMENT)
}

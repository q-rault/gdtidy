//! `no-elif-return` and `no-else-return`, as in gdlint.
//!
//! A block "always returns" when one of its own statements is a `return`,
//! an `if` with an `else` whose branches all always return, or a `match`
//! with a `_` branch whose branches all always return. As in gdlint, the
//! `return` does not have to be last.

use std::collections::HashSet;

use gdsyntax::ast::{self, AstNode, Pattern, Stmt};

use super::{Context, Finding, Rule};

pub const NO_ELIF_RETURN: Rule = Rule {
    id: "no-elif-return",
    concern: "control-flow:elif-after-return",
    summary: "`elif` after a branch that always returns; it can be a plain `if`.",
    default_enabled: true,
    fixable: false,
    options: &[],
    check: Some(no_elif_return),
};

pub const NO_ELSE_RETURN: Rule = Rule {
    id: "no-else-return",
    concern: "control-flow:else-after-return",
    summary: "`else` after branches that all return; its body can follow the `if`.",
    default_enabled: true,
    fixable: false,
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
            out.push(Finding::new(
                keyword_range(elif.syntax()),
                "Unnecessary \"elif\" after \"return\"",
            ));
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
        out.push(Finding::new(
            keyword_range(else_clause.syntax()),
            "Unnecessary \"else\" after \"return\"",
        ));
    }
}

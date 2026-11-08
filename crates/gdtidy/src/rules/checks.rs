//! Rules about single expressions and statements: gdlint's
//! `duplicated-load`, `comparison-with-itself` and `unnecessary-pass`, and
//! `self-assignment`, `loop-variable-assignment` and `private-call`.

use std::collections::HashSet;

use gdsyntax::SyntaxKind::{
    ANNOTATION_NODE, CLASS_BODY, ERROR, LAMBDA, PASS_STMT, SOURCE_FILE, SUITE,
};
use gdsyntax::ast::{self, AstNode, BinaryOp, Expr};
use gdsyntax::{SyntaxNode, SyntaxToken};

use super::{Context, Edit, Finding, Fix, Rule, token_range};

macro_rules! simple_rule {
    ($const:ident, $id:literal, $concern:literal, $summary:literal, $on:literal, $fixable:literal, $check:ident) => {
        pub const $const: Rule = Rule {
            id: $id,
            concern: $concern,
            summary: $summary,
            default_enabled: $on,
            fixable: $fixable,
            options: &[],
            check: Some($check),
        };
    };
}

simple_rule!(
    DUPLICATED_LOAD,
    "duplicated-load",
    "duplicated-load",
    "The same path is loaded more than once in a file.",
    true,
    false,
    duplicated_load
);
simple_rule!(
    COMPARISON_WITH_ITSELF,
    "comparison-with-itself",
    "comparison-with-itself",
    "Something is compared with itself, as in `x == x`.",
    true,
    false,
    comparison_with_itself
);
simple_rule!(
    UNNECESSARY_PASS,
    "unnecessary-pass",
    "unnecessary-pass",
    "`pass` in a block that has other statements.",
    true,
    true,
    unnecessary_pass
);
simple_rule!(
    SELF_ASSIGNMENT,
    "self-assignment",
    "self-assignment",
    "Something is assigned to itself, as in `x = x`.",
    true,
    false,
    self_assignment
);
simple_rule!(
    LOOP_VARIABLE_ASSIGNMENT,
    "loop-variable-assignment",
    "loop-variable-assignment",
    "A `for` loop's variable is assigned in the loop, which does not change the iteration.",
    true,
    false,
    loop_variable_assignment
);
simple_rule!(
    PRIVATE_CALL,
    "private-call",
    "private-call",
    "A method starting with `_` is called on something other than `self` or `super`.",
    false,
    false,
    private_call
);

fn all<N: AstNode>(cx: &Context) -> impl Iterator<Item = N> {
    cx.tree.root().descendants().filter_map(N::cast)
}

fn node_range(node: &SyntaxNode) -> std::ops::Range<usize> {
    let range = ast::code_range(node).unwrap_or(node.text_range());
    usize::from(range.start())..usize::from(range.end())
}

/// The code of an expression, as (kind, text) pairs: two expressions with
/// the same code are the same expression, however they are spaced.
fn code(node: &SyntaxNode) -> Vec<(gdsyntax::SyntaxKind, String)> {
    ast::code_tokens(node)
        .map(|t| (t.kind(), t.text().to_string()))
        .collect()
}

// ---- duplicated-load ----

/// The path loaded by `load("...")` or `preload("...")`, as gdlint finds it:
/// a string literal as the first argument.
fn loaded_path(expr: &Expr) -> Option<(SyntaxToken, String)> {
    let first = match expr {
        Expr::Preload(preload) => preload.path(),
        Expr::Call(call) => match call.callee()? {
            Expr::Name(name) if name.name()?.text() == "load" => call.arguments().next(),
            _ => None,
        },
        _ => None,
    }?;
    let Expr::Literal(literal) = first else {
        return None;
    };
    let token = literal.token()?;
    let Some(gdsyntax::literal::LiteralValue::String(path)) = literal.value() else {
        return None;
    };
    Some((token, path))
}

fn duplicated_load(cx: &Context, out: &mut Vec<Finding>) {
    let mut seen = HashSet::new();
    for expr in cx.tree.root().descendants().filter_map(Expr::cast) {
        if let Some((token, path)) = loaded_path(&expr)
            && !seen.insert(path.clone())
        {
            out.push(Finding::new(
                token_range(&token),
                format!("duplicated loading of {}", token.text()),
            ));
        }
    }
}

// ---- comparison-with-itself ----

fn comparison_with_itself(cx: &Context, out: &mut Vec<Finding>) {
    for binary in all::<ast::BinaryExpr>(cx) {
        let comparison = matches!(
            binary.op(),
            Some(
                BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
            )
        );
        let (Some(lhs), Some(rhs)) = (binary.lhs(), binary.rhs()) else {
            continue;
        };
        if comparison && code(lhs.syntax()) == code(rhs.syntax()) {
            out.push(Finding::new(
                node_range(binary.syntax()),
                "Redundant comparison",
            ));
        }
    }
}

// ---- self-assignment ----

fn self_assignment(cx: &Context, out: &mut Vec<Finding>) {
    for assignment in all::<ast::Assignment>(cx) {
        if assignment.op() != Some(ast::AssignOp::Assign) {
            continue;
        }
        let (Some(target), Some(value)) = (assignment.target(), assignment.value()) else {
            continue;
        };
        let target_code = code(target.syntax());
        if target_code == code(value.syntax()) {
            let text: Vec<_> = target_code.iter().map(|(_, t)| t.as_str()).collect();
            out.push(Finding::new(
                node_range(assignment.syntax()),
                format!("`{}` is assigned to itself.", text.concat()),
            ));
        }
    }
}

// ---- loop-variable-assignment ----

fn loop_variable_assignment(cx: &Context, out: &mut Vec<Finding>) {
    for for_stmt in all::<ast::ForStmt>(cx) {
        let (Some(variable), Some(body)) = (for_stmt.variable(), for_stmt.body()) else {
            continue;
        };
        let name = variable.text();
        let mut preorder = body.syntax().preorder();
        while let Some(event) = preorder.next() {
            let gdsyntax::rowan::WalkEvent::Enter(node) = event else {
                continue;
            };
            // A lambda, or an inner loop over the same name, has its own
            // variable.
            let rebinds = ast::ForStmt::cast(node.clone())
                .and_then(|f| f.variable())
                .is_some_and(|v| v.text() == name);
            if node.kind() == LAMBDA || rebinds {
                preorder.skip_subtree();
                continue;
            }
            let Some(assignment) = ast::Assignment::cast(node) else {
                continue;
            };
            if let Some(Expr::Name(target)) = assignment.target()
                && target.name().is_some_and(|t| t.text() == name)
            {
                out.push(Finding::new(
                    node_range(assignment.syntax()),
                    format!(
                        "Assigning to the loop variable `{name}` does not change the iteration."
                    ),
                ));
            }
        }
    }
}

// ---- unnecessary-pass ----

fn unnecessary_pass(cx: &Context, out: &mut Vec<Finding>) {
    for block in cx.tree.root().descendants() {
        if !matches!(block.kind(), SUITE | CLASS_BODY | SOURCE_FILE) {
            continue;
        }
        let items: Vec<SyntaxNode> = block
            .children()
            .filter(|n| is_block_item(block.kind(), n))
            .collect();
        let passes: Vec<&SyntaxNode> = items.iter().filter(|n| n.kind() == PASS_STMT).collect();
        if passes.len() == items.len() {
            continue;
        }
        for pass in passes {
            out.push(Finding {
                range: node_range(pass),
                message: "\"pass\" statement not necessary".to_string(),
                fix: remove_line(cx, pass),
            });
        }
    }
}

/// Whether `node` is a statement or member of a block of `kind`. Annotations
/// on their own line, such as `@warning_ignore_start`, are not.
fn is_block_item(kind: gdsyntax::SyntaxKind, node: &SyntaxNode) -> bool {
    if node.kind() == ANNOTATION_NODE || node.kind() == ERROR {
        return false;
    }
    match kind {
        SUITE => ast::Stmt::cast(node.clone()).is_some(),
        _ => ast::Member::cast(node.clone()).is_some(),
    }
}

/// Delete the line `node` is on, when it is alone there: no `;`, comment or
/// line continuation shares the line.
fn remove_line(cx: &Context, node: &SyntaxNode) -> Option<Fix> {
    let range = node_range(node);
    let line = cx.tree.position(range.start).line;
    if cx.tree.position(range.end).line != line {
        return None;
    }
    let text = cx.tree.line_text(line)?;
    let node_text = &cx.tree.source()[range.clone()];
    if text.trim() != node_text {
        return None;
    }
    let start = cx.tree.line_range(line)?.start;
    // The line and its line break; the last line of a file may have none.
    let end = cx
        .tree
        .line_range(line + 1)
        .map_or(cx.tree.source().len(), |next| next.start);
    Some(Fix {
        edits: vec![Edit {
            range: start..end,
            text: String::new(),
        }],
    })
}

// ---- private-call ----

fn private_call(cx: &Context, out: &mut Vec<Finding>) {
    // Calling a private static method through the class's own name is fine.
    let mut own_names: HashSet<String> = all::<ast::ClassName>(cx)
        .filter_map(|c| c.name())
        .map(|t| t.text().to_string())
        .collect();
    own_names.extend(
        all::<ast::Class>(cx)
            .filter_map(|c| c.name())
            .map(|t| t.text().to_string()),
    );
    for call in all::<ast::CallExpr>(cx) {
        let Some(Expr::Attribute(attribute)) = call.callee() else {
            continue;
        };
        let Some(name) = attribute.name() else {
            continue;
        };
        if !name.text().starts_with('_') {
            continue;
        }
        let allowed = match attribute.base() {
            Some(Expr::SelfExpr(_)) | None => true,
            Some(Expr::Name(base)) => base.name().is_some_and(|t| own_names.contains(t.text())),
            _ => false,
        };
        if !allowed {
            out.push(Finding::new(
                token_range(&name),
                format!("`{}` is private; call it only on `self`.", name.text()),
            ));
        }
    }
}

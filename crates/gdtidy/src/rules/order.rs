//! `class-definitions-order`: gdlint's member order, per class.
//!
//! Each declaration falls in a section, and sections must come in the order
//! the `sections` option lists. Functions, and everything gdlint has no
//! section for, are `others`. Inner classes, `pass` and annotations other
//! than `@tool` are not checked.

use gdsyntax::SyntaxKind::{
    ANNOTATION_NODE, CLASS, CLASS_BODY, CLASS_NAME, CONST, ENUM, EXTENDS, FUNC, SIGNAL,
    SOURCE_FILE, STRING_STATEMENT, VAR,
};
use gdsyntax::ast::{self, AstNode};
use gdsyntax::{SyntaxKind, SyntaxNode};

use super::{Context, Finding, OptionKind, Rule, RuleOption, start_after_annotations};

pub const CLASS_DEFINITIONS_ORDER: Rule = Rule {
    id: "class-definitions-order",
    concern: "member-order",
    summary: "A class member comes after a section that should follow it.",
    default_enabled: true,
    fixable: false,
    options: &[RuleOption {
        name: "sections",
        kind: OptionKind::StringList(&[
            "tools",
            "classnames",
            "extends",
            "docstrings",
            "signals",
            "enums",
            "consts",
            "staticvars",
            "exports",
            "pubvars",
            "prvvars",
            "onreadypubvars",
            "onreadyprvvars",
            "others",
        ]),
        description: "The sections, in the order they must come.",
    }],
    check: Some(class_definitions_order),
};

fn class_definitions_order(cx: &Context, out: &mut Vec<Finding>) {
    let order = cx.options.strings("sections");
    if order.is_empty() {
        return;
    }
    for scope in cx.tree.root().descendants() {
        let (name, items): (String, Vec<SyntaxNode>) = match scope.kind() {
            SOURCE_FILE => ("global scope".into(), scope.children().collect()),
            CLASS_BODY => {
                let Some(class) = scope.parent().and_then(ast::Class::cast) else {
                    continue;
                };
                let name = class
                    .name()
                    .map_or_else(String::new, |t| t.text().to_string());
                // `class A extends B:` keeps its `extends` before the body.
                let header = class.syntax().children().filter(|n| n.kind() == EXTENDS);
                (name, header.chain(scope.children()).collect())
            }
            _ => continue,
        };
        check_scope(&name, &items, order, out);
    }
}

fn check_scope(name: &str, items: &[SyntaxNode], order: &[String], out: &mut Vec<Finding>) {
    let rank = |section: &str| order.iter().position(|s| s == section);
    let mut current = order[0].as_str();
    for item in items {
        let Some(section) = section(item) else {
            continue;
        };
        let range = start_after_annotations(item);
        match (rank(current), rank(section)) {
            (Some(at), Some(here)) if here >= at => current = section,
            (Some(_), Some(_)) => out.push(Finding {
                fix: None,
                range,
                message: format!(
                    "Definition out of order in {name}: `{section}` must come before `{current}`"
                ),
            }),
            _ => out.push(Finding {
                fix: None,
                range,
                message: format!(
                    "Definition order not specified for `{section}` or `{current}`; add it to `sections`"
                ),
            }),
        }
    }
}

/// The section a class-level item belongs to, or `None` if the order does
/// not apply to it.
fn section(node: &SyntaxNode) -> Option<&'static str> {
    let kind: SyntaxKind = node.kind();
    Some(match kind {
        ANNOTATION_NODE => {
            let annotation = ast::Annotation::cast(node.clone())?;
            return (annotation.name()?.text() == "@tool").then_some("tools");
        }
        CLASS_NAME => {
            // gdlint files `class_name A extends B` on one line under extends.
            if node.children().any(|n| n.kind() == EXTENDS) {
                "extends"
            } else {
                "classnames"
            }
        }
        EXTENDS => "extends",
        STRING_STATEMENT => "docstrings",
        SIGNAL => "signals",
        ENUM => "enums",
        CONST => "consts",
        VAR => {
            let var = ast::Var::cast(node.clone())?;
            if var.is_static() {
                return Some("staticvars");
            }
            let names: Vec<String> = var
                .annotations()
                .filter_map(|a| a.name().map(|t| t.text().to_string()))
                .collect();
            let private = var.name().is_some_and(|t| t.text().starts_with('_'));
            if names.iter().any(|n| n.starts_with("@export")) {
                "exports"
            } else if names.iter().any(|n| n == "@onready") {
                if private {
                    "onreadyprvvars"
                } else {
                    "onreadypubvars"
                }
            } else if private {
                "prvvars"
            } else {
                "pubvars"
            }
        }
        FUNC => "others",
        CLASS => return None,
        _ => return None,
    })
}

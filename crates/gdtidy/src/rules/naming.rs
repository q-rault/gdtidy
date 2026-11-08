//! The 14 naming rules, kept from gdlint with its IDs, default patterns and
//! messages. Each checks the names of one kind of declaration against a
//! regular expression that must match the whole name.
//!
//! Which declarations each rule covers follows gdlint, so that switching
//! reports the same names:
//! - variables and constants set directly to `load(...)` or `preload(...)`
//!   have their own rules, except local variables set to `load(...)`, which
//!   gdlint checks with neither rule;
//! - lambda parameters count as function arguments; lambda names, signal
//!   parameters and setter parameters are not checked.
//!
//! One deliberate difference: `class_name Name extends Base` on one line is
//! checked like `class_name Name` alone. gdlint skips it.

use gdsyntax::SyntaxKind::{self, CLASS_BODY, FUNC, LAMBDA, SOURCE_FILE};
use gdsyntax::ast::{self, AstNode, Expr};
use gdsyntax::{SyntaxNode, SyntaxToken};

use super::{Context, Finding, OptionKind, Rule, RuleOption};

// gdlint's building blocks.
const PASCAL_CASE: &str = "([A-Z][a-z0-9]*)+";
const SNAKE_CASE: &str = "[a-z][a-z0-9]*(_[a-z0-9]+)*";
const PRIVATE_SNAKE_CASE: &str = "_?[a-z][a-z0-9]*(_[a-z0-9]+)*";
const UPPER_SNAKE_CASE: &str = "[A-Z][A-Z0-9]*(_[A-Z0-9]+)*";
const PRIVATE_UPPER_SNAKE_CASE: &str = "_?[A-Z][A-Z0-9]*(_[A-Z0-9]+)*";

macro_rules! naming_rule {
    ($const:ident, $id:literal, $concern:literal, $what:literal, $default:expr, $check:ident) => {
        pub const $const: Rule = Rule {
            id: $id,
            concern: $concern,
            summary: concat!("The name of ", $what, " does not match `pattern`."),
            default_enabled: true,
            fixable: false,
            options: &[RuleOption {
                name: "pattern",
                kind: OptionKind::Pattern($default),
                description: "A regular expression the whole name must match.",
            }],
            check: Some($check),
        };
    };
}

naming_rule!(
    FUNCTION_NAME,
    "function-name",
    "naming:function",
    "a function",
    "(_on_([A-Z][a-z0-9]*)+(_[a-z0-9]+)*|_?[a-z][a-z0-9]*(_[a-z0-9]+)*)",
    function_name
);
naming_rule!(
    CLASS_NAME,
    "class-name",
    "naming:class",
    "a script's `class_name`",
    PASCAL_CASE,
    class_name
);
naming_rule!(
    SUB_CLASS_NAME,
    "sub-class-name",
    "naming:inner-class",
    "an inner class",
    "_?([A-Z][a-z0-9]*)+",
    sub_class_name
);
naming_rule!(
    SIGNAL_NAME,
    "signal-name",
    "naming:signal",
    "a signal",
    SNAKE_CASE,
    signal_name
);
naming_rule!(
    CLASS_VARIABLE_NAME,
    "class-variable-name",
    "naming:member-variable",
    "a member variable",
    PRIVATE_SNAKE_CASE,
    class_variable_name
);
naming_rule!(
    CLASS_LOAD_VARIABLE_NAME,
    "class-load-variable-name",
    "naming:member-load-variable",
    "a member variable set to `load()` or `preload()`",
    "(([A-Z][a-z0-9]*)+|_?[a-z][a-z0-9]*(_[a-z0-9]+)*)",
    class_load_variable_name
);
naming_rule!(
    FUNCTION_VARIABLE_NAME,
    "function-variable-name",
    "naming:local-variable",
    "a local variable",
    SNAKE_CASE,
    function_variable_name
);
naming_rule!(
    FUNCTION_PRELOAD_VARIABLE_NAME,
    "function-preload-variable-name",
    "naming:local-preload-variable",
    "a local variable set to `preload()`",
    PASCAL_CASE,
    function_preload_variable_name
);
naming_rule!(
    FUNCTION_ARGUMENT_NAME,
    "function-argument-name",
    "naming:parameter",
    "a function or lambda parameter",
    PRIVATE_SNAKE_CASE,
    function_argument_name
);
naming_rule!(
    LOOP_VARIABLE_NAME,
    "loop-variable-name",
    "naming:loop-variable",
    "a `for` loop variable",
    PRIVATE_SNAKE_CASE,
    loop_variable_name
);
naming_rule!(
    ENUM_NAME,
    "enum-name",
    "naming:enum",
    "an enum",
    PASCAL_CASE,
    enum_name
);
naming_rule!(
    ENUM_ELEMENT_NAME,
    "enum-element-name",
    "naming:enum-value",
    "an enum value",
    UPPER_SNAKE_CASE,
    enum_element_name
);
naming_rule!(
    CONSTANT_NAME,
    "constant-name",
    "naming:constant",
    "a constant",
    PRIVATE_UPPER_SNAKE_CASE,
    constant_name
);
naming_rule!(
    LOAD_CONSTANT_NAME,
    "load-constant-name",
    "naming:load-constant",
    "a constant set to `load()` or `preload()`",
    "(([A-Z][a-z0-9]*)+|_?[A-Z][A-Z0-9]*(_[A-Z0-9]+)*)",
    load_constant_name
);

/// Report each name in `names` that the rule's pattern does not match, with
/// gdlint's message for it.
fn check_names(
    cx: &Context,
    out: &mut Vec<Finding>,
    names: impl Iterator<Item = SyntaxToken>,
    message: &str,
) {
    let pattern = cx.options.pattern("pattern");
    for name in names {
        if !pattern.is_match(name.text()) {
            out.push(Finding {
                fix: None,
                range: usize::from(name.text_range().start())..usize::from(name.text_range().end()),
                message: format!("{message} \"{}\" is not valid", name.text()),
            });
        }
    }
}

fn all<N: AstNode>(cx: &Context) -> impl Iterator<Item = N> {
    cx.tree.root().descendants().filter_map(N::cast)
}

/// What a declaration is set to, as far as the load rules care.
#[derive(PartialEq)]
enum Loads {
    Nothing,
    Load,
    Preload,
}

/// Whether `value` is a direct `load(...)` or `preload(...)` call, as
/// gdlint checks it: the call is the whole value.
fn loads(value: Option<Expr>) -> Loads {
    match value {
        Some(Expr::Preload(_)) => Loads::Preload,
        Some(Expr::Call(call)) => match call.callee() {
            Some(Expr::Name(name)) => match name.name().as_ref().map(|t| t.text()) {
                Some("load") => Loads::Load,
                Some("preload") => Loads::Preload,
                _ => Loads::Nothing,
            },
            _ => Loads::Nothing,
        },
        _ => Loads::Nothing,
    }
}

fn parent_kind(node: &SyntaxNode) -> Option<SyntaxKind> {
    node.parent().map(|p| p.kind())
}

/// Variables declared at class level, with what they are set to.
fn variables(cx: &Context, member: bool) -> impl Iterator<Item = (ast::Var, Loads)> {
    all::<ast::Var>(cx)
        .filter(move |var| {
            matches!(parent_kind(var.syntax()), Some(SOURCE_FILE | CLASS_BODY)) == member
        })
        .map(|var| {
            let loads = loads(var.initializer());
            (var, loads)
        })
}

fn function_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = all::<ast::Func>(cx).filter_map(|f| f.name());
    check_names(cx, out, names, "Function name");
}

fn class_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = all::<ast::ClassName>(cx).filter_map(|c| c.name());
    check_names(cx, out, names, "Class name");
}

fn sub_class_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = all::<ast::Class>(cx).filter_map(|c| c.name());
    check_names(cx, out, names, "Class name");
}

fn signal_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = all::<ast::Signal>(cx).filter_map(|s| s.name());
    check_names(cx, out, names, "Signal name");
}

fn class_variable_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = variables(cx, true)
        .filter(|(_, loads)| *loads == Loads::Nothing)
        .filter_map(|(var, _)| var.name());
    check_names(cx, out, names, "Class-scope variable name");
}

fn class_load_variable_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = variables(cx, true)
        .filter(|(_, loads)| *loads != Loads::Nothing)
        .filter_map(|(var, _)| var.name());
    check_names(cx, out, names, "Class-scope load/preload variable name");
}

fn function_variable_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = variables(cx, false)
        .filter(|(_, loads)| *loads == Loads::Nothing)
        .filter_map(|(var, _)| var.name());
    check_names(cx, out, names, "Function-scope variable name");
}

fn function_preload_variable_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = variables(cx, false)
        .filter(|(_, loads)| *loads == Loads::Preload)
        .filter_map(|(var, _)| var.name());
    check_names(cx, out, names, "Function-scope preload variable name");
}

fn function_argument_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = all::<ast::Param>(cx)
        .filter(|param| {
            let list = param.syntax().parent();
            matches!(list.and_then(|l| parent_kind(&l)), Some(FUNC | LAMBDA))
        })
        .filter_map(|param| param.name());
    check_names(cx, out, names, "Function argument name");
}

fn loop_variable_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = all::<ast::ForStmt>(cx).filter_map(|f| f.variable());
    check_names(cx, out, names, "Loop variable name");
}

fn enum_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = all::<ast::Enum>(cx).filter_map(|e| e.name());
    check_names(cx, out, names, "Enum name");
}

fn enum_element_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = all::<ast::EnumValue>(cx).filter_map(|e| e.name());
    check_names(cx, out, names, "Enum element name");
}

fn constant_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = all::<ast::Const>(cx)
        .filter(|c| loads(c.initializer()) == Loads::Nothing)
        .filter_map(|c| c.name());
    check_names(cx, out, names, "Constant name");
}

fn load_constant_name(cx: &Context, out: &mut Vec<Finding>) {
    let names = all::<ast::Const>(cx)
        .filter(|c| loads(c.initializer()) != Loads::Nothing)
        .filter_map(|c| c.name());
    check_names(cx, out, names, "Constant (load/preload) name");
}

pub const FILE_NAME: Rule = Rule {
    id: "file-name",
    concern: "naming:file",
    summary: "A script's file name does not match `pattern`.",
    default_enabled: false,
    fixable: false,
    options: &[RuleOption {
        name: "pattern",
        kind: OptionKind::Pattern("[a-z0-9]+(_[a-z0-9]+)*"),
        description: "A regular expression the whole file name, without `.gd`, must match.",
    }],
    check: Some(file_name),
};

fn file_name(cx: &Context, out: &mut Vec<Finding>) {
    let stem = cx.file_name.strip_suffix(".gd").unwrap_or(cx.file_name);
    if !cx.options.pattern("pattern").is_match(stem) {
        out.push(Finding::new(
            0..0,
            format!("File name \"{}\" is not valid", cx.file_name),
        ));
    }
}

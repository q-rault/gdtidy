//! Typed access to the tree: one wrapper per kind of declaration, statement
//! and expression, with accessors for their parts.
//!
//! Every accessor returns an `Option` or an iterator, because a file with
//! errors still has a tree and any part of it may be missing.

use crate::SyntaxKind::{self, *};
use crate::{SyntaxNode, SyntaxToken, parser};

/// A typed view of a node of one kind.
pub trait AstNode: Sized {
    fn cast(node: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;

    /// The node's source text, trivia inside it included.
    fn text(&self) -> String {
        self.syntax().to_string()
    }
}

macro_rules! ast_node {
    ($($(#[$doc:meta])* $name:ident => $kind:ident,)*) => {$(
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            fn cast(node: SyntaxNode) -> Option<Self> {
                (node.kind() == $kind).then(|| Self(node))
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    )*};
}

ast_node! {
    /// A whole file: script annotations, `class_name`, `extends` and members.
    SourceFile => SOURCE_FILE,
    Annotation => ANNOTATION_NODE,
    ClassName => CLASS_NAME,
    Extends => EXTENDS,
    /// An inner class.
    Class => CLASS,
    ClassBody => CLASS_BODY,
    StringStatement => STRING_STATEMENT,
    /// A variable, at class level or in a block.
    Var => VAR,
    /// A constant, at class level or in a block.
    Const => CONST,
    Signal => SIGNAL,
    Enum => ENUM,
    EnumValue => ENUM_VALUE,
    Func => FUNC,
    ParamList => PARAM_LIST,
    Param => PARAM,
    ReturnType => RETURN_TYPE,
    Type => TYPE,
    Property => PROPERTY,
    Setter => SETTER,
    Getter => GETTER,
    Suite => SUITE,
    PassStmt => PASS_STMT,
    ReturnStmt => RETURN_STMT,
    BreakStmt => BREAK_STMT,
    ContinueStmt => CONTINUE_STMT,
    BreakpointStmt => BREAKPOINT_STMT,
    ExprStmt => EXPR_STMT,
    IfStmt => IF_STMT,
    ElifClause => ELIF_CLAUSE,
    ElseClause => ELSE_CLAUSE,
    ForStmt => FOR_STMT,
    WhileStmt => WHILE_STMT,
    MatchStmt => MATCH_STMT,
    MatchBranch => MATCH_BRANCH,
    AssertStmt => ASSERT_STMT,
    ArgList => ARG_LIST,
    Literal => LITERAL,
    NameRef => NAME_REF,
    UnaryExpr => UNARY_EXPR,
    BinaryExpr => BINARY_EXPR,
    TernaryExpr => TERNARY_EXPR,
    Assignment => ASSIGNMENT,
    AwaitExpr => AWAIT_EXPR,
    ArrayExpr => ARRAY,
    DictExpr => DICT,
    DictEntry => DICT_ENTRY,
    ParenExpr => PAREN_EXPR,
    AttributeExpr => ATTRIBUTE,
    SubscriptExpr => SUBSCRIPT,
    CastExpr => CAST,
    CallExpr => CALL,
    GetNodeExpr => GET_NODE,
    PreloadExpr => PRELOAD,
    LambdaExpr => LAMBDA,
    TypeTestExpr => TYPE_TEST,
}

// ---- Helpers ----

fn child<N: AstNode>(node: &SyntaxNode) -> Option<N> {
    node.children().find_map(N::cast)
}

fn children<N: AstNode>(node: &SyntaxNode) -> impl Iterator<Item = N> {
    node.children().filter_map(N::cast)
}

fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == kind)
}

/// The first name token directly in `node`: an identifier, or a keyword
/// Godot accepts as a name.
fn name_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| parser::is_identifier(token.kind()))
}

fn has_token(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    token(node, kind).is_some()
}

/// The `##` documentation comment lines just before `node`, in order, with
/// only blank space between them and the node.
pub fn doc_comments(node: &SyntaxNode) -> Vec<SyntaxToken> {
    let mut lines = Vec::new();
    let mut element = node.prev_sibling_or_token();
    let mut breaks = 0;
    while let Some(current) = element {
        match current.as_token() {
            Some(token) if token.kind() == DOC_COMMENT => {
                lines.push(token.clone());
                breaks = 0;
            }
            Some(token) if token.kind() == WHITESPACE => {}
            Some(token) if matches!(token.kind(), LINE_BREAK | NEWLINE) && breaks == 0 => {
                breaks += 1;
            }
            _ => break,
        }
        element = current.prev_sibling_or_token();
    }
    lines.reverse();
    lines
}

// ---- Declarations ----

/// A declaration at class level.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Member {
    Var(Var),
    Const(Const),
    Signal(Signal),
    Enum(Enum),
    Func(Func),
    Class(Class),
}

impl Member {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        Some(match node.kind() {
            VAR => Member::Var(Var(node)),
            CONST => Member::Const(Const(node)),
            SIGNAL => Member::Signal(Signal(node)),
            ENUM => Member::Enum(Enum(node)),
            FUNC => Member::Func(Func(node)),
            CLASS => Member::Class(Class(node)),
            _ => return None,
        })
    }

    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Member::Var(n) => n.syntax(),
            Member::Const(n) => n.syntax(),
            Member::Signal(n) => n.syntax(),
            Member::Enum(n) => n.syntax(),
            Member::Func(n) => n.syntax(),
            Member::Class(n) => n.syntax(),
        }
    }

    /// The member's name; an unnamed enum has none.
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(self.syntax())
    }

    pub fn annotations(&self) -> impl Iterator<Item = Annotation> {
        children(self.syntax())
    }

    pub fn doc_comments(&self) -> Vec<SyntaxToken> {
        doc_comments(self.syntax())
    }
}

impl SourceFile {
    pub fn class_name(&self) -> Option<ClassName> {
        child(&self.0)
    }

    /// `extends`, on its own line or after `class_name`.
    pub fn extends(&self) -> Option<Extends> {
        child(&self.0).or_else(|| self.class_name().and_then(|c| c.extends()))
    }

    /// Annotations that apply to the script, such as `@tool`.
    pub fn annotations(&self) -> impl Iterator<Item = Annotation> {
        children(&self.0)
    }

    pub fn members(&self) -> impl Iterator<Item = Member> {
        self.0.children().filter_map(Member::cast)
    }
}

impl ClassName {
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }

    pub fn extends(&self) -> Option<Extends> {
        child(&self.0)
    }
}

impl Extends {
    /// The script path in `extends "res://base.gd"`.
    pub fn path(&self) -> Option<SyntaxToken> {
        token(&self.0, STRING)
    }

    /// The class names, as in `extends Base.Inner`.
    pub fn names(&self) -> impl Iterator<Item = SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| parser::is_identifier(token.kind()))
    }
}

impl Class {
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }

    pub fn extends(&self) -> Option<Extends> {
        child(&self.0).or_else(|| self.body().and_then(|b| child(&b.0)))
    }

    pub fn body(&self) -> Option<ClassBody> {
        child(&self.0)
    }
}

impl ClassBody {
    pub fn members(&self) -> impl Iterator<Item = Member> {
        self.0.children().filter_map(Member::cast)
    }
}

impl Annotation {
    /// The name with its `@`, such as `@export`.
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, ANNOTATION)
    }

    pub fn arguments(&self) -> impl Iterator<Item = Expr> {
        self.0
            .children()
            .filter(|n| n.kind() == ANNOTATION_ARGS)
            .flat_map(|args| args.children().filter_map(Expr::cast).collect::<Vec<_>>())
    }
}

impl Var {
    pub fn annotations(&self) -> impl Iterator<Item = Annotation> {
        children(&self.0)
    }

    pub fn is_static(&self) -> bool {
        has_token(&self.0, STATIC_KW)
    }

    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }

    pub fn declared_type(&self) -> Option<Type> {
        child(&self.0)
    }

    /// Whether the type is inferred with `:=`.
    pub fn is_inferred(&self) -> bool {
        // A `:` directly followed by `=`, with no type node between them.
        let kinds: Vec<_> = self
            .0
            .children_with_tokens()
            .map(|e| e.kind())
            .filter(|kind| !kind.is_trivia())
            .collect();
        kinds.windows(2).any(|w| w == [COLON, EQUAL])
    }

    pub fn initializer(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }

    pub fn property(&self) -> Option<Property> {
        child(&self.0)
    }
}

impl Const {
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }

    pub fn declared_type(&self) -> Option<Type> {
        child(&self.0)
    }

    pub fn initializer(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl Property {
    pub fn setter(&self) -> Option<Setter> {
        child(&self.0)
    }

    pub fn getter(&self) -> Option<Getter> {
        child(&self.0)
    }
}

impl Setter {
    /// The body of an inline setter, `set(value): ...`.
    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }
}

impl Getter {
    /// The body of an inline getter, `get: ...`.
    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }
}

impl Signal {
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }

    pub fn params(&self) -> Option<ParamList> {
        child(&self.0)
    }
}

impl Enum {
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }

    pub fn values(&self) -> impl Iterator<Item = EnumValue> {
        children(&self.0)
    }
}

impl EnumValue {
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }

    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl Func {
    pub fn annotations(&self) -> impl Iterator<Item = Annotation> {
        children(&self.0)
    }

    pub fn is_static(&self) -> bool {
        has_token(&self.0, STATIC_KW)
    }

    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }

    pub fn params(&self) -> Option<ParamList> {
        child(&self.0)
    }

    pub fn return_type(&self) -> Option<Type> {
        child::<ReturnType>(&self.0).and_then(|r| child(&r.0))
    }

    /// The body; abstract functions have none.
    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }
}

impl ParamList {
    pub fn params(&self) -> impl Iterator<Item = Param> {
        children(&self.0)
    }
}

impl Param {
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }

    /// Whether this is the rest parameter, `...args`.
    pub fn is_rest(&self) -> bool {
        has_token(&self.0, PERIOD_PERIOD_PERIOD)
    }

    pub fn declared_type(&self) -> Option<Type> {
        child(&self.0)
    }

    pub fn default_value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl Type {
    /// The names in the type, as in `Base.Inner`.
    pub fn names(&self) -> impl Iterator<Item = SyntaxToken> {
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| parser::is_identifier(t.kind()) || t.kind() == VOID_KW)
    }

    /// The element types of a typed collection, as in `Dictionary[String, int]`.
    pub fn arguments(&self) -> impl Iterator<Item = Type> {
        self.0
            .children()
            .filter(|n| n.kind() == TYPE_ARGS)
            .flat_map(|args| children::<Type>(&args).collect::<Vec<_>>())
    }
}

// ---- Statements ----

/// A statement in a block.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stmt {
    Var(Var),
    Const(Const),
    Pass(PassStmt),
    Return(ReturnStmt),
    Break(BreakStmt),
    Continue(ContinueStmt),
    Breakpoint(BreakpointStmt),
    Expr(ExprStmt),
    If(IfStmt),
    For(ForStmt),
    While(WhileStmt),
    Match(MatchStmt),
    Assert(AssertStmt),
}

impl Stmt {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        Some(match node.kind() {
            VAR => Stmt::Var(Var(node)),
            CONST => Stmt::Const(Const(node)),
            PASS_STMT => Stmt::Pass(PassStmt(node)),
            RETURN_STMT => Stmt::Return(ReturnStmt(node)),
            BREAK_STMT => Stmt::Break(BreakStmt(node)),
            CONTINUE_STMT => Stmt::Continue(ContinueStmt(node)),
            BREAKPOINT_STMT => Stmt::Breakpoint(BreakpointStmt(node)),
            EXPR_STMT => Stmt::Expr(ExprStmt(node)),
            IF_STMT => Stmt::If(IfStmt(node)),
            FOR_STMT => Stmt::For(ForStmt(node)),
            WHILE_STMT => Stmt::While(WhileStmt(node)),
            MATCH_STMT => Stmt::Match(MatchStmt(node)),
            ASSERT_STMT => Stmt::Assert(AssertStmt(node)),
            _ => return None,
        })
    }

    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Stmt::Var(n) => n.syntax(),
            Stmt::Const(n) => n.syntax(),
            Stmt::Pass(n) => n.syntax(),
            Stmt::Return(n) => n.syntax(),
            Stmt::Break(n) => n.syntax(),
            Stmt::Continue(n) => n.syntax(),
            Stmt::Breakpoint(n) => n.syntax(),
            Stmt::Expr(n) => n.syntax(),
            Stmt::If(n) => n.syntax(),
            Stmt::For(n) => n.syntax(),
            Stmt::While(n) => n.syntax(),
            Stmt::Match(n) => n.syntax(),
            Stmt::Assert(n) => n.syntax(),
        }
    }
}

impl Suite {
    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.0.children().filter_map(Stmt::cast)
    }

    /// Whether the block is on the same line, as in `if x: return`.
    pub fn is_inline(&self) -> bool {
        !has_token(&self.0, INDENT)
    }
}

impl ReturnStmt {
    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl ExprStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

impl IfStmt {
    pub fn condition(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }

    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }

    pub fn elif_clauses(&self) -> impl Iterator<Item = ElifClause> {
        children(&self.0)
    }

    pub fn else_clause(&self) -> Option<ElseClause> {
        child(&self.0)
    }
}

impl ElifClause {
    pub fn condition(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }

    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }
}

impl ElseClause {
    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }
}

impl ForStmt {
    pub fn variable(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }

    pub fn declared_type(&self) -> Option<Type> {
        child(&self.0)
    }

    pub fn iterable(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }

    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }
}

impl WhileStmt {
    pub fn condition(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }

    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }
}

impl MatchStmt {
    pub fn subject(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }

    pub fn branches(&self) -> impl Iterator<Item = MatchBranch> {
        children(&self.0)
    }
}

impl MatchBranch {
    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }

    /// Whether the branch has a `when` guard.
    pub fn has_guard(&self) -> bool {
        self.0.children().any(|n| n.kind() == PATTERN_GUARD)
    }
}

impl AssertStmt {
    pub fn arguments(&self) -> Option<ArgList> {
        child(&self.0)
    }
}

// ---- Expressions ----

/// An expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    Literal(Literal),
    Name(NameRef),
    SelfExpr(SyntaxNode),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    Ternary(TernaryExpr),
    Assignment(Assignment),
    Await(AwaitExpr),
    Array(ArrayExpr),
    Dict(DictExpr),
    Paren(ParenExpr),
    Attribute(AttributeExpr),
    Subscript(SubscriptExpr),
    Cast(CastExpr),
    Call(CallExpr),
    GetNode(GetNodeExpr),
    Preload(PreloadExpr),
    Lambda(LambdaExpr),
    TypeTest(TypeTestExpr),
}

impl Expr {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        Some(match node.kind() {
            LITERAL => Expr::Literal(Literal(node)),
            NAME_REF => Expr::Name(NameRef(node)),
            SELF_EXPR => Expr::SelfExpr(node),
            UNARY_EXPR => Expr::Unary(UnaryExpr(node)),
            BINARY_EXPR => Expr::Binary(BinaryExpr(node)),
            TERNARY_EXPR => Expr::Ternary(TernaryExpr(node)),
            ASSIGNMENT => Expr::Assignment(Assignment(node)),
            AWAIT_EXPR => Expr::Await(AwaitExpr(node)),
            ARRAY => Expr::Array(ArrayExpr(node)),
            DICT => Expr::Dict(DictExpr(node)),
            PAREN_EXPR => Expr::Paren(ParenExpr(node)),
            ATTRIBUTE => Expr::Attribute(AttributeExpr(node)),
            SUBSCRIPT => Expr::Subscript(SubscriptExpr(node)),
            CAST => Expr::Cast(CastExpr(node)),
            CALL => Expr::Call(CallExpr(node)),
            GET_NODE => Expr::GetNode(GetNodeExpr(node)),
            PRELOAD => Expr::Preload(PreloadExpr(node)),
            LAMBDA => Expr::Lambda(LambdaExpr(node)),
            TYPE_TEST => Expr::TypeTest(TypeTestExpr(node)),
            _ => return None,
        })
    }

    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Expr::Literal(n) => n.syntax(),
            Expr::Name(n) => n.syntax(),
            Expr::SelfExpr(n) => n,
            Expr::Unary(n) => n.syntax(),
            Expr::Binary(n) => n.syntax(),
            Expr::Ternary(n) => n.syntax(),
            Expr::Assignment(n) => n.syntax(),
            Expr::Await(n) => n.syntax(),
            Expr::Array(n) => n.syntax(),
            Expr::Dict(n) => n.syntax(),
            Expr::Paren(n) => n.syntax(),
            Expr::Attribute(n) => n.syntax(),
            Expr::Subscript(n) => n.syntax(),
            Expr::Cast(n) => n.syntax(),
            Expr::Call(n) => n.syntax(),
            Expr::GetNode(n) => n.syntax(),
            Expr::Preload(n) => n.syntax(),
            Expr::Lambda(n) => n.syntax(),
            Expr::TypeTest(n) => n.syntax(),
        }
    }
}

fn exprs(node: &SyntaxNode) -> impl Iterator<Item = Expr> {
    node.children().filter_map(Expr::cast)
}

/// The first non-trivia token directly in `node` that is not part of a
/// child node.
fn operator(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia())
}

impl Literal {
    pub fn token(&self) -> Option<SyntaxToken> {
        operator(&self.0)
    }
}

impl NameRef {
    pub fn name(&self) -> Option<SyntaxToken> {
        operator(&self.0)
    }
}

impl UnaryExpr {
    pub fn operator(&self) -> Option<SyntaxToken> {
        operator(&self.0)
    }

    pub fn operand(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }
}

impl BinaryExpr {
    pub fn lhs(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }

    pub fn rhs(&self) -> Option<Expr> {
        exprs(&self.0).nth(1)
    }

    /// The operator; for `not in`, the `not`.
    pub fn operator(&self) -> Option<SyntaxToken> {
        operator(&self.0)
    }
}

impl TernaryExpr {
    pub fn true_value(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }

    pub fn condition(&self) -> Option<Expr> {
        exprs(&self.0).nth(1)
    }

    pub fn false_value(&self) -> Option<Expr> {
        exprs(&self.0).nth(2)
    }
}

impl Assignment {
    pub fn target(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }

    pub fn value(&self) -> Option<Expr> {
        exprs(&self.0).nth(1)
    }

    pub fn operator(&self) -> Option<SyntaxToken> {
        operator(&self.0)
    }
}

impl AwaitExpr {
    pub fn operand(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }
}

impl ArrayExpr {
    pub fn elements(&self) -> impl Iterator<Item = Expr> {
        exprs(&self.0)
    }
}

impl DictExpr {
    pub fn entries(&self) -> impl Iterator<Item = DictEntry> {
        children(&self.0)
    }
}

impl DictEntry {
    pub fn key(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }

    pub fn value(&self) -> Option<Expr> {
        exprs(&self.0).nth(1)
    }

    /// Whether the entry is Lua-style, `key = value`.
    pub fn is_lua_style(&self) -> bool {
        has_token(&self.0, EQUAL)
    }
}

impl ParenExpr {
    pub fn inner(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }
}

impl AttributeExpr {
    pub fn base(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }

    pub fn name(&self) -> Option<SyntaxToken> {
        let mut after_period = false;
        for token in self.0.children_with_tokens().filter_map(|e| e.into_token()) {
            if after_period && !token.kind().is_trivia() {
                return Some(token);
            }
            after_period |= token.kind() == PERIOD;
        }
        None
    }
}

impl SubscriptExpr {
    pub fn base(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }

    pub fn index(&self) -> Option<Expr> {
        exprs(&self.0).nth(1)
    }
}

impl CastExpr {
    pub fn operand(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }

    pub fn target_type(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl CallExpr {
    /// What is called; `None` for `super(...)` and `super.name(...)`.
    pub fn callee(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }

    pub fn is_super(&self) -> bool {
        has_token(&self.0, SUPER_KW)
    }

    pub fn arguments(&self) -> impl Iterator<Item = Expr> {
        child::<ArgList>(&self.0)
            .into_iter()
            .flat_map(|args| exprs(&args.0).collect::<Vec<_>>())
    }
}

impl ArgList {
    pub fn arguments(&self) -> impl Iterator<Item = Expr> {
        exprs(&self.0)
    }
}

impl PreloadExpr {
    pub fn path(&self) -> Option<Expr> {
        child::<ArgList>(&self.0).and_then(|args| exprs(&args.0).next())
    }
}

impl LambdaExpr {
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }

    pub fn params(&self) -> Option<ParamList> {
        child(&self.0)
    }

    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }
}

impl TypeTestExpr {
    pub fn operand(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }

    pub fn is_negated(&self) -> bool {
        has_token(&self.0, NOT_KW)
    }

    pub fn tested_type(&self) -> Option<Type> {
        child(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(src: &str) -> SourceFile {
        let parse = parser::parse(src);
        assert!(parse.errors.is_empty(), "{:?}", parse.errors);
        SourceFile::cast(SyntaxNode::new_root(parse.green)).unwrap()
    }

    #[test]
    fn members_and_their_parts() {
        let file = file(
            "@tool\nclass_name Player extends Node2D\n\n## Speed in pixels.\n@export var speed: float = 1.0\nstatic func make(a: int, ...rest) -> Player:\n\treturn null\n",
        );
        assert_eq!(file.class_name().unwrap().name().unwrap().text(), "Player");
        assert_eq!(
            file.extends().unwrap().names().next().unwrap().text(),
            "Node2D"
        );
        assert_eq!(file.annotations().count(), 1);

        let members: Vec<_> = file.members().collect();
        assert_eq!(members.len(), 2);
        let Member::Var(var) = &members[0] else {
            panic!()
        };
        assert_eq!(var.name().unwrap().text(), "speed");
        assert_eq!(var.declared_type().unwrap().text(), "float");
        assert_eq!(var.initializer().unwrap().syntax().to_string(), "1.0");
        assert_eq!(
            var.annotations().next().unwrap().name().unwrap().text(),
            "@export"
        );
        assert_eq!(members[0].doc_comments()[0].text(), "## Speed in pixels.");

        let Member::Func(func) = &members[1] else {
            panic!()
        };
        assert!(func.is_static());
        assert_eq!(func.name().unwrap().text(), "make");
        let params: Vec<_> = func.params().unwrap().params().collect();
        assert_eq!(params.len(), 2);
        assert!(params[1].is_rest());
        assert_eq!(func.return_type().unwrap().text(), "Player");
        let body: Vec<_> = func.body().unwrap().statements().collect();
        assert!(matches!(body[0], Stmt::Return(_)));
    }

    #[test]
    fn expressions() {
        let file = file("var x = a.b(1, 2) + c[0] * 3\n");
        let Some(Member::Var(var)) = file.members().next() else {
            panic!()
        };
        let Some(Expr::Binary(sum)) = var.initializer() else {
            panic!()
        };
        assert_eq!(sum.operator().unwrap().text(), "+");
        let Some(Expr::Call(call)) = sum.lhs() else {
            panic!()
        };
        assert_eq!(call.arguments().count(), 2);
        let Some(Expr::Attribute(callee)) = call.callee() else {
            panic!()
        };
        assert_eq!(callee.name().unwrap().text(), "b");
        assert!(matches!(sum.rhs(), Some(Expr::Binary(_))));
    }

    #[test]
    fn typed_collections_and_inferred_types() {
        let file = file("var a: Dictionary[String, int] = {}\nvar b := 1\n");
        let members: Vec<_> = file.members().collect();
        let Member::Var(a) = &members[0] else {
            panic!()
        };
        let args: Vec<_> = a
            .declared_type()
            .unwrap()
            .arguments()
            .map(|t| t.text())
            .collect();
        assert_eq!(args, ["String", "int"]);
        let Member::Var(b) = &members[1] else {
            panic!()
        };
        assert!(b.is_inferred());
        assert!(!a.is_inferred());
    }
}

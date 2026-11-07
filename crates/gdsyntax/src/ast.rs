//! Typed access to the tree: one wrapper per kind of declaration, statement
//! and expression, with accessors for their parts.
//!
//! Every accessor returns an `Option` or an iterator, because a file with
//! errors still has a tree and any part of it may be missing. Code the
//! parser could not read is kept in [`Error`] nodes, which appear as the
//! `Error` variant of [`Member`], [`Stmt`], [`Expr`] and [`Pattern`].
//!
//! On a file without errors, every node of the tree is reachable from
//! [`SourceFile`] through these accessors, except three list nodes whose
//! items are returned directly: `RETURN_TYPE` (see [`Func::return_type`]),
//! `TYPE_ARGS` ([`Type::arguments`]) and `ANNOTATION_ARGS`
//! ([`Annotation::arguments`]). The `typed_tree` test checks this on all of
//! Godot's test scripts.

use crate::SyntaxKind::{self, *};
use crate::literal::{self, LiteralValue};
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
    SelfExpr => SELF_EXPR,
    /// Code the parser could not read, after a syntax error.
    Error => ERROR,
    /// The `when` guard of a match branch.
    PatternGuard => PATTERN_GUARD,
    /// `var name` in a pattern.
    BindPattern => BIND_PATTERN,
    /// `_`.
    WildcardPattern => WILDCARD_PATTERN,
    /// `..` at the end of an array or dictionary pattern.
    RestPattern => REST_PATTERN,
    ArrayPattern => ARRAY_PATTERN,
    DictPattern => DICT_PATTERN,
    DictPatternEntry => DICT_PATTERN_ENTRY,
    /// A literal, a constant name or another expression to compare with.
    ExprPattern => EXPR_PATTERN,
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

/// The first non-trivia token directly in `node` after a `kind` token.
fn token_after(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .skip_while(|token| token.kind() != kind)
        .nth(1)
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
    /// A string used as a comment.
    StringStatement(StringStatement),
    /// `pass`, as in an empty inner class.
    Pass(PassStmt),
    Error(Error),
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
            STRING_STATEMENT => Member::StringStatement(StringStatement(node)),
            PASS_STMT => Member::Pass(PassStmt(node)),
            ERROR => Member::Error(Error(node)),
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
            Member::StringStatement(n) => n.syntax(),
            Member::Pass(n) => n.syntax(),
            Member::Error(n) => n.syntax(),
        }
    }

    /// The member's name; an unnamed enum, a string statement, `pass` and an
    /// error have none.
    pub fn name(&self) -> Option<SyntaxToken> {
        match self {
            Member::StringStatement(_) | Member::Pass(_) | Member::Error(_) => None,
            _ => name_token(self.syntax()),
        }
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

    /// Annotations that apply to the script, such as `@tool`, and those on
    /// their own line, such as `@warning_ignore_start`.
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
    pub fn annotations(&self) -> impl Iterator<Item = Annotation> {
        children(&self.0)
    }

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

    /// Annotations on their own line, such as `@warning_ignore_start`.
    pub fn annotations(&self) -> impl Iterator<Item = Annotation> {
        children(&self.0)
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
    /// The parameter list of an inline setter, `set(value): ...`.
    pub fn params(&self) -> Option<ParamList> {
        child(&self.0)
    }

    /// The body of an inline setter.
    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }

    /// The function named in `set = function`.
    pub fn function(&self) -> Option<SyntaxToken> {
        token_after(&self.0, EQUAL)
    }
}

impl Getter {
    /// The body of an inline getter, `get: ...`.
    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }

    /// The function named in `get = function`.
    pub fn function(&self) -> Option<SyntaxToken> {
        token_after(&self.0, EQUAL)
    }
}

impl StringStatement {
    pub fn token(&self) -> Option<SyntaxToken> {
        token(&self.0, STRING)
    }

    /// The string's contents, decoded.
    pub fn value(&self) -> Option<String> {
        literal::string_value(self.token()?.text())
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
    /// An annotation on its own line, such as `@warning_ignore_start`.
    Annotation(Annotation),
    Error(Error),
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
            ANNOTATION_NODE => Stmt::Annotation(Annotation(node)),
            ERROR => Stmt::Error(Error(node)),
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
            Stmt::Annotation(n) => n.syntax(),
            Stmt::Error(n) => n.syntax(),
        }
    }

    /// The annotations on the statement, such as `@warning_ignore`.
    pub fn annotations(&self) -> impl Iterator<Item = Annotation> {
        let own = !matches!(self, Stmt::Annotation(_));
        children(self.syntax()).filter(move |_| own)
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

    /// A `pass` in place of the branches.
    pub fn pass(&self) -> Option<PassStmt> {
        child(&self.0)
    }

    pub fn branches(&self) -> impl Iterator<Item = MatchBranch> {
        children(&self.0)
    }
}

impl MatchBranch {
    pub fn annotations(&self) -> impl Iterator<Item = Annotation> {
        children(&self.0)
    }

    /// The patterns, as in `1, 2, 3:`.
    pub fn patterns(&self) -> impl Iterator<Item = Pattern> {
        self.0.children().filter_map(Pattern::cast)
    }

    /// The `when` guard.
    pub fn guard(&self) -> Option<PatternGuard> {
        child(&self.0)
    }

    /// Whether the branch has a `when` guard.
    pub fn has_guard(&self) -> bool {
        self.guard().is_some()
    }

    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }
}

impl PatternGuard {
    pub fn condition(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }
}

// ---- Patterns ----

/// A pattern in a match branch.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Pattern {
    Expr(ExprPattern),
    Bind(BindPattern),
    Wildcard(WildcardPattern),
    Rest(RestPattern),
    Array(ArrayPattern),
    Dict(DictPattern),
    Error(Error),
}

impl Pattern {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        Some(match node.kind() {
            EXPR_PATTERN => Pattern::Expr(ExprPattern(node)),
            BIND_PATTERN => Pattern::Bind(BindPattern(node)),
            WILDCARD_PATTERN => Pattern::Wildcard(WildcardPattern(node)),
            REST_PATTERN => Pattern::Rest(RestPattern(node)),
            ARRAY_PATTERN => Pattern::Array(ArrayPattern(node)),
            DICT_PATTERN => Pattern::Dict(DictPattern(node)),
            ERROR => Pattern::Error(Error(node)),
            _ => return None,
        })
    }

    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Pattern::Expr(n) => n.syntax(),
            Pattern::Bind(n) => n.syntax(),
            Pattern::Wildcard(n) => n.syntax(),
            Pattern::Rest(n) => n.syntax(),
            Pattern::Array(n) => n.syntax(),
            Pattern::Dict(n) => n.syntax(),
            Pattern::Error(n) => n.syntax(),
        }
    }
}

impl ExprPattern {
    pub fn expr(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }
}

impl BindPattern {
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }
}

impl ArrayPattern {
    /// The element patterns, the closing `..` included.
    pub fn elements(&self) -> impl Iterator<Item = Pattern> {
        self.0.children().filter_map(Pattern::cast)
    }
}

impl DictPattern {
    pub fn entries(&self) -> impl Iterator<Item = DictPatternEntry> {
        children(&self.0)
    }

    /// The closing `..`, which matches any other keys.
    pub fn rest(&self) -> Option<RestPattern> {
        child(&self.0)
    }
}

impl DictPatternEntry {
    pub fn key(&self) -> Option<Expr> {
        exprs(&self.0).next()
    }

    /// The value pattern; `None` in `{"key", ..}`, which only checks the key.
    pub fn value(&self) -> Option<Pattern> {
        self.0.children().find_map(Pattern::cast)
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
    SelfExpr(SelfExpr),
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
    Error(Error),
}

impl Expr {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        Some(match node.kind() {
            LITERAL => Expr::Literal(Literal(node)),
            NAME_REF => Expr::Name(NameRef(node)),
            SELF_EXPR => Expr::SelfExpr(SelfExpr(node)),
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
            ERROR => Expr::Error(Error(node)),
            _ => return None,
        })
    }

    pub fn syntax(&self) -> &SyntaxNode {
        match self {
            Expr::Literal(n) => n.syntax(),
            Expr::Name(n) => n.syntax(),
            Expr::SelfExpr(n) => n.syntax(),
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
            Expr::Error(n) => n.syntax(),
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

    /// The decoded value. `None` for a malformed literal, or an integer that
    /// does not fit in 64 bits. `PI`, `TAU`, `INF` and `NAN` are names, not
    /// literals, as in Godot.
    pub fn value(&self) -> Option<LiteralValue> {
        let token = self.token()?;
        let text = token.text();
        Some(match token.kind() {
            INT => LiteralValue::Int(literal::int_value(text)?),
            FLOAT => LiteralValue::Float(literal::float_value(text)?),
            STRING => LiteralValue::String(literal::string_value(text)?),
            STRING_NAME => LiteralValue::StringName(literal::string_value(text)?),
            NODE_PATH => LiteralValue::NodePath(literal::string_value(text)?),
            TRUE_KW => LiteralValue::Bool(true),
            FALSE_KW => LiteralValue::Bool(false),
            NULL_KW => LiteralValue::Null,
            _ => return None,
        })
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

    pub fn op(&self) -> Option<UnaryOp> {
        Some(match self.operator()?.kind() {
            MINUS => UnaryOp::Negate,
            PLUS => UnaryOp::Plus,
            TILDE => UnaryOp::BitNot,
            NOT_KW | BANG => UnaryOp::Not,
            _ => return None,
        })
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

    /// The operator token; for `not in`, the `not`.
    pub fn operator(&self) -> Option<SyntaxToken> {
        operator(&self.0)
    }

    pub fn op(&self) -> Option<BinaryOp> {
        use BinaryOp::*;
        Some(match self.operator()?.kind() {
            PLUS => Add,
            MINUS => Subtract,
            STAR => Multiply,
            SLASH => Divide,
            PERCENT => Modulo,
            STAR_STAR => Power,
            LESS_LESS => ShiftLeft,
            GREATER_GREATER => ShiftRight,
            AMPERSAND => BitAnd,
            PIPE => BitOr,
            CARET => BitXor,
            AND_KW | AMPERSAND_AMPERSAND => And,
            OR_KW | PIPE_PIPE => Or,
            EQUAL_EQUAL => Equal,
            BANG_EQUAL => NotEqual,
            LESS => Less,
            LESS_EQUAL => LessEqual,
            GREATER => Greater,
            GREATER_EQUAL => GreaterEqual,
            IN_KW => In,
            NOT_KW => NotIn,
            _ => return None,
        })
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

    pub fn op(&self) -> Option<AssignOp> {
        use AssignOp::*;
        Some(match self.operator()?.kind() {
            EQUAL => Assign,
            PLUS_EQUAL => Add,
            MINUS_EQUAL => Subtract,
            STAR_EQUAL => Multiply,
            STAR_STAR_EQUAL => Power,
            SLASH_EQUAL => Divide,
            PERCENT_EQUAL => Modulo,
            LESS_LESS_EQUAL => ShiftLeft,
            GREATER_GREATER_EQUAL => ShiftRight,
            AMPERSAND_EQUAL => BitAnd,
            PIPE_EQUAL => BitOr,
            CARET_EQUAL => BitXor,
            _ => return None,
        })
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

impl GetNodeExpr {
    /// Whether this is a scene-unique name, `%Name`, rather than `$Path`.
    pub fn is_unique(&self) -> bool {
        has_token(&self.0, PERCENT)
    }

    /// The node path, as Godot reads it: `$Path/To` and `$"Path/To"` both
    /// give `Path/To`.
    pub fn path(&self) -> Option<String> {
        let mut path = String::new();
        for token in self
            .0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| !t.kind().is_trivia() && !matches!(t.kind(), DOLLAR | PERCENT))
        {
            if token.kind() == STRING {
                path.push_str(&literal::string_value(token.text())?);
            } else {
                path.push_str(token.text());
            }
        }
        Some(path)
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

    pub fn arg_list(&self) -> Option<ArgList> {
        child(&self.0)
    }

    pub fn arguments(&self) -> impl Iterator<Item = Expr> {
        self.arg_list()
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
    pub fn arg_list(&self) -> Option<ArgList> {
        child(&self.0)
    }

    pub fn path(&self) -> Option<Expr> {
        self.arg_list().and_then(|args| exprs(&args.0).next())
    }
}

impl LambdaExpr {
    pub fn name(&self) -> Option<SyntaxToken> {
        name_token(&self.0)
    }

    pub fn params(&self) -> Option<ParamList> {
        child(&self.0)
    }

    pub fn return_type(&self) -> Option<Type> {
        child::<ReturnType>(&self.0).and_then(|r| child(&r.0))
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

// ---- Operators ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// `-x`
    Negate,
    /// `+x`
    Plus,
    /// `~x`
    BitNot,
    /// `not x` or `!x`
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
    ShiftLeft,
    ShiftRight,
    BitAnd,
    BitOr,
    BitXor,
    /// `and` or `&&`
    And,
    /// `or` or `||`
    Or,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    In,
    NotIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssignOp {
    /// `=`
    Assign,
    Add,
    Subtract,
    Multiply,
    Power,
    Divide,
    Modulo,
    ShiftLeft,
    ShiftRight,
    BitAnd,
    BitOr,
    BitXor,
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

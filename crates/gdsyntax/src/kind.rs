//! Every kind of token and node in a GDScript syntax tree.
//!
//! Token kinds follow Godot's `GDScriptTokenizer::Token::Type` (Godot 4.7),
//! in the same order, with three differences needed for a lossless tree:
//!
//! - **Trivia kinds** (whitespace, comments, line breaks, `\` continuations)
//!   hold the text Godot's tokenizer skips.
//! - **Godot's `LITERAL`** is split by literal type, so tools can tell a
//!   string from a number without reading the text.
//! - **`NEWLINE`, `INDENT` and `DEDENT`** are the tokens Godot's parser sees.
//!   A `NEWLINE` is the line break that ends a statement; `INDENT` and
//!   `DEDENT` take no space and sit just before the first token of a line.

macro_rules! syntax_kinds {
    ($($(#[$doc:meta])* $kind:ident,)*) => {
        /// A token or node kind. Kinds before [`SyntaxKind::SOURCE_FILE`] are tokens.
        #[allow(non_camel_case_types)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        pub enum SyntaxKind {
            $($(#[$doc])* $kind,)*
        }

        const ALL: &[SyntaxKind] = &[$(SyntaxKind::$kind,)*];
    };
}

syntax_kinds! {
    // ---- Trivia: text Godot's tokenizer skips ----
    WHITESPACE,
    /// `# comment`, also a comment-only line.
    COMMENT,
    /// `## documentation comment`
    DOC_COMMENT,
    /// A line break that does not end a statement: blank lines, comment-only
    /// lines, and line breaks inside brackets.
    LINE_BREAK,
    /// The `\` that joins two lines. The line break after it is a `LINE_BREAK`.
    LINE_CONTINUATION,
    /// A UTF-8 byte order mark at the start of the file.
    BYTE_ORDER_MARK,

    // ---- Tokens Godot's parser sees ----
    ANNOTATION,
    IDENTIFIER,
    // Godot's LITERAL, split by type.
    INT,
    FLOAT,
    /// `"..."`, `'...'`, triple-quoted and raw (`r"..."`) strings.
    STRING,
    /// `&"name"`
    STRING_NAME,
    /// `^"path"`
    NODE_PATH,
    TRUE_KW,
    FALSE_KW,
    NULL_KW,
    // Comparison
    LESS,
    LESS_EQUAL,
    GREATER,
    GREATER_EQUAL,
    EQUAL_EQUAL,
    BANG_EQUAL,
    // Logical
    AND_KW,
    OR_KW,
    NOT_KW,
    AMPERSAND_AMPERSAND,
    PIPE_PIPE,
    BANG,
    // Bitwise
    AMPERSAND,
    PIPE,
    TILDE,
    CARET,
    LESS_LESS,
    GREATER_GREATER,
    // Math
    PLUS,
    MINUS,
    STAR,
    STAR_STAR,
    SLASH,
    PERCENT,
    // Assignment
    EQUAL,
    PLUS_EQUAL,
    MINUS_EQUAL,
    STAR_EQUAL,
    STAR_STAR_EQUAL,
    SLASH_EQUAL,
    PERCENT_EQUAL,
    LESS_LESS_EQUAL,
    GREATER_GREATER_EQUAL,
    AMPERSAND_EQUAL,
    PIPE_EQUAL,
    CARET_EQUAL,
    // Control flow
    IF_KW,
    ELIF_KW,
    ELSE_KW,
    FOR_KW,
    WHILE_KW,
    BREAK_KW,
    CONTINUE_KW,
    PASS_KW,
    RETURN_KW,
    MATCH_KW,
    WHEN_KW,
    // Keywords
    AS_KW,
    ASSERT_KW,
    AWAIT_KW,
    BREAKPOINT_KW,
    CLASS_KW,
    CLASS_NAME_KW,
    CONST_KW,
    ENUM_KW,
    EXTENDS_KW,
    FUNC_KW,
    IN_KW,
    IS_KW,
    NAMESPACE_KW,
    PRELOAD_KW,
    SELF_KW,
    SIGNAL_KW,
    STATIC_KW,
    SUPER_KW,
    TRAIT_KW,
    VAR_KW,
    VOID_KW,
    YIELD_KW,
    // Punctuation
    BRACKET_OPEN,
    BRACKET_CLOSE,
    BRACE_OPEN,
    BRACE_CLOSE,
    PARENTHESIS_OPEN,
    PARENTHESIS_CLOSE,
    COMMA,
    SEMICOLON,
    PERIOD,
    PERIOD_PERIOD,
    PERIOD_PERIOD_PERIOD,
    COLON,
    DOLLAR,
    FORWARD_ARROW,
    UNDERSCORE,
    // Whitespace Godot's parser sees
    NEWLINE,
    INDENT,
    DEDENT,
    // Constants
    CONST_PI,
    CONST_TAU,
    CONST_INF,
    CONST_NAN,
    // Kept for better error messages
    VCS_CONFLICT_MARKER,
    BACKTICK,
    QUESTION_MARK,
    /// Text the tokenizer could not turn into a token, such as an
    /// unterminated string or an invalid character. Always comes with an error.
    ERROR_TOKEN,
    /// End of input. Never stored in a tree.
    EOF,

    // ---- Nodes ----
    SOURCE_FILE,
    /// Code the parser could not make sense of.
    ERROR,

    // Declarations
    /// `@name` or `@name(args)`. Its first token is an `ANNOTATION`.
    ANNOTATION_NODE,
    /// The `(args)` of an annotation.
    ANNOTATION_ARGS,
    CLASS_NAME,
    EXTENDS,
    /// An inner class: `class Name extends Base:` and its body.
    CLASS,
    CLASS_BODY,
    /// A string alone on a line at class level, used as a comment.
    STRING_STATEMENT,
    VAR,
    CONST,
    SIGNAL,
    ENUM,
    ENUM_VALUE,
    FUNC,
    PARAM_LIST,
    PARAM,
    /// `-> Type`
    RETURN_TYPE,
    /// `int`, `Node.Child`, `Array[int]`, `Dictionary[String, int]`.
    TYPE,
    /// The `[...]` of a typed collection.
    TYPE_ARGS,
    /// The `get`/`set` part of a variable after its second `:`.
    PROPERTY,
    SETTER,
    GETTER,

    // Statements
    /// A block: one inline statement, or a line break, an indented list of
    /// statements and the dedent.
    SUITE,
    PASS_STMT,
    RETURN_STMT,
    BREAK_STMT,
    CONTINUE_STMT,
    BREAKPOINT_STMT,
    EXPR_STMT,
    IF_STMT,
    ELIF_CLAUSE,
    ELSE_CLAUSE,
    FOR_STMT,
    WHILE_STMT,
    MATCH_STMT,
    MATCH_BRANCH,
    /// `when condition` after match patterns.
    PATTERN_GUARD,
    ASSERT_STMT,

    // Match patterns
    BIND_PATTERN,
    WILDCARD_PATTERN,
    REST_PATTERN,
    ARRAY_PATTERN,
    DICT_PATTERN,
    DICT_PATTERN_ENTRY,
    EXPR_PATTERN,

    // Expressions
    LITERAL,
    NAME_REF,
    SELF_EXPR,
    UNARY_EXPR,
    BINARY_EXPR,
    /// `a if condition else b`
    TERNARY_EXPR,
    /// `=` and the compound assignments.
    ASSIGNMENT,
    AWAIT_EXPR,
    ARRAY,
    DICT,
    DICT_ENTRY,
    /// `(expression)`
    PAREN_EXPR,
    /// `base.name`
    ATTRIBUTE,
    /// `base[index]`
    SUBSCRIPT,
    /// `value as Type`
    CAST,
    /// `callee(args)`, `super(args)` and `super.name(args)`.
    CALL,
    ARG_LIST,
    /// `$Path`, `$"Path"`, `%Unique`, `$%A/%B`.
    GET_NODE,
    PRELOAD,
    LAMBDA,
    /// `value is Type` and `value is not Type`.
    TYPE_TEST,
}

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::WHITESPACE
                | SyntaxKind::COMMENT
                | SyntaxKind::DOC_COMMENT
                | SyntaxKind::LINE_BREAK
                | SyntaxKind::LINE_CONTINUATION
                | SyntaxKind::BYTE_ORDER_MARK
        )
    }

    pub fn is_token(self) -> bool {
        self < SyntaxKind::SOURCE_FILE
    }

    /// Godot's `LITERAL` token type.
    pub fn is_literal(self) -> bool {
        matches!(
            self,
            SyntaxKind::INT
                | SyntaxKind::FLOAT
                | SyntaxKind::STRING
                | SyntaxKind::STRING_NAME
                | SyntaxKind::NODE_PATH
                | SyntaxKind::TRUE_KW
                | SyntaxKind::FALSE_KW
                | SyntaxKind::NULL_KW
        )
    }

    /// Godot's `Token::can_precede_bin_op`: whether a `+` or `-` after this
    /// token is an operator rather than the sign of a number.
    pub fn can_precede_binary_operator(self) -> bool {
        self.is_literal()
            || matches!(
                self,
                SyntaxKind::IDENTIFIER
                    | SyntaxKind::SELF_KW
                    | SyntaxKind::BRACKET_CLOSE
                    | SyntaxKind::BRACE_CLOSE
                    | SyntaxKind::PARENTHESIS_CLOSE
                    | SyntaxKind::CONST_PI
                    | SyntaxKind::CONST_TAU
                    | SyntaxKind::CONST_INF
                    | SyntaxKind::CONST_NAN
            )
    }

    pub fn from_raw(raw: u16) -> Option<SyntaxKind> {
        ALL.get(raw as usize).copied()
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind as u16)
    }
}

/// The rowan language tag for GDScript trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GDScript {}

impl rowan::Language for GDScript {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        SyntaxKind::from_raw(raw.0).expect("a tree only holds kinds gdsyntax created")
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_values_round_trip() {
        for (i, kind) in ALL.iter().enumerate() {
            assert_eq!(*kind as usize, i);
            assert_eq!(SyntaxKind::from_raw(i as u16), Some(*kind));
        }
    }
}

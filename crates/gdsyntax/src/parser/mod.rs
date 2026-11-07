//! A port of Godot 4.7's GDScript parser (`GDScriptParser` in
//! `modules/gdscript/gdscript_parser.cpp`) that builds a lossless tree.
//!
//! Functions keep Godot's names (`parse_class_body`, `parse_statement`,
//! `parse_precedence`, ...) and follow its control flow, so a change in Godot
//! maps to one change here. What differs:
//!
//! - **The output is a concrete syntax tree** ([`rowan`]) holding every token,
//!   trivia included, instead of Godot's abstract syntax tree.
//! - **Only syntax is checked.** Godot's parser also tracks scopes and reports
//!   names declared twice, for example. Those checks, and every warning, are
//!   left to Godot itself (gdtypecheck). `tests/godot/semantic-errors.txt`
//!   lists the parser errors this leaves out.
//! - **Nesting is bounded**, so deeply nested input reports an error instead
//!   of overflowing the stack.

mod class;
mod expr;
mod stmt;

use rowan::{Checkpoint, GreenNode, GreenNodeBuilder};

use crate::SyntaxKind::{self, *};
use crate::lexer::{Lexer, Token};

/// An error found while parsing, with Godot's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    /// Byte offsets of the error in the source.
    pub start: usize,
    pub end: usize,
}

/// A parsed file: its lossless tree and the errors found, in the order Godot
/// reports them.
#[derive(Debug, Clone)]
pub struct Parse {
    pub green: GreenNode,
    pub errors: Vec<ParseError>,
}

/// Parse GDScript source. Parsing never fails: code that is not valid
/// GDScript ends up in the tree all the same, with errors.
pub fn parse(source: &str) -> Parse {
    let mut parser = Parser::new(source);
    parser.parse();
    parser.finish()
}

/// Godot's `AnnotationInfo::TargetKind`.
pub(crate) mod target {
    pub const SCRIPT: u32 = 1 << 0;
    pub const CLASS: u32 = 1 << 1;
    pub const VARIABLE: u32 = 1 << 2;
    pub const CONSTANT: u32 = 1 << 3;
    pub const SIGNAL: u32 = 1 << 4;
    pub const FUNCTION: u32 = 1 << 5;
    pub const STATEMENT: u32 = 1 << 6;
    pub const STANDALONE: u32 = 1 << 7;
    pub const CLASS_LEVEL: u32 = CLASS | VARIABLE | CONSTANT | SIGNAL | FUNCTION;
}

/// The annotations Godot 4.7 registers, with what each may apply to.
fn annotation_targets(name: &str) -> Option<u32> {
    use target::*;
    Some(match name {
        "@tool" | "@icon" | "@static_unload" => SCRIPT,
        "@abstract" => SCRIPT | CLASS | FUNCTION,
        "@onready"
        | "@export"
        | "@export_enum"
        | "@export_file"
        | "@export_file_path"
        | "@export_dir"
        | "@export_global_file"
        | "@export_global_dir"
        | "@export_multiline"
        | "@export_placeholder"
        | "@export_range"
        | "@export_exp_easing"
        | "@export_color_no_alpha"
        | "@export_node_path"
        | "@export_flags"
        | "@export_flags_2d_render"
        | "@export_flags_2d_physics"
        | "@export_flags_2d_navigation"
        | "@export_flags_3d_render"
        | "@export_flags_3d_physics"
        | "@export_flags_3d_navigation"
        | "@export_flags_avoidance"
        | "@export_storage"
        | "@export_custom"
        | "@export_tool_button" => VARIABLE,
        "@export_category" | "@export_group" | "@export_subgroup" => STANDALONE,
        "@warning_ignore" => CLASS_LEVEL | STATEMENT,
        "@warning_ignore_start" | "@warning_ignore_restore" => STANDALONE,
        "@rpc" => FUNCTION,
        _ => return None,
    })
}

/// An annotation read but not yet attached to what follows it.
#[derive(Clone)]
pub(crate) struct PendingAnnotation {
    name: String,
    targets: u32,
    checkpoint: Checkpoint,
    /// The node the annotation was read in; it can only attach to a
    /// declaration in the same node.
    parent: u32,
    start: usize,
    end: usize,
}

/// Nesting deeper than this is reported instead of followed.
const MAX_DEPTH: u32 = 256;

pub(crate) struct Parser<'s> {
    src: &'s str,
    lexer: Lexer<'s>,
    /// Tokens read but not yet in the tree: trivia, then the current token.
    buf: Vec<Token>,
    /// Byte offset of `buf[0]`.
    buf_start: usize,
    current: SyntaxKind,
    current_start: usize,
    current_end: usize,
    previous: SyntaxKind,
    previous_start: usize,
    previous_end: usize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
    lexer_errors_seen: usize,
    /// Tokenizer errors Godot reports at the next token.
    deferred_lexer_errors: Vec<ParseError>,
    multiline_stack: Vec<bool>,
    panic_mode: bool,
    lambda_ended: bool,
    in_lambda: bool,
    can_break: bool,
    can_continue: bool,
    /// Godot's `current_function`: whether we are inside a function, and
    /// whether it is static.
    function: Option<FunctionContext>,
    annotation_stack: Vec<PendingAnnotation>,
    /// Where a `static` before a declaration was read, and in which node.
    static_checkpoint: Option<(Checkpoint, u32)>,
    /// Godot's `current_class->extends_used`.
    class_extends_used: bool,
    /// Ids of the open nodes, innermost last.
    open_nodes: Vec<u32>,
    next_node_id: u32,
    depth: u32,
    too_deep_reported: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct FunctionContext {
    is_static: bool,
    /// `_init` or `_static_init`, which cannot return a value.
    is_constructor: bool,
}

impl<'s> Parser<'s> {
    fn new(src: &'s str) -> Self {
        Parser {
            src,
            lexer: Lexer::new(src),
            buf: Vec::new(),
            buf_start: 0,
            current: EOF,
            current_start: 0,
            current_end: 0,
            previous: EOF,
            previous_start: 0,
            previous_end: 0,
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),
            lexer_errors_seen: 0,
            deferred_lexer_errors: Vec::new(),
            multiline_stack: Vec::new(),
            panic_mode: false,
            lambda_ended: false,
            in_lambda: false,
            can_break: false,
            can_continue: false,
            function: None,
            annotation_stack: Vec::new(),
            static_checkpoint: None,
            class_extends_used: false,
            open_nodes: Vec::new(),
            next_node_id: 0,
            depth: 0,
            too_deep_reported: false,
        }
    }

    /// Godot's `parse`.
    fn parse(&mut self) {
        self.open_node();
        self.builder.start_node(SOURCE_FILE.into());
        self.scan();
        // Leading line breaks are not statements.
        while self.current == NEWLINE {
            self.demote_current();
        }
        self.push_multiline(false); // Keep one for the whole parsing.
        self.parse_program();
        self.pop_multiline();
        // Whatever is left (trivia, and tokens after an error) stays in the tree.
        while self.current != EOF {
            self.advance();
        }
        self.flush_before_current();
        self.builder.finish_node();
    }

    fn finish(mut self) -> Parse {
        self.collect_lexer_errors();
        self.errors.append(&mut self.deferred_lexer_errors);
        let builder = std::mem::replace(&mut self.builder, GreenNodeBuilder::new());
        Parse {
            green: builder.finish(),
            errors: self.errors,
        }
    }

    // ---- Tokens ----

    /// `tokenizer->scan()`: read the next significant token into `current`,
    /// skipping error tokens as Godot's `advance` does.
    fn scan(&mut self) {
        for error in std::mem::take(&mut self.deferred_lexer_errors) {
            self.panic_mode = true;
            self.errors.push(error);
        }
        loop {
            let kind = self.lexer.scan(&mut self.buf);
            self.collect_lexer_errors();
            if kind == ERROR_TOKEN {
                continue;
            }
            let buffered: usize = self.buf.iter().map(|t| t.len as usize).sum();
            let end = self.buf_start + buffered;
            self.current = kind;
            if kind == EOF {
                self.current_start = end;
                self.current_end = end;
            } else {
                let len = self.buf.last().map_or(0, |t| t.len as usize);
                self.current_start = end - len;
                self.current_end = end;
            }
            return;
        }
    }

    fn collect_lexer_errors(&mut self) {
        let new = &self.lexer.errors()[self.lexer_errors_seen..];
        self.lexer_errors_seen += new.len();
        for error in new {
            let error_out = ParseError {
                message: error.message.clone(),
                start: error.start,
                end: error.end,
            };
            if error.deferred {
                self.deferred_lexer_errors.push(error_out);
            } else {
                self.panic_mode = true;
                self.errors.push(error_out);
            }
        }
    }

    /// Put the tokens before the current one into the tree.
    fn flush_before_current(&mut self) {
        let keep = if self.current == EOF { 0 } else { 1 };
        let n = self.buf.len().saturating_sub(keep);
        self.flush(n);
    }

    fn flush(&mut self, n: usize) {
        for token in self.buf.drain(..n) {
            let end = self.buf_start + token.len as usize;
            self.builder
                .token(token.kind.into(), &self.src[self.buf_start..end]);
            self.buf_start = end;
        }
    }

    /// Godot's `advance`: the current token goes into the tree and the next
    /// one is read.
    fn advance(&mut self) -> SyntaxKind {
        self.lambda_ended = false;
        if self.current == EOF {
            return EOF;
        }
        let n = self.buf.len();
        self.flush(n);
        self.previous = self.current;
        self.previous_start = self.current_start;
        self.previous_end = self.current_end;
        self.scan();
        self.previous
    }

    /// Replace a `NEWLINE`, `INDENT` or `DEDENT` the parser decides to skip
    /// (Godot's `current = tokenizer->scan()`): a line break becomes trivia
    /// and a zero-width token disappears.
    fn demote_current(&mut self) {
        if self.current == EOF {
            return;
        }
        if let Some(last) = self.buf.last_mut() {
            if last.len == 0 {
                self.buf.pop();
            } else {
                last.kind = LINE_BREAK;
            }
        }
        self.scan();
    }

    /// Godot's `check`. `IDENTIFIER` also matches the keywords Godot accepts
    /// as names.
    fn check(&self, kind: SyntaxKind) -> bool {
        match kind {
            IDENTIFIER => is_identifier(self.current),
            _ => self.current == kind,
        }
    }

    fn check_literal(&self) -> bool {
        self.current.is_literal()
    }

    /// Godot's `match`.
    fn eat(&mut self, kind: SyntaxKind) -> bool {
        if !self.check(kind) {
            return false;
        }
        self.advance();
        true
    }

    /// Godot's `consume`.
    fn consume(&mut self, kind: SyntaxKind, message: &str) -> bool {
        if self.eat(kind) {
            return true;
        }
        self.push_error(message);
        false
    }

    fn is_at_end(&self) -> bool {
        self.current == EOF
    }

    /// The source text of the current token.
    fn current_text(&self) -> &'s str {
        &self.src[self.current_start..self.current_end]
    }

    fn previous_text(&self) -> &'s str {
        &self.src[self.previous_start..self.previous_end]
    }

    // ---- Errors ----

    /// Godot's `push_error`: at the previous token.
    fn push_error(&mut self, message: impl Into<String>) {
        let (start, end) = (self.previous_start, self.previous_end);
        self.push_error_at(message, start, end);
    }

    fn push_error_at(&mut self, message: impl Into<String>, start: usize, end: usize) {
        self.panic_mode = true;
        self.errors.push(ParseError {
            message: message.into(),
            start,
            end,
        });
    }

    /// Godot's `synchronize`: after an error, skip to the next likely start
    /// of a statement or declaration.
    fn synchronize(&mut self) {
        self.panic_mode = false;
        while !self.is_at_end() {
            if matches!(self.previous, NEWLINE | SEMICOLON) {
                return;
            }
            if matches!(
                self.current,
                CLASS_KW
                    | FUNC_KW
                    | STATIC_KW
                    | VAR_KW
                    | CONST_KW
                    | SIGNAL_KW
                    | FOR_KW
                    | WHILE_KW
                    | MATCH_KW
                    | RETURN_KW
                    | ANNOTATION
            ) {
                return;
            }
            self.advance();
        }
    }

    // ---- Modes ----

    /// Godot's `push_multiline`.
    fn push_multiline(&mut self, state: bool) {
        self.multiline_stack.push(state);
        self.lexer.set_multiline_mode(state);
        if state {
            // Skip whitespace tokens already waiting.
            while matches!(self.current, NEWLINE | INDENT | DEDENT) {
                self.demote_current();
            }
        }
    }

    /// Godot's `pop_multiline`.
    fn pop_multiline(&mut self) {
        self.multiline_stack.pop();
        let state = self.multiline_stack.last().copied().unwrap_or(false);
        self.lexer.set_multiline_mode(state);
    }

    fn is_statement_end_token(&self) -> bool {
        matches!(self.current, NEWLINE | SEMICOLON | EOF)
    }

    fn is_statement_end(&self) -> bool {
        self.lambda_ended || self.in_lambda || self.is_statement_end_token()
    }

    /// Godot's `end_statement`.
    fn end_statement(&mut self, context: &str) {
        let mut found = false;
        while self.is_statement_end() && !self.is_at_end() {
            if self.is_statement_end_token() {
                self.advance();
            } else if self.lambda_ended {
                self.lambda_ended = false;
                found = true;
                break;
            } else {
                if !found {
                    self.lambda_ended = true;
                    found = true;
                }
                break;
            }
            found = true;
        }
        if !found && !self.is_at_end() {
            let name = token_name(self.current);
            let (start, end) = (self.previous_start, self.previous_end);
            self.push_error_at(
                format!("Expected end of statement after {context}, found \"{name}\" instead."),
                start,
                end,
            );
        }
    }

    // ---- Tree ----

    fn open_node(&mut self) {
        self.next_node_id += 1;
        self.open_nodes.push(self.next_node_id);
    }

    /// The id of the innermost open node.
    fn node_id(&self) -> u32 {
        self.open_nodes.last().copied().unwrap_or(0)
    }

    fn start(&mut self, kind: SyntaxKind) {
        self.flush_before_current();
        self.open_node();
        self.builder.start_node(kind.into());
    }

    fn checkpoint(&mut self) -> Checkpoint {
        self.flush_before_current();
        self.builder.checkpoint()
    }

    fn start_at(&mut self, checkpoint: Checkpoint, kind: SyntaxKind) {
        self.flush_before_current();
        self.open_node();
        self.builder.start_node_at(checkpoint, kind.into());
    }

    fn finish_node(&mut self) {
        self.open_nodes.pop();
        self.builder.finish_node();
    }

    /// Enter one level of nesting. Returns false, after reporting it once,
    /// when the input is nested too deeply to follow.
    fn enter(&mut self) -> bool {
        if self.depth >= MAX_DEPTH {
            if !self.too_deep_reported {
                self.too_deep_reported = true;
                let (start, end) = (self.current_start, self.current_end);
                self.push_error_at("The code is nested too deeply to parse.", start, end);
            }
            return false;
        }
        self.depth += 1;
        true
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }
}

/// Godot's `Token::is_identifier`: names, and the keywords Godot still
/// accepts as names.
pub(crate) fn is_identifier(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        IDENTIFIER | MATCH_KW | WHEN_KW | CONST_PI | CONST_INF | CONST_NAN | CONST_TAU
    )
}

/// Godot's `Token::is_node_name`: what may follow `$` or `.`.
pub(crate) fn is_node_name(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        IDENTIFIER
            | AND_KW
            | AS_KW
            | ASSERT_KW
            | AWAIT_KW
            | BREAK_KW
            | BREAKPOINT_KW
            | CLASS_NAME_KW
            | CLASS_KW
            | CONST_KW
            | CONST_PI
            | CONST_INF
            | CONST_NAN
            | CONST_TAU
            | CONTINUE_KW
            | ELIF_KW
            | ELSE_KW
            | ENUM_KW
            | EXTENDS_KW
            | FOR_KW
            | FUNC_KW
            | IF_KW
            | IN_KW
            | IS_KW
            | MATCH_KW
            | NAMESPACE_KW
            | NOT_KW
            | OR_KW
            | PASS_KW
            | PRELOAD_KW
            | RETURN_KW
            | SELF_KW
            | SIGNAL_KW
            | STATIC_KW
            | SUPER_KW
            | TRAIT_KW
            | UNDERSCORE
            | VAR_KW
            | VOID_KW
            | WHILE_KW
            | WHEN_KW
            | YIELD_KW
    )
}

/// Godot's `Token::get_name`, used in error messages.
pub(crate) fn token_name(kind: SyntaxKind) -> &'static str {
    match kind {
        ANNOTATION => "Annotation",
        IDENTIFIER => "Identifier",
        INT | FLOAT | STRING | STRING_NAME | NODE_PATH | TRUE_KW | FALSE_KW | NULL_KW => "Literal",
        LESS => "<",
        LESS_EQUAL => "<=",
        GREATER => ">",
        GREATER_EQUAL => ">=",
        EQUAL_EQUAL => "==",
        BANG_EQUAL => "!=",
        AND_KW => "and",
        OR_KW => "or",
        NOT_KW => "not",
        AMPERSAND_AMPERSAND => "&&",
        PIPE_PIPE => "||",
        BANG => "!",
        AMPERSAND => "&",
        PIPE => "|",
        TILDE => "~",
        CARET => "^",
        LESS_LESS => "<<",
        GREATER_GREATER => ">>",
        PLUS => "+",
        MINUS => "-",
        STAR => "*",
        STAR_STAR => "**",
        SLASH => "/",
        PERCENT => "%",
        EQUAL => "=",
        PLUS_EQUAL => "+=",
        MINUS_EQUAL => "-=",
        STAR_EQUAL => "*=",
        STAR_STAR_EQUAL => "**=",
        SLASH_EQUAL => "/=",
        PERCENT_EQUAL => "%=",
        LESS_LESS_EQUAL => "<<=",
        GREATER_GREATER_EQUAL => ">>=",
        AMPERSAND_EQUAL => "&=",
        PIPE_EQUAL => "|=",
        CARET_EQUAL => "^=",
        IF_KW => "if",
        ELIF_KW => "elif",
        ELSE_KW => "else",
        FOR_KW => "for",
        WHILE_KW => "while",
        BREAK_KW => "break",
        CONTINUE_KW => "continue",
        PASS_KW => "pass",
        RETURN_KW => "return",
        MATCH_KW => "match",
        WHEN_KW => "when",
        AS_KW => "as",
        ASSERT_KW => "assert",
        AWAIT_KW => "await",
        BREAKPOINT_KW => "breakpoint",
        CLASS_KW => "class",
        CLASS_NAME_KW => "class_name",
        CONST_KW => "const",
        ENUM_KW => "enum",
        EXTENDS_KW => "extends",
        FUNC_KW => "func",
        IN_KW => "in",
        IS_KW => "is",
        NAMESPACE_KW => "namespace",
        PRELOAD_KW => "preload",
        SELF_KW => "self",
        SIGNAL_KW => "signal",
        STATIC_KW => "static",
        SUPER_KW => "super",
        TRAIT_KW => "trait",
        VAR_KW => "var",
        VOID_KW => "void",
        YIELD_KW => "yield",
        BRACKET_OPEN => "[",
        BRACKET_CLOSE => "]",
        BRACE_OPEN => "{",
        BRACE_CLOSE => "}",
        PARENTHESIS_OPEN => "(",
        PARENTHESIS_CLOSE => ")",
        COMMA => ",",
        SEMICOLON => ";",
        PERIOD => ".",
        PERIOD_PERIOD => "..",
        PERIOD_PERIOD_PERIOD => "...",
        COLON => ":",
        DOLLAR => "$",
        FORWARD_ARROW => "->",
        UNDERSCORE => "_",
        NEWLINE => "Newline",
        INDENT => "Indent",
        DEDENT => "Dedent",
        CONST_PI => "PI",
        CONST_TAU => "TAU",
        CONST_INF => "INF",
        CONST_NAN => "NaN",
        VCS_CONFLICT_MARKER => "VCS conflict marker",
        BACKTICK => "`",
        QUESTION_MARK => "?",
        ERROR_TOKEN => "Error",
        EOF => "End of file",
        _ => "Empty",
    }
}

#[cfg(test)]
mod tests;

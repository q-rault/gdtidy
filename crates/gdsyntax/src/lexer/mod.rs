//! A port of Godot 4.7's GDScript tokenizer (`GDScriptTokenizerText` in
//! `modules/gdscript/gdscript_tokenizer.cpp`) that keeps every byte.
//!
//! Godot's tokenizer skips whitespace and comments; this one returns them as
//! trivia tokens, so the tokens of a file always add up to its exact text.
//! Everything else follows Godot: which line breaks end a statement, when
//! indentation changes, how numbers and strings are read, and the error
//! messages. Function names match Godot's so a change there maps to one here.
//!
//! Like Godot's, the tokenizer is driven by the parser. Inside brackets the
//! parser turns on *multiline mode*, where line breaks and indentation are
//! ignored, and a lambda inside brackets gets its own indentation stack.
//! [`tokenize`] approximates this on its own by turning multiline mode on
//! inside brackets, which is what Godot's parser does outside lambdas.
//!
//! Not ported: the check for identifiers that look like keywords
//! ("visually similar to the GDScript keyword"), which needs ICU's
//! confusable tables, and Godot's handling of a carriage return alone at the
//! start of a line, which reports the same error but counts indentation
//! slightly differently.

mod unicode;

use crate::SyntaxKind::{self, *};

/// A token: its kind and its length in bytes. Its text is the next `len`
/// bytes of the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: SyntaxKind,
    pub len: u32,
}

/// An error found while reading tokens, with Godot's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    /// Byte offsets of the error in the source.
    pub start: usize,
    pub end: usize,
    /// Godot keeps this error back until the parser reads the next token
    /// (its tokenizer's `error_stack`), which decides the order errors are
    /// reported in.
    pub deferred: bool,
}

/// Godot counts a tab as this many columns when comparing indentation. The
/// editor setting can change it; the command line and the test runner use 4.
const TAB_SIZE: u32 = 4;

/// Where the next `NEWLINE` token comes from.
#[derive(Debug, Clone, Copy)]
enum PendingNewline {
    /// The line break at this index of the output.
    At(usize),
    /// The end of the file, which ends a statement even without a line break.
    EndOfFile,
}

pub struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    /// Start of the token being read (Godot's `_start`).
    token_start: usize,
    errors: Vec<LexError>,
    multiline_mode: bool,
    line_continuation: bool,
    pending_newline: Option<PendingNewline>,
    pending_indents: i32,
    indent_stack: Vec<u32>,
    indent_stack_stack: Vec<Vec<u32>>,
    indent_char: Option<char>,
    paren_stack: Vec<char>,
    /// Kind of the last token made (Godot's `last_token`), which decides
    /// whether `-1` is a number or a minus followed by one.
    last_token: SyntaxKind,
    /// Byte length of a byte order mark at the start, if there is one.
    bom_len: usize,
    reached_end: bool,
    /// Whether errors recorded now are held back (see [`LexError::deferred`]).
    defer_errors: bool,
    /// Index in the output of the token the last `scan` returned.
    significant_index: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        let bom_len = if src.starts_with('\u{feff}') { 3 } else { 0 };
        Lexer {
            src,
            pos: 0,
            token_start: 0,
            errors: Vec::new(),
            multiline_mode: false,
            line_continuation: false,
            pending_newline: None,
            pending_indents: 0,
            indent_stack: Vec::new(),
            indent_stack_stack: Vec::new(),
            indent_char: None,
            paren_stack: Vec::new(),
            last_token: EOF,
            bom_len,
            reached_end: src.len() == bom_len,
            defer_errors: false,
            significant_index: 0,
        }
    }

    pub fn errors(&self) -> &[LexError] {
        &self.errors
    }

    pub fn into_errors(self) -> Vec<LexError> {
        self.errors
    }

    /// Godot's `set_multiline_mode`: inside brackets, line breaks and
    /// indentation don't count.
    pub fn set_multiline_mode(&mut self, state: bool) {
        self.multiline_mode = state;
    }

    /// Godot's `push_expression_indented_block`, used when a lambda body
    /// starts inside brackets.
    pub fn push_expression_indented_block(&mut self) {
        self.indent_stack_stack.push(self.indent_stack.clone());
    }

    /// Godot's `pop_expression_indented_block`.
    pub fn pop_expression_indented_block(&mut self) {
        if let Some(stack) = self.indent_stack_stack.pop() {
            self.indent_stack = stack;
        }
    }

    /// Where in `out` the token returned by the last [`Lexer::scan`] is.
    /// Usually the last token, but a `NEWLINE` can be followed by the blank
    /// and comment lines Godot reads past before returning it. At the end of
    /// the input it is `out.len()`.
    pub fn significant_index(&self) -> usize {
        self.significant_index
    }

    /// Godot's `scan`: read the next token the parser sees, appending it to
    /// `out` with the trivia around it, and return its kind. At the end of
    /// the input, appends any remaining trivia and returns [`SyntaxKind::EOF`].
    pub fn scan(&mut self, out: &mut Vec<Token>) -> SyntaxKind {
        let kind = self.scan_token(out);
        if kind == EOF {
            self.significant_index = out.len();
        }
        kind
    }

    fn scan_token(&mut self, out: &mut Vec<Token>) -> SyntaxKind {
        if self.pos == 0 && self.bom_len > 0 {
            self.pos = self.bom_len;
            out.push(Token {
                kind: BYTE_ORDER_MARK,
                len: self.bom_len as u32,
            });
        }
        loop {
            let errors_before = self.errors.len();
            self.skip_whitespace(out);

            if let Some(pending) = self.pending_newline.take()
                && !self.multiline_mode
            {
                // Godot returns the line break before the errors found
                // while skipping whitespace.
                for error in &mut self.errors[errors_before..] {
                    error.deferred = true;
                }
                // In multiline mode the line break stays trivia.
                match pending {
                    PendingNewline::At(index) => {
                        out[index].kind = NEWLINE;
                        self.significant_index = index;
                    }
                    PendingNewline::EndOfFile => {
                        self.significant_index = out.len();
                        out.push(Token {
                            kind: NEWLINE,
                            len: 0,
                        });
                    }
                }
                return NEWLINE;
            }

            if self.pending_indents != 0 {
                let kind = if self.pending_indents > 0 {
                    self.pending_indents -= 1;
                    INDENT
                } else {
                    self.pending_indents += 1;
                    DEDENT
                };
                self.significant_index = out.len();
                out.push(Token { kind, len: 0 });
                self.last_token = kind;
                return kind;
            }

            if self.at_end() {
                return EOF;
            }

            self.token_start = self.pos;
            let c = self.advance();

            if c == '\\' {
                // Line continuation with a backslash.
                if self.peek() == '\r' && self.peek_at(1) != '\n' {
                    return self.error_token(out, "Unexpected carriage return character.");
                }
                if !self.at_line_break() {
                    return self.error_token(out, "Expected new line after \"\\\".");
                }
                out.push(Token {
                    kind: LINE_CONTINUATION,
                    len: 1,
                });
                self.line_break(out, false);
                self.line_continuation = true;
                // Skip whitespace and comment lines after `\` (Godot GH-89403),
                // then read the next token.
                self.skip_whitespace(out);
                continue;
            }

            self.line_continuation = false;
            self.defer_errors = true;
            let kind = self.token(c);
            self.defer_errors = false;
            if kind == ERROR_TOKEN {
                return self.finish_error_token(out);
            }
            self.significant_index = out.len();
            out.push(Token {
                kind,
                len: (self.pos - self.token_start) as u32,
            });
            self.last_token = kind;
            return kind;
        }
    }

    // ---- Characters ----

    fn at_end(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn peek(&self) -> char {
        self.src[self.pos..].chars().next().unwrap_or('\0')
    }

    fn peek_at(&self, offset: usize) -> char {
        self.src[self.pos..].chars().nth(offset).unwrap_or('\0')
    }

    fn previous(&self) -> char {
        self.src[..self.pos].chars().next_back().unwrap_or('\0')
    }

    fn at_line_break(&self) -> bool {
        let rest = &self.src.as_bytes()[self.pos..];
        rest.starts_with(b"\n") || rest.starts_with(b"\r\n")
    }

    /// Godot's `column == 1`.
    fn at_line_start(&self) -> bool {
        self.pos == self.bom_len || self.src.as_bytes()[self.pos - 1] == b'\n'
    }

    /// Godot's `_advance`. Reaching the end of the input adds the line break
    /// and dedents the parser needs, as Godot does.
    fn advance(&mut self) -> char {
        let Some(c) = self.src[self.pos..].chars().next() else {
            return '\0';
        };
        self.pos += c.len_utf8();
        self.after_advance();
        c
    }

    fn after_advance(&mut self) {
        if self.at_end() && !self.reached_end {
            self.reached_end = true;
            self.request_newline(PendingNewline::EndOfFile);
            self.dedent_all();
        }
    }

    /// Godot's `newline(true)`.
    fn request_newline(&mut self, at: PendingNewline) {
        if self.pending_newline.is_none() && !self.line_continuation {
            self.pending_newline = Some(at);
            self.last_token = NEWLINE;
        }
    }

    fn dedent_all(&mut self) {
        self.pending_indents -= self.indent_stack.len() as i32;
        self.indent_stack.clear();
    }

    // ---- Errors ----

    fn error(&mut self, message: impl Into<String>, start: usize, end: usize) {
        self.errors.push(LexError {
            message: message.into(),
            start,
            end: end.max(start),
            deferred: self.defer_errors,
        });
    }

    /// An error whose position is the current token (Godot's `push_error`
    /// with a message).
    fn token_error(&mut self, message: impl Into<String>) {
        let (start, end) = (self.token_start, self.pos);
        self.error(message, start, end);
    }

    /// An error at the next character (Godot's errors that set
    /// `start_column = column`).
    fn error_here(&mut self, message: impl Into<String>) {
        let end = self.pos + self.peek().len_utf8();
        self.error(message, self.pos, end);
    }

    /// Godot's `make_error`: the text read so far becomes an error token.
    fn error_token(&mut self, out: &mut Vec<Token>, message: &str) -> SyntaxKind {
        self.token_error(message);
        self.finish_error_token(out)
    }

    fn finish_error_token(&mut self, out: &mut Vec<Token>) -> SyntaxKind {
        // The error that made this an error token is reported right away.
        if let Some(error) = self.errors.last_mut() {
            error.deferred = false;
        }
        self.significant_index = out.len();
        out.push(Token {
            kind: ERROR_TOKEN,
            len: (self.pos - self.token_start) as u32,
        });
        self.last_token = ERROR_TOKEN;
        ERROR_TOKEN
    }

    // ---- Trivia and indentation ----

    /// One line break, `\n` or `\r\n`. `make_newline` is Godot's
    /// `newline(true)`: the line break may end a statement.
    fn line_break(&mut self, out: &mut Vec<Token>, make_newline: bool) {
        let index = out.len();
        let len = if self.src.as_bytes()[self.pos] == b'\r' {
            2
        } else {
            1
        };
        let had_pending = self.pending_newline.is_some();
        self.pos += len;
        out.push(Token {
            kind: LINE_BREAK,
            len: len as u32,
        });
        self.after_advance();
        if !had_pending && self.pending_newline.is_some() {
            // The end of the file came right after this line break.
            self.pending_newline = Some(PendingNewline::At(index));
        }
        if make_newline {
            self.request_newline(PendingNewline::At(index));
        }
    }

    fn whitespace(&mut self, out: &mut Vec<Token>) -> (u32, bool, char) {
        let start = self.pos;
        let first = self.peek();
        let (mut count, mut mixed) = (0, false);
        while !self.at_end() {
            let c = self.peek();
            match c {
                '\t' => count += TAB_SIZE,
                ' ' => count += 1,
                _ => break,
            }
            mixed |= c != first;
            self.pos += 1;
        }
        if self.pos > start {
            out.push(Token {
                kind: WHITESPACE,
                len: (self.pos - start) as u32,
            });
            self.after_advance();
        }
        (count, mixed, first)
    }

    /// A comment up to the end of the line, without the line break.
    fn comment(&mut self, out: &mut Vec<Token>) {
        let start = self.pos;
        let rest = &self.src[start..];
        let mut len = rest.find('\n').unwrap_or(rest.len());
        if len > 0 && rest.as_bytes()[len - 1] == b'\r' && len < rest.len() {
            len -= 1;
        }
        self.pos += len;
        let kind = if rest.starts_with("##") {
            DOC_COMMENT
        } else {
            COMMENT
        };
        out.push(Token {
            kind,
            len: len as u32,
        });
        self.after_advance();
    }

    /// Godot's `_skip_whitespace`.
    fn skip_whitespace(&mut self, out: &mut Vec<Token>) {
        if self.pending_indents != 0 {
            // Still have indent or dedent tokens to give.
            return;
        }
        if self.at_line_start() {
            self.check_indent(out);
            return;
        }
        loop {
            match self.peek() {
                ' ' | '\t' => {
                    self.whitespace(out);
                }
                '\r' if self.peek_at(1) != '\n' => {
                    let start = self.pos;
                    self.pos += 1;
                    out.push(Token {
                        kind: WHITESPACE,
                        len: 1,
                    });
                    self.after_advance();
                    self.error(
                        "Stray carriage return character in source code.",
                        start,
                        start + 1,
                    );
                    return;
                }
                '\r' | '\n' => {
                    self.line_break(out, true);
                    self.check_indent(out);
                }
                '#' => {
                    self.comment(out);
                    if self.at_end() {
                        return;
                    }
                    self.line_break(out, true);
                    self.check_indent(out);
                }
                _ => return,
            }
        }
    }

    /// Godot's `check_indent`, run at the start of each line.
    fn check_indent(&mut self, out: &mut Vec<Token>) {
        if self.at_end() {
            self.dedent_all();
            return;
        }
        loop {
            let first = self.peek();
            if !matches!(first, ' ' | '\t' | '\r' | '\n' | '#') {
                // The line starts with a token: no indentation at all.
                if self.line_continuation || self.multiline_mode {
                    return;
                }
                self.dedent_all();
                return;
            }
            if first == '\r' && self.peek_at(1) != '\n' {
                let start = self.pos;
                self.pos += 1;
                out.push(Token {
                    kind: WHITESPACE,
                    len: 1,
                });
                self.after_advance();
                self.error(
                    "Stray carriage return character in source code.",
                    start,
                    start + 1,
                );
                if self.at_end() {
                    self.dedent_all();
                    return;
                }
                continue;
            }
            if self.at_line_break() {
                // Empty line.
                self.line_break(out, false);
                continue;
            }

            let line_start = self.pos;
            let (count, mixed, indent_char) = self.whitespace(out);
            if self.at_end() {
                // The file ends with an empty line.
                self.dedent_all();
                return;
            }
            if self.at_line_break() {
                self.line_break(out, false);
                continue;
            }
            if self.peek() == '#' {
                // Comment lines don't change indentation.
                self.comment(out);
                if self.at_end() {
                    self.dedent_all();
                    return;
                }
                self.line_break(out, false);
                continue;
            }

            if mixed && !self.line_continuation && !self.multiline_mode {
                self.error(
                    "Mixed use of tabs and spaces for indentation.",
                    line_start,
                    self.pos,
                );
            }
            if self.line_continuation || self.multiline_mode {
                return;
            }

            match self.indent_char {
                None => self.indent_char = Some(indent_char),
                Some(previous) if previous != indent_char => {
                    let name = |c: char| if c == ' ' { "space" } else { "tab" };
                    self.error(
                        format!(
                            "Used {} character for indentation instead of {} as used before in the file.",
                            name(indent_char),
                            name(previous)
                        ),
                        line_start,
                        self.pos,
                    );
                }
                Some(_) => {}
            }

            let previous_indent = self.indent_stack.last().copied().unwrap_or(0);
            if count == previous_indent {
                return;
            }
            if count > previous_indent {
                self.indent_stack.push(count);
                self.pending_indents += 1;
            } else {
                if self.indent_stack.is_empty() {
                    self.error(
                        "Tokenizer bug: trying to dedent without previous indent.",
                        line_start,
                        self.pos,
                    );
                    return;
                }
                while self.indent_stack.last().is_some_and(|&level| level > count) {
                    self.indent_stack.pop();
                    self.pending_indents -= 1;
                }
                let mismatched = match self.indent_stack.last() {
                    Some(&level) => level != count,
                    None => count != 0,
                };
                if mismatched {
                    self.error(
                        "Unindent doesn't match the previous indentation level.",
                        line_start,
                        self.pos + 1,
                    );
                    // Lenient, as Godot: keep this level.
                    self.indent_stack.push(count);
                }
            }
            return;
        }
    }

    // ---- Tokens ----

    /// Read the token that starts with `c`, already consumed, and return its
    /// kind. Returns `ERROR_TOKEN` after recording the error.
    fn token(&mut self, c: char) -> SyntaxKind {
        if c.is_ascii_digit() {
            return self.number();
        }
        if c == 'r' && matches!(self.peek(), '"' | '\'') {
            return self.string();
        }
        if is_identifier_start(c) {
            return self.potential_identifier();
        }
        match c {
            '"' | '\'' => self.string(),
            '@' => self.annotation(),
            '~' => TILDE,
            ',' => COMMA,
            ':' => COLON,
            ';' => SEMICOLON,
            '$' => DOLLAR,
            '?' => QUESTION_MARK,
            '`' => BACKTICK,
            '(' | '[' | '{' => {
                self.paren_stack.push(c);
                match c {
                    '(' => PARENTHESIS_OPEN,
                    '[' => BRACKET_OPEN,
                    _ => BRACE_OPEN,
                }
            }
            ')' | ']' | '}' => {
                let opening = match c {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                // Godot's `pop_paren` removes the top bracket even when it
                // doesn't match, then `make_paren_error` looks at the next one.
                if self.paren_stack.pop() == Some(opening) {
                    return match c {
                        ')' => PARENTHESIS_CLOSE,
                        ']' => BRACKET_CLOSE,
                        _ => BRACE_CLOSE,
                    };
                }
                match self.paren_stack.pop() {
                    Some(open) => self.token_error(format!(
                        "Closing \"{c}\" doesn't match the opening \"{open}\"."
                    )),
                    None => self.token_error(format!(
                        "Closing \"{c}\" doesn't have an opening counterpart."
                    )),
                }
                ERROR_TOKEN
            }
            '!' => self.two('=', BANG_EQUAL, BANG),
            '.' => {
                if self.peek() == '.' {
                    self.advance();
                    if self.peek() == '.' {
                        self.advance();
                        PERIOD_PERIOD_PERIOD
                    } else {
                        PERIOD_PERIOD
                    }
                } else if self.peek().is_ascii_digit() {
                    self.number()
                } else {
                    PERIOD
                }
            }
            '+' => {
                if self.peek() == '=' {
                    self.advance();
                    PLUS_EQUAL
                } else if self.peek().is_ascii_digit()
                    && !self.last_token.can_precede_binary_operator()
                {
                    self.number()
                } else {
                    PLUS
                }
            }
            '-' => {
                if self.peek() == '=' {
                    self.advance();
                    MINUS_EQUAL
                } else if self.peek().is_ascii_digit()
                    && !self.last_token.can_precede_binary_operator()
                {
                    self.number()
                } else if self.peek() == '>' {
                    self.advance();
                    FORWARD_ARROW
                } else {
                    MINUS
                }
            }
            '*' => {
                if self.peek() == '=' {
                    self.advance();
                    STAR_EQUAL
                } else if self.peek() == '*' {
                    self.advance();
                    self.two('=', STAR_STAR_EQUAL, STAR_STAR)
                } else {
                    STAR
                }
            }
            '/' => self.two('=', SLASH_EQUAL, SLASH),
            '%' => self.two('=', PERCENT_EQUAL, PERCENT),
            '^' => {
                if self.peek() == '=' {
                    self.advance();
                    CARET_EQUAL
                } else if matches!(self.peek(), '"' | '\'') {
                    self.string()
                } else {
                    CARET
                }
            }
            '&' => {
                if self.peek() == '&' {
                    self.advance();
                    AMPERSAND_AMPERSAND
                } else if self.peek() == '=' {
                    self.advance();
                    AMPERSAND_EQUAL
                } else if matches!(self.peek(), '"' | '\'') {
                    self.string()
                } else {
                    AMPERSAND
                }
            }
            '|' => {
                if self.peek() == '|' {
                    self.advance();
                    PIPE_PIPE
                } else {
                    self.two('=', PIPE_EQUAL, PIPE)
                }
            }
            // Potential version control conflict markers.
            '=' => {
                if self.peek() == '=' {
                    self.check_vcs_marker('=', EQUAL_EQUAL)
                } else {
                    EQUAL
                }
            }
            '<' => {
                if self.peek() == '=' {
                    self.advance();
                    LESS_EQUAL
                } else if self.peek() == '<' {
                    if self.peek_at(1) == '=' {
                        self.advance();
                        self.advance();
                        LESS_LESS_EQUAL
                    } else {
                        self.check_vcs_marker('<', LESS_LESS)
                    }
                } else {
                    LESS
                }
            }
            '>' => {
                if self.peek() == '=' {
                    self.advance();
                    GREATER_EQUAL
                } else if self.peek() == '>' {
                    if self.peek_at(1) == '=' {
                        self.advance();
                        self.advance();
                        GREATER_GREATER_EQUAL
                    } else {
                        self.check_vcs_marker('>', GREATER_GREATER)
                    }
                } else {
                    GREATER
                }
            }
            _ => {
                let code = c as u32;
                if is_whitespace(c) {
                    self.token_error(format!("Invalid white space character U+{code:04X}."));
                } else {
                    self.token_error(format!("Invalid character \"{c}\" (U+{code:04X})."));
                }
                ERROR_TOKEN
            }
        }
    }

    /// A one-character token, or a two-character one if `second` follows.
    fn two(&mut self, second: char, double: SyntaxKind, single: SyntaxKind) -> SyntaxKind {
        if self.peek() == second {
            self.advance();
            double
        } else {
            single
        }
    }

    /// Godot's `check_vcs_marker`: seven or more of `test` in a row is a
    /// version control conflict marker.
    fn check_vcs_marker(&mut self, test: char, double: SyntaxKind) -> SyntaxKind {
        let run = self.src[self.pos..]
            .chars()
            .take_while(|&c| c == test)
            .count()
            + 1;
        if run >= 7 {
            for _ in 1..run {
                self.advance();
            }
            VCS_CONFLICT_MARKER
        } else {
            self.advance();
            double
        }
    }

    /// Godot's `annotation`.
    fn annotation(&mut self) -> SyntaxKind {
        if is_identifier_start(self.peek()) {
            self.advance();
        } else {
            self.token_error("Expected annotation identifier after \"@\".");
        }
        while is_identifier_continue(self.peek()) {
            self.advance();
        }
        ANNOTATION
    }

    /// Godot's `potential_identifier`: an identifier, a keyword, or `true`,
    /// `false` and `null`.
    fn potential_identifier(&mut self) -> SyntaxKind {
        while is_identifier_continue(self.peek()) {
            self.advance();
        }
        let name = &self.src[self.token_start..self.pos];
        if name == "_" {
            return UNDERSCORE;
        }
        keyword(name).unwrap_or(IDENTIFIER)
    }

    /// Godot's `number`.
    fn number(&mut self) -> SyntaxKind {
        let mut base = 10;
        let mut has_decimal = false;
        let mut has_exponent = false;
        let mut has_error = false;
        let mut need_digits = false;

        // Sign before a hexadecimal or binary number.
        if matches!(self.previous(), '+' | '-') && self.peek() == '0' {
            self.advance();
        }

        if self.previous() == '.' {
            has_decimal = true;
        } else if self.previous() == '0' {
            match self.peek() {
                'x' | 'X' => {
                    base = 16;
                    need_digits = true;
                    self.advance();
                }
                'b' | 'B' => {
                    base = 2;
                    need_digits = true;
                    self.advance();
                }
                _ => {}
            }
        }
        let is_base_digit = |c: char| match base {
            16 => c.is_ascii_hexdigit(),
            2 => c == '0' || c == '1',
            _ => c.is_ascii_digit(),
        };

        if base != 10 && self.peek() == '_' {
            // Disallow `0x_` and `0b_`.
            let letter = self.previous();
            self.error_here(format!("Unexpected underscore after \"0{letter}\"."));
            has_error = true;
        }
        let mut previous_was_underscore = false;
        while is_base_digit(self.peek()) || self.peek() == '_' {
            if self.peek() == '_' {
                if previous_was_underscore {
                    self.error_here(
                        "Multiple underscores cannot be adjacent in a numeric literal.",
                    );
                }
                previous_was_underscore = true;
            } else {
                need_digits = false;
                previous_was_underscore = false;
            }
            self.advance();
        }

        // It might be a `..` token instead of a decimal point.
        if self.peek() == '.' && self.peek_at(1) != '.' {
            if base == 10 && !has_decimal {
                has_decimal = true;
            } else {
                self.error_here(match base {
                    10 => "Cannot use a decimal point twice in a number.",
                    16 => "Cannot use a decimal point in a hexadecimal number.",
                    _ => "Cannot use a decimal point in a binary number.",
                });
                has_error = true;
            }
            if !has_error {
                self.advance();
                if self.peek() == '_' {
                    // Disallow `10._`, but allow `10.`.
                    self.error_here("Unexpected underscore after decimal point.");
                    has_error = true;
                }
                self.digits_with_underscores();
            }
        }
        if base == 10 && matches!(self.peek(), 'e' | 'E') {
            has_exponent = true;
            self.advance();
            if matches!(self.peek(), '+' | '-') {
                self.advance();
            }
            if !self.peek().is_ascii_digit() {
                self.error_here("Expected exponent value after \"e\".");
            }
            self.digits_with_underscores();
        }

        if need_digits {
            // No digits in a hexadecimal or binary number.
            let (name, letter) = if base == 16 {
                ("hexadecimal", 'x')
            } else {
                ("binary", 'b')
            };
            self.error_here(format!("Expected {name} digit after \"0{letter}\"."));
            return ERROR_TOKEN;
        }

        if !has_error && has_decimal && self.peek() == '.' && self.peek_at(1) != '.' {
            self.error_here("Cannot use a decimal point twice in a number.");
        } else if is_identifier_continue(self.peek()) {
            // A letter at the end of the number.
            self.token_error("Invalid numeric notation.");
        }

        if base == 10 && (has_decimal || has_exponent) {
            FLOAT
        } else {
            INT
        }
    }

    fn digits_with_underscores(&mut self) {
        let mut previous_was_underscore = false;
        while self.peek().is_ascii_digit() || self.peek() == '_' {
            if self.peek() == '_' {
                if previous_was_underscore {
                    self.error_here(
                        "Multiple underscores cannot be adjacent in a numeric literal.",
                    );
                }
                previous_was_underscore = true;
            } else {
                previous_was_underscore = false;
            }
            self.advance();
        }
    }

    /// Godot's `string`: every form of string literal. The opening quote, and
    /// the `r`, `&` or `^` before it, are already consumed except the quote
    /// after a prefix.
    fn string(&mut self) -> SyntaxKind {
        let mut is_raw = false;
        let mut kind = STRING;
        match self.previous() {
            'r' => {
                is_raw = true;
                self.advance();
            }
            '&' => {
                kind = STRING_NAME;
                self.advance();
            }
            '^' => {
                kind = NODE_PATH;
                self.advance();
            }
            _ => {}
        }
        let quote = self.previous();
        let mut is_multiline = false;
        if self.peek() == quote && self.peek_at(1) == quote {
            is_multiline = true;
            self.advance();
            self.advance();
        }

        // A lead surrogate from a `\u` escape waiting for its trail, and
        // where it was.
        let mut lead_surrogate: Option<usize> = None;
        // Godot ends this message with a period only when another escape
        // follows the lead surrogate.
        let unpaired_lead = |lexer: &mut Self, at: usize| {
            lexer.error(
                "Invalid UTF-16 sequence in string, unpaired lead surrogate",
                at,
                at + 2,
            );
        };
        let unpaired_lead_before_escape = |lexer: &mut Self, at: usize| {
            lexer.error(
                "Invalid UTF-16 sequence in string, unpaired lead surrogate.",
                at,
                at + 2,
            );
        };

        loop {
            if self.at_end() {
                self.token_error("Unterminated string.");
                return ERROR_TOKEN;
            }
            let c = self.peek();
            if matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
            {
                if is_raw {
                    self.error_here("Invisible text direction control character present in the string, use regular string literal instead of r-string.");
                } else {
                    let code = c as u32;
                    self.error_here(format!("Invisible text direction control character present in the string, escape it (\"\\u{code:x}\") to avoid confusion."));
                }
            }

            if c == '\\' {
                let escape_start = self.pos;
                self.advance();
                if self.at_end() {
                    self.token_error("Unterminated string.");
                    return ERROR_TOKEN;
                }
                if is_raw {
                    if self.peek() == quote || self.peek() == '\\' {
                        self.advance();
                        if self.at_end() {
                            self.token_error("Unterminated string.");
                            return ERROR_TOKEN;
                        }
                    }
                    continue;
                }
                let code = self.advance();
                if self.at_end() {
                    self.token_error("Unterminated string.");
                    return ERROR_TOKEN;
                }
                let mut escaped: Option<u32> = None;
                match code {
                    'a' | 'b' | 'f' | 'n' | 'r' | 't' | 'v' | '\'' | '"' | '\\' => {
                        escaped = Some(code as u32);
                    }
                    'u' | 'U' => {
                        let digits = if code == 'U' { 6 } else { 4 };
                        let mut value = 0u32;
                        let mut valid = true;
                        for _ in 0..digits {
                            if self.at_end() {
                                self.token_error("Unterminated string.");
                                return ERROR_TOKEN;
                            }
                            match self.peek().to_digit(16) {
                                Some(digit) => {
                                    value = (value << 4) | digit;
                                    self.advance();
                                }
                                None => {
                                    self.error_here(
                                        "Invalid hexadecimal digit in unicode escape sequence.",
                                    );
                                    valid = false;
                                    break;
                                }
                            }
                        }
                        if valid {
                            escaped = Some(value);
                        }
                    }
                    '\r' if self.peek() != '\n' => {
                        // A carriage return alone after `\` is kept as is.
                    }
                    '\r' | '\n' => {
                        // An escaped line break: the string continues.
                        if code == '\r' {
                            self.advance();
                        }
                    }
                    _ => {
                        self.error("Invalid escape in string.", escape_start, self.pos);
                    }
                }
                // Pair UTF-16 surrogates.
                if let Some(value) = escaped {
                    if value & 0xffff_fc00 == 0xd800 {
                        if let Some(at) = lead_surrogate.take() {
                            unpaired_lead_before_escape(self, at);
                        } else {
                            lead_surrogate = Some(escape_start);
                            continue;
                        }
                    } else if value & 0xffff_fc00 == 0xdc00 {
                        if lead_surrogate.take().is_none() {
                            self.error(
                                "Invalid UTF-16 sequence in string, unpaired trail surrogate.",
                                escape_start,
                                escape_start + 2,
                            );
                        }
                    } else if let Some(at) = lead_surrogate.take() {
                        unpaired_lead_before_escape(self, at);
                    }
                }
            } else if c == quote {
                if let Some(at) = lead_surrogate.take() {
                    unpaired_lead(self, at);
                }
                self.advance();
                if !is_multiline {
                    break;
                }
                if self.peek() == quote && self.peek_at(1) == quote {
                    self.advance();
                    self.advance();
                    break;
                }
            } else {
                if let Some(at) = lead_surrogate.take() {
                    unpaired_lead(self, at);
                }
                self.advance();
            }
        }
        if let Some(at) = lead_surrogate {
            unpaired_lead(self, at);
        }
        kind
    }
}

/// Godot's `KEYWORDS` list, plus `true`, `false` and `null`.
fn keyword(name: &str) -> Option<SyntaxKind> {
    Some(match name {
        "as" => AS_KW,
        "and" => AND_KW,
        "assert" => ASSERT_KW,
        "await" => AWAIT_KW,
        "break" => BREAK_KW,
        "breakpoint" => BREAKPOINT_KW,
        "class" => CLASS_KW,
        "class_name" => CLASS_NAME_KW,
        "const" => CONST_KW,
        "continue" => CONTINUE_KW,
        "elif" => ELIF_KW,
        "else" => ELSE_KW,
        "enum" => ENUM_KW,
        "extends" => EXTENDS_KW,
        "for" => FOR_KW,
        "func" => FUNC_KW,
        "if" => IF_KW,
        "in" => IN_KW,
        "is" => IS_KW,
        "match" => MATCH_KW,
        "namespace" => NAMESPACE_KW,
        "not" => NOT_KW,
        "or" => OR_KW,
        "pass" => PASS_KW,
        "preload" => PRELOAD_KW,
        "return" => RETURN_KW,
        "self" => SELF_KW,
        "signal" => SIGNAL_KW,
        "static" => STATIC_KW,
        "super" => SUPER_KW,
        "trait" => TRAIT_KW,
        "var" => VAR_KW,
        "void" => VOID_KW,
        "while" => WHILE_KW,
        "when" => WHEN_KW,
        "yield" => YIELD_KW,
        "INF" => CONST_INF,
        "NAN" => CONST_NAN,
        "PI" => CONST_PI,
        "TAU" => CONST_TAU,
        "true" => TRUE_KW,
        "false" => FALSE_KW,
        "null" => NULL_KW,
        _ => return None,
    })
}

fn in_table(table: &[(u32, u32)], c: char) -> bool {
    let c = c as u32;
    table
        .binary_search_by(|&(low, high)| {
            if high < c {
                std::cmp::Ordering::Less
            } else if low > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Godot's `is_unicode_identifier_start`: XID_Start and `_`.
pub fn is_identifier_start(c: char) -> bool {
    in_table(&unicode::XID_START, c)
}

/// Godot's `is_unicode_identifier_continue`: XID_Continue.
pub fn is_identifier_continue(c: char) -> bool {
    in_table(&unicode::XID_CONTINUE, c)
}

/// Godot's `is_whitespace`.
fn is_whitespace(c: char) -> bool {
    matches!(c as u32,
        0x20 | 0xa0 | 0x1680 | 0x2000..=0x200b | 0x202f | 0x205f | 0x3000 | 0x2028 | 0x2029
        | 0x09..=0x0d | 0x85)
}

/// The tokens of a whole file and the errors found while reading them.
#[derive(Debug, Clone)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub errors: Vec<LexError>,
}

/// Read every token of `src`, turning multiline mode on inside brackets as
/// Godot's parser does outside lambdas. The tokens' lengths add up to
/// `src.len()`.
pub fn tokenize(src: &str) -> Lexed {
    let mut lexer = Lexer::new(src);
    let mut tokens = Vec::new();
    let mut depth = 0usize;
    loop {
        match lexer.scan(&mut tokens) {
            EOF => break,
            PARENTHESIS_OPEN | BRACKET_OPEN | BRACE_OPEN => depth += 1,
            PARENTHESIS_CLOSE | BRACKET_CLOSE | BRACE_CLOSE => depth = depth.saturating_sub(1),
            _ => {}
        }
        lexer.set_multiline_mode(depth > 0);
    }
    Lexed {
        tokens,
        errors: lexer.into_errors(),
    }
}

#[cfg(test)]
mod tests;

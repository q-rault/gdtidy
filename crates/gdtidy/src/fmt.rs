//! `gdtidy fmt` for one file (RFC 0003).
//!
//! The formatter is a line printer over gdsyntax's lossless tree. It splits
//! the tokens into physical lines, keeping the author's line breaks, then
//! decides each line's indentation, the blank lines before it and the spaces
//! between its tokens from the tree. Spelling and layout rewrites (`rewrite`)
//! are applied to the tokens before they are split into lines.
//!
//! Every result passes a safety check before it is used: it must parse
//! without errors and have the same code as the input, apart from what the
//! formatter is allowed to change (`fingerprint`).

use std::collections::HashMap;

use gdsyntax::ast::code_range;
use gdsyntax::literal;
use gdsyntax::{SyntaxKind as K, SyntaxNode, SyntaxToken};

/// The `[fmt]` settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Levels added for a continuation line that does not follow a bracket
    /// ending its line: content after an opener on the same line, or a `\`.
    pub continuation_indent: usize,
    /// `{ "a": 1 }` rather than `{"a": 1}` for one-line dictionaries.
    pub dict_brace_spaces: bool,
    /// Add a trailing comma to multi-line arrays, dictionaries and enums.
    pub trailing_commas: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            continuation_indent: 2,
            dict_brace_spaces: true,
            trailing_commas: true,
        }
    }
}

/// Why a file was not formatted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The file does not parse: Godot's first error.
    Syntax {
        line: usize,
        column: usize,
        message: String,
        not_utf8: bool,
    },
    /// The formatted text would not have the same code. This is a gdtidy bug.
    Unsafe,
}

/// The formatted text of a file.
pub fn format(bytes: &[u8], options: &Options) -> Result<String, Failure> {
    let tree = gdsyntax::parse_bytes(bytes);
    if let Some(error) = tree.errors().into_iter().next() {
        return Err(Failure::Syntax {
            line: error.start.line,
            column: error.start.column,
            not_utf8: error.message == gdsyntax::INVALID_UTF8_MESSAGE,
            message: error.message,
        });
    }
    let output = Printer::new(&tree, options).print();
    let after = gdsyntax::parse(output.clone());
    if !after.errors().is_empty() || fingerprint(after.root()) != fingerprint(tree.root()) {
        if std::env::var_os("GDTIDY_FMT_DEBUG").is_some() {
            debug_unsafe(&tree, &after);
        }
        return Err(Failure::Unsafe);
    }
    Ok(output)
}

// ---------------------------------------------------------------------------
// Lines

/// A token as it will be printed.
#[derive(Debug, Clone)]
struct Item {
    /// The token, or for an inserted token, the one it follows.
    token: SyntaxToken,
    kind: K,
    text: String,
    /// The whitespace written before it on its line (empty if none).
    space: String,
    inserted: bool,
}

impl Item {
    fn is_comment(&self) -> bool {
        matches!(self.kind, K::COMMENT | K::DOC_COMMENT)
    }

    fn offset(&self) -> usize {
        self.token.text_range().start().into()
    }
}

#[derive(Debug, Clone, Default)]
struct Line {
    items: Vec<Item>,
    /// Empty lines written before this one.
    blank_before: usize,
    /// The whitespace this line was written with.
    indent_text: String,
    /// The line ends with a `\` continuation.
    continued: bool,
    /// Byte range of the line in the source, for `fmt-off` regions.
    source_start: usize,
    source_end: usize,
    /// Kept as written (inside `# gdtidy: fmt-off`).
    verbatim: bool,
}

impl Line {
    fn first(&self) -> &Item {
        &self.items[0]
    }

    fn is_comment_only(&self) -> bool {
        self.items.iter().all(Item::is_comment)
    }

    fn last_code(&self) -> Option<&Item> {
        self.items.iter().rev().find(|i| !i.is_comment())
    }
}

/// An open bracket at the start of a line.
#[derive(Debug, Clone, Copy)]
struct Open {
    line: usize,
    /// The opener ends its line, so its content gets one level.
    hanging: bool,
    offset: usize,
}

/// Where a line that starts a statement sits.
#[derive(Debug, Clone, Copy)]
struct Statement {
    indent: usize,
    /// The statement is a class member (in a file or a class body).
    member: bool,
    /// The statement is a function or a class declaration.
    definition: bool,
    /// The offset where the statement's code ends.
    end: usize,
}

struct Printer<'a> {
    source: &'a str,
    options: &'a Options,
    lines: Vec<Line>,
    /// The line each token starts on, by offset.
    line_of: HashMap<usize, usize>,
    indent: Vec<usize>,
    /// For code lines: the statement the line starts, if it starts one.
    statement: Vec<Option<Statement>>,
    /// For code lines: the indentation of the line its statement starts on.
    statement_indent: Vec<usize>,
    opens: Vec<Vec<Open>>,
    /// Columns of a written indentation level, for files indented with spaces.
    indent_width: usize,
}

const OPENERS: [K; 3] = [K::PARENTHESIS_OPEN, K::BRACKET_OPEN, K::BRACE_OPEN];
const CLOSERS: [K; 3] = [K::PARENTHESIS_CLOSE, K::BRACKET_CLOSE, K::BRACE_CLOSE];

impl<'a> Printer<'a> {
    fn new(tree: &'a gdsyntax::SyntaxTree, options: &'a Options) -> Self {
        let pieces = rewrite::pieces(tree.root(), options);
        let mut printer = Printer {
            source: tree.source(),
            options,
            lines: Vec::new(),
            line_of: HashMap::new(),
            indent: Vec::new(),
            statement: Vec::new(),
            statement_indent: Vec::new(),
            opens: Vec::new(),
            indent_width: 4,
        };
        printer.split_lines(pieces);
        printer
    }

    /// Split the tokens into physical lines.
    fn split_lines(&mut self, pieces: Vec<rewrite::Piece>) {
        let mut line = Line::default();
        let mut blank = 0;
        let mut space = String::new();
        let mut off = false;
        for piece in pieces {
            match piece {
                rewrite::Piece::Break { start, end } => {
                    if line.items.is_empty() {
                        blank += 1;
                    } else {
                        line.blank_before = std::mem::take(&mut blank);
                        line.source_end = start;
                        self.lines.push(std::mem::take(&mut line));
                    }
                    line = Line {
                        source_start: end,
                        ..Line::default()
                    };
                    space.clear();
                }
                rewrite::Piece::Space(text) => {
                    if line.items.is_empty() {
                        line.indent_text.push_str(&text);
                    } else {
                        space.push_str(&text);
                    }
                }
                rewrite::Piece::Continuation => line.continued = true,
                rewrite::Piece::Item(mut item) => {
                    item.space = std::mem::take(&mut space);
                    if item.is_comment() {
                        match item.text.trim() {
                            "# gdtidy: fmt-off" => off = true,
                            "# gdtidy: fmt-on" => {
                                off = false;
                                // The fmt-on comment itself is formatted.
                                line.verbatim = false;
                            }
                            _ => {}
                        }
                    }
                    line.verbatim |= off;
                    line.items.push(item);
                }
            }
        }
        if !line.items.is_empty() {
            line.blank_before = blank;
            line.source_end = self.source.len();
            self.lines.push(line);
        }
        for (i, line) in self.lines.iter().enumerate() {
            for item in &line.items {
                if !item.inserted {
                    self.line_of.entry(item.offset()).or_insert(i);
                }
            }
        }
    }

    fn print(mut self) -> String {
        self.indent_width = self.detect_indent_width();
        self.compute_indents();
        let blanks = self.compute_blank_lines();
        let mut out = String::with_capacity(self.source.len());
        let mut i = 0;
        while i < self.lines.len() {
            for _ in 0..blanks[i] {
                out.push('\n');
            }
            if self.lines[i].verbatim {
                // A fmt-off region: copy the source from this line to the
                // last line of the region, blank lines included.
                let start = self.lines[i].source_start;
                let mut j = i;
                while j + 1 < self.lines.len() && self.lines[j + 1].verbatim {
                    j += 1;
                }
                let text = &self.source[start..self.lines[j].source_end];
                out.push_str(text.trim_end_matches(['\r', '\n']));
                out.push('\n');
                i = j + 1;
                continue;
            }
            let line = &self.lines[i];
            for _ in 0..self.indent[i] {
                out.push('\t');
            }
            for (k, item) in line.items.iter().enumerate() {
                if k > 0 {
                    out.push_str(&self.space_between(&line.items[k - 1], item));
                }
                out.push_str(&item.text);
            }
            if line.continued {
                out.push_str(" \\");
            }
            out.push('\n');
            i += 1;
        }
        out
    }

    fn detect_indent_width(&self) -> usize {
        self.lines
            .iter()
            .map(|l| &l.indent_text)
            .find(|t| !t.is_empty())
            .filter(|t| t.chars().all(|c| c == ' '))
            .map_or(4, |t| t.len())
    }

    // -----------------------------------------------------------------------
    // Indentation

    fn compute_indents(&mut self) {
        let n = self.lines.len();
        self.indent = vec![0; n];
        self.statement = vec![None; n];
        self.statement_indent = vec![0; n];
        self.opens = vec![Vec::new(); n];
        let mut stack: Vec<Open> = Vec::new();
        for i in 0..n {
            self.opens[i] = stack.clone();
            if !self.lines[i].is_comment_only() {
                self.indent_code_line(i, &stack);
            }
            let line = &self.lines[i];
            let last_code = line.items.iter().rposition(|it| !it.is_comment());
            for (k, item) in line.items.iter().enumerate() {
                if OPENERS.contains(&item.kind) {
                    stack.push(Open {
                        line: i,
                        hanging: Some(k) == last_code,
                        offset: item.offset(),
                    });
                } else if CLOSERS.contains(&item.kind) {
                    stack.pop();
                }
            }
        }
        for i in 0..n {
            if self.lines[i].is_comment_only() {
                self.indent[i] = self.comment_indent(i);
            }
        }
    }

    fn indent_code_line(&mut self, i: usize, stack: &[Open]) {
        let token = self.lines[i]
            .items
            .iter()
            .find(|it| !it.is_comment())
            .expect("a code line has code")
            .token
            .clone();
        if let Some(statement) = self.statement_at(&token) {
            self.indent[i] = statement.indent;
            self.statement[i] = Some(statement);
            self.statement_indent[i] = statement.indent;
            return;
        }
        // A continuation line: inside brackets opened by this statement, or
        // after a `\`.
        let start = statement_start(&token);
        let first_line = self.line_of.get(&start).copied().unwrap_or(i);
        self.statement_indent[i] = self.indent[first_line];
        let open = stack.iter().rev().find(|o| o.offset >= start);
        // After a lambda's body, Godot only accepts going back to a level
        // that is open: the statement's.
        if let Some(open) = open
            && self.follows_lambda_body(i, open.offset)
        {
            self.indent[i] = self.indent[first_line];
            return;
        }
        self.indent[i] = match open {
            Some(open) if CLOSERS.contains(&token.kind()) => self.indent[open.line],
            Some(open) => self.indent[open.line] + self.content_levels(open),
            None => self.indent[first_line] + self.options.continuation_indent,
        };
    }

    /// The code line before line `i` ends the body of a lambda that starts
    /// after `from`.
    fn follows_lambda_body(&self, i: usize, from: usize) -> bool {
        let Some(prev) = (0..i).rev().find(|&j| !self.lines[j].is_comment_only()) else {
            return false;
        };
        let Some(last) = self.lines[prev].last_code() else {
            return false;
        };
        last.token.parent_ancestors().any(|a| {
            a.kind() == K::SUITE
                && a.parent().is_some_and(|l| {
                    l.kind() == K::LAMBDA && usize::from(l.text_range().start()) >= from
                })
                && a.children_with_tokens().any(|c| c.kind() == K::INDENT)
        })
    }

    fn content_levels(&self, open: &Open) -> usize {
        if open.hanging {
            1
        } else {
            self.options.continuation_indent
        }
    }

    /// If `token` starts a statement (or a clause, branch, getter...) in a
    /// block, where that statement sits.
    fn statement_at(&self, token: &SyntaxToken) -> Option<Statement> {
        let offset: usize = token.text_range().start().into();
        let mut node = token.parent();
        while let Some(n) = node {
            // A member under annotations on their own lines starts a line too.
            let annotation_line = n
                .children()
                .any(|c| c.kind() == K::ANNOTATION_NODE && first_code(&c) == Some(offset));
            if first_code(&n) != Some(offset)
                && first_after_annotations(&n) != Some(offset)
                && !annotation_line
            {
                return None;
            }
            let parent = n.parent()?;
            let indent = match (parent.kind(), n.kind()) {
                (K::SOURCE_FILE, _) => Some(0),
                (K::SUITE | K::CLASS_BODY, _) => parent.parent().map(|o| self.indent_of(&o) + 1),
                (K::MATCH_STMT, K::MATCH_BRANCH) => Some(self.indent_of(&parent) + 1),
                (K::PROPERTY, K::GETTER | K::SETTER) => {
                    parent.parent().map(|o| self.indent_of(&o) + 1)
                }
                (K::IF_STMT, K::ELIF_CLAUSE | K::ELSE_CLAUSE) => Some(self.indent_of(&parent)),
                _ => None,
            };
            if let Some(indent) = indent {
                return Some(Statement {
                    indent,
                    member: matches!(parent.kind(), K::SOURCE_FILE | K::CLASS_BODY),
                    definition: matches!(n.kind(), K::FUNC | K::CLASS),
                    end: code_range(&n)
                        .map_or(n.text_range().end(), |r| r.end())
                        .into(),
                });
            }
            node = Some(parent);
        }
        None
    }

    /// The indentation of the line `node` starts on.
    fn indent_of(&self, node: &SyntaxNode) -> usize {
        first_code(node)
            .and_then(|o| self.line_of.get(&o))
            .map_or(0, |&line| self.indent[line])
    }

    fn comment_indent(&self, i: usize) -> usize {
        let next = (i + 1..self.lines.len()).find(|&j| !self.lines[j].is_comment_only());
        let prev = (0..i).rev().find(|&j| !self.lines[j].is_comment_only());
        if let (Some(next), Some(open)) = (next, self.opens[i].last())
            && self.statement[next].is_none()
        {
            let first = self.lines[next].items.iter().find(|it| !it.is_comment());
            return match first {
                Some(item) if CLOSERS.contains(&item.kind) => {
                    self.indent[open.line] + self.content_levels(open)
                }
                _ => self.indent[next],
            };
        }
        let low = next.map_or(0, |j| self.indent[j]);
        let high = prev.map_or(low, |j| self.statement_indent[j]).max(low);
        self.written_level(i).clamp(low, high)
    }

    fn written_level(&self, i: usize) -> usize {
        let text = &self.lines[i].indent_text;
        let tabs = text.chars().filter(|&c| c == '\t').count();
        let spaces = text.chars().filter(|&c| c == ' ').count();
        tabs + spaces / self.indent_width.max(1)
    }

    // -----------------------------------------------------------------------
    // Blank lines

    #[allow(clippy::needless_range_loop)] // Lines are compared with their neighbours.
    fn compute_blank_lines(&self) -> Vec<usize> {
        let n = self.lines.len();
        let mut blanks = vec![0; n];
        for i in 1..n {
            let max = if !self.opens[i].is_empty() {
                1
            } else if self.is_member_level(i) {
                2
            } else {
                1
            };
            blanks[i] = self.lines[i].blank_before.min(max);
            if self.starts_block(i) {
                blanks[i] = 0;
            }
        }
        // Two blank lines around functions and classes.
        for d in 0..n {
            let Some(statement) = self.statement[d] else {
                continue;
            };
            if !(statement.member && statement.definition) {
                continue;
            }
            // Comments and annotations directly above belong to it.
            let mut u = d;
            while u > 0
                && self.lines[u].blank_before == 0
                && self.indent[u - 1] == self.indent[d]
                && (self.lines[u - 1].is_comment_only() || self.is_annotation_line(u - 1))
            {
                u -= 1;
            }
            if u > 0 && !self.starts_block(u) {
                blanks[u] = 2;
            }
            let after = (d + 1..n).find(|&j| {
                self.lines[j].first().offset() >= statement.end
                    && self.indent[j] <= statement.indent
            });
            if let Some(after) = after {
                blanks[after] = 2;
            }
        }
        blanks
    }

    /// The line is the first one in a block: the previous line ends with
    /// the `:` that opens it.
    fn starts_block(&self, i: usize) -> bool {
        i > 0
            && self.lines[i - 1]
                .last_code()
                .is_some_and(|it| it.kind == K::COLON)
            && self.indent[i] > self.indent[i - 1]
    }

    fn is_member_level(&self, i: usize) -> bool {
        if let Some(statement) = self.statement[i] {
            return statement.member;
        }
        if !self.lines[i].is_comment_only() {
            return false;
        }
        let next = (i + 1..self.lines.len()).find(|&j| !self.lines[j].is_comment_only());
        match next.and_then(|j| self.statement[j].map(|s| (j, s))) {
            Some((j, statement)) => statement.member && self.indent[j] == self.indent[i],
            None => self.indent[i] == 0,
        }
    }

    /// A line holding only annotations, such as `@rpc` above a function.
    fn is_annotation_line(&self, i: usize) -> bool {
        self.lines[i]
            .items
            .iter()
            .filter(|it| !it.is_comment())
            .all(|it| {
                it.token
                    .parent_ancestors()
                    .any(|a| a.kind() == K::ANNOTATION_NODE)
            })
    }

    // -----------------------------------------------------------------------
    // Spaces within a line

    fn space_between(&self, a: &Item, b: &Item) -> String {
        if b.is_comment() {
            return if b.space.is_empty() {
                " ".to_string()
            } else {
                b.space.clone()
            };
        }
        let written = || {
            if b.space.is_empty() {
                String::new()
            } else {
                " ".to_string()
            }
        };
        let one = || " ".to_string();
        let none = String::new;

        // Node paths are written exactly as they are.
        if let (Some(x), Some(y)) = (get_node(&a.token), get_node(&b.token))
            && x == y
            && !b.inserted
        {
            return b.space.clone();
        }
        let parent_b = b.token.parent().map(|p| p.kind());
        if a.kind == K::ANNOTATION {
            return if b.kind == K::PARENTHESIS_OPEN {
                none()
            } else {
                one()
            };
        }
        // `:=`
        if a.kind == K::COLON && b.kind == K::EQUAL {
            return none();
        }
        if b.kind == K::COLON && next_is_equal(&b.token) {
            return one();
        }
        match b.kind {
            K::COMMA
            | K::SEMICOLON
            | K::PARENTHESIS_CLOSE
            | K::BRACKET_CLOSE
            | K::PERIOD
            | K::COLON => return none(),
            K::BRACE_CLOSE => {
                return if a.kind != K::BRACE_OPEN && self.dict_spaces(&b.token) {
                    one()
                } else if is_dict(&b.token) {
                    none()
                } else {
                    written()
                };
            }
            _ => {}
        }
        match a.kind {
            K::PARENTHESIS_OPEN | K::BRACKET_OPEN | K::PERIOD => return none(),
            K::BRACE_OPEN => {
                return if self.dict_spaces(&a.token) {
                    one()
                } else if is_dict(&a.token) {
                    none()
                } else {
                    written()
                };
            }
            K::COMMA | K::COLON => return one(),
            _ => {}
        }
        if b.kind == K::PARENTHESIS_OPEN {
            if matches!(
                parent_b,
                Some(K::ARG_LIST | K::PARAM_LIST | K::PRELOAD | K::ASSERT_STMT)
            ) {
                return none();
            }
        } else if b.kind == K::BRACKET_OPEN && matches!(parent_b, Some(K::SUBSCRIPT | K::TYPE_ARGS))
        {
            return none();
        }
        if is_unary(&a.token) {
            return if a.kind == K::NOT_KW { one() } else { none() };
        }
        if is_spaced_operator(&a.token) || is_spaced_operator(&b.token) {
            return one();
        }
        if is_keyword(a.kind) || is_keyword(b.kind) {
            return one();
        }
        written()
    }

    /// `{` or `}` of a dictionary written on one line, with brace spaces on.
    fn dict_spaces(&self, brace: &SyntaxToken) -> bool {
        if !self.options.dict_brace_spaces || !is_dict(brace) {
            return false;
        }
        let Some(node) = brace.parent() else {
            return false;
        };
        let range = node.text_range();
        let (Some(open), Some(close)) = (
            self.line_of.get(&usize::from(range.start())),
            node.last_token()
                .and_then(|t| self.line_of.get(&usize::from(t.text_range().start()))),
        ) else {
            return false;
        };
        open == close
    }
}

fn is_dict(brace: &SyntaxToken) -> bool {
    matches!(
        brace.parent().map(|p| p.kind()),
        Some(K::DICT | K::DICT_PATTERN)
    )
}

fn get_node(token: &SyntaxToken) -> Option<SyntaxNode> {
    token.parent_ancestors().find(|a| a.kind() == K::GET_NODE)
}

fn next_is_equal(token: &SyntaxToken) -> bool {
    let mut next = token.next_token();
    while let Some(t) = &next {
        if t.kind().is_trivia() {
            next = t.next_token();
        } else {
            break;
        }
    }
    next.is_some_and(|t| t.kind() == K::EQUAL)
}

/// The offset of a node's first code token.
fn first_code(node: &SyntaxNode) -> Option<usize> {
    code_range(node).map(|r| r.start().into())
}

/// The offset of a node's first code token that is not in one of its
/// annotations.
fn first_after_annotations(node: &SyntaxNode) -> Option<usize> {
    node.children_with_tokens()
        .filter(|e| e.kind() != K::ANNOTATION_NODE)
        .find_map(|e| match e {
            gdsyntax::rowan::NodeOrToken::Node(n) => first_code(&n),
            gdsyntax::rowan::NodeOrToken::Token(t) => {
                gdsyntax::ast::is_code(&t).then(|| t.text_range().start().into())
            }
        })
}

/// Where the statement holding `token` starts: the nearest ancestor that is
/// a direct child of a block.
fn statement_start(token: &SyntaxToken) -> usize {
    for node in token.parent_ancestors() {
        let in_block = node.parent().is_none_or(|p| {
            matches!(
                p.kind(),
                K::SOURCE_FILE
                    | K::SUITE
                    | K::CLASS_BODY
                    | K::MATCH_STMT
                    | K::PROPERTY
                    | K::IF_STMT
            )
        });
        if in_block && let Some(start) = first_code(&node) {
            return start;
        }
    }
    0
}

fn is_unary(token: &SyntaxToken) -> bool {
    token.parent().is_some_and(|p| p.kind() == K::UNARY_EXPR)
        && matches!(
            token.kind(),
            K::MINUS | K::PLUS | K::TILDE | K::BANG | K::NOT_KW
        )
}

/// A binary or assignment operator, or `->`: one space on each side.
fn is_spaced_operator(token: &SyntaxToken) -> bool {
    use K::*;
    let kind = token.kind();
    let parent = token.parent().map(|p| p.kind());
    match kind {
        FORWARD_ARROW => true,
        EQUAL => true,
        PLUS_EQUAL
        | MINUS_EQUAL
        | STAR_EQUAL
        | STAR_STAR_EQUAL
        | SLASH_EQUAL
        | PERCENT_EQUAL
        | LESS_LESS_EQUAL
        | GREATER_GREATER_EQUAL
        | AMPERSAND_EQUAL
        | PIPE_EQUAL
        | CARET_EQUAL => true,
        LESS | LESS_EQUAL | GREATER | GREATER_EQUAL | EQUAL_EQUAL | BANG_EQUAL
        | AMPERSAND_AMPERSAND | PIPE_PIPE | AMPERSAND | PIPE | CARET | LESS_LESS
        | GREATER_GREATER | PLUS | MINUS | STAR | STAR_STAR | SLASH | PERCENT => {
            parent == Some(BINARY_EXPR)
        }
        _ => false,
    }
}

/// Keywords that are words, and so need a space on each side.
fn is_keyword(kind: K) -> bool {
    use K::*;
    matches!(
        kind,
        AND_KW
            | OR_KW
            | NOT_KW
            | IF_KW
            | ELIF_KW
            | ELSE_KW
            | FOR_KW
            | WHILE_KW
            | RETURN_KW
            | MATCH_KW
            | WHEN_KW
            | AS_KW
            | AWAIT_KW
            | CLASS_KW
            | CLASS_NAME_KW
            | CONST_KW
            | ENUM_KW
            | EXTENDS_KW
            | FUNC_KW
            | IN_KW
            | IS_KW
            | SIGNAL_KW
            | STATIC_KW
            | VAR_KW
    )
}

// ---------------------------------------------------------------------------
// Safety check

/// What must not change when formatting: the code tokens, each with the
/// number of blocks it is in, normalized for the changes `fmt` makes, and
/// the comments' text.
#[derive(Debug, PartialEq)]
struct Fingerprint {
    code: Vec<(K, String, usize)>,
    comments: Vec<String>,
}

fn fingerprint(root: &SyntaxNode) -> Fingerprint {
    let mut code: Vec<(K, String, usize)> = Vec::new();
    let mut comments = Vec::new();
    for token in root
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        let kind = token.kind();
        match kind {
            K::COMMENT | K::DOC_COMMENT => {
                comments.push(token.text().trim_end().to_string());
                continue;
            }
            K::SEMICOLON => continue,
            k if k.is_trivia() || matches!(k, K::NEWLINE | K::INDENT | K::DEDENT | K::EOF) => {
                continue;
            }
            _ => {}
        }
        if matches!(kind, K::PARENTHESIS_OPEN | K::PARENTHESIS_CLOSE) && is_condition_paren(&token)
        {
            continue;
        }
        let (kind, text) = match kind {
            K::AMPERSAND_AMPERSAND => (K::AND_KW, "and".to_string()),
            K::PIPE_PIPE => (K::OR_KW, "or".to_string()),
            K::BANG => (K::NOT_KW, "not".to_string()),
            // `- 1` may become `-1`, which Godot reads as one token.
            K::INT | K::FLOAT if token.text().starts_with('-') => {
                let depth = depth_of(&token);
                code.push((K::MINUS, "-".to_string(), depth));
                let text = &token.text()[1..];
                let value = if kind == K::INT {
                    format!("{:?}", literal::int_value(text))
                } else {
                    format!("{:?}", literal::float_value(text).map(f64::to_bits))
                };
                code.push((kind, value, depth));
                continue;
            }
            K::INT => (kind, format!("{:?}", literal::int_value(token.text()))),
            K::FLOAT => (
                kind,
                format!("{:?}", literal::float_value(token.text()).map(f64::to_bits)),
            ),
            K::STRING | K::STRING_NAME | K::NODE_PATH => {
                let prefix = token
                    .text()
                    .chars()
                    .next()
                    .filter(|c| matches!(c, '&' | '^'));
                (
                    kind,
                    format!("{prefix:?}{:?}", literal::string_value(token.text())),
                )
            }
            _ => (kind, token.text().to_string()),
        };
        code.push((kind, text, depth_of(&token)));
    }
    // Trailing commas may be added or removed.
    let mut kept = Vec::with_capacity(code.len());
    for (i, entry) in code.iter().enumerate() {
        let before_closer = code
            .get(i + 1)
            .is_some_and(|next| CLOSERS.contains(&next.0));
        if !(entry.0 == K::COMMA && before_closer) {
            kept.push(entry.clone());
        }
    }
    Fingerprint {
        code: kept,
        comments,
    }
}

/// Why a result failed the safety check, on stderr (`GDTIDY_FMT_DEBUG`).
fn debug_unsafe(before: &gdsyntax::SyntaxTree, after: &gdsyntax::SyntaxTree) {
    if let Some(error) = after.errors().first() {
        let line = after.line_text(error.start.line).unwrap_or("");
        eprintln!(
            "fmt debug: {}:{}: {}
  {line}",
            error.start.line, error.start.column, error.message
        );
        return;
    }
    let (a, b) = (fingerprint(before.root()), fingerprint(after.root()));
    if let Some(i) = (0..a.code.len().min(b.code.len())).find(|&i| a.code[i] != b.code[i]) {
        let context = |f: &Fingerprint| {
            f.code[i.saturating_sub(4)..(i + 3).min(f.code.len())]
                .iter()
                .map(|(_, t, d)| format!("{t}@{d}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        eprintln!(
            "fmt debug: code differs
  before: {}
  after:  {}",
            context(&a),
            context(&b)
        );
    } else if a.code.len() != b.code.len() {
        eprintln!(
            "fmt debug: {} code tokens before, {} after",
            a.code.len(),
            b.code.len()
        );
    } else {
        eprintln!("fmt debug: comments differ");
    }
}

/// How many blocks a token is in.
fn depth_of(token: &SyntaxToken) -> usize {
    token
        .parent_ancestors()
        .filter(|a| matches!(a.kind(), K::SUITE | K::CLASS_BODY))
        .count()
}

/// A parenthesis around a whole `if`, `elif` or `while` condition.
fn is_condition_paren(token: &SyntaxToken) -> bool {
    token.parent().is_some_and(|paren| {
        paren.kind() == K::PAREN_EXPR
            && paren
                .parent()
                .is_some_and(|p| matches!(p.kind(), K::IF_STMT | K::ELIF_CLAUSE | K::WHILE_STMT))
    })
}

mod rewrite;

#[cfg(test)]
mod tests;

//! GDScript syntax trees and syntax errors, shared by the gdsuite tools.
//!
//! The parser is a translation of Godot 4.7's own tokenizer and parser
//! ([`lexer`], [`parser`]), so it accepts and rejects what Godot does, with
//! Godot's error messages. Its tree is lossless: every byte of the file,
//! comments and whitespace included, is in it, so `tree.text()` is always the
//! source. [`ast`] gives typed access to declarations, statements and
//! expressions.
//!
//! ```
//! use gdsyntax::ast::Member;
//!
//! let tree = gdsyntax::parse("extends Node\n\n\nfunc _ready() -> void:\n\tprint(\"hi\")\n");
//! assert!(tree.errors().is_empty());
//! for member in tree.file().members() {
//!     if let Member::Func(func) = member {
//!         assert_eq!(func.name().unwrap().text(), "_ready");
//!     }
//! }
//! ```

pub mod ast;
pub mod kind;
pub mod lexer;
pub mod literal;
pub mod parser;

pub use kind::{GDScript, SyntaxKind};

/// A node of a lossless GDScript tree.
pub type SyntaxNode = rowan::SyntaxNode<GDScript>;
/// A token of a lossless GDScript tree.
pub type SyntaxToken = rowan::SyntaxToken<GDScript>;
/// A node or a token.
pub type SyntaxElement = rowan::SyntaxElement<GDScript>;

/// A place in the source. Both fields are 1-based; `column` counts characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// An error in the source, with Godot's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub start: Position,
    pub end: Position,
    /// Byte offsets of the error.
    pub range: std::ops::Range<usize>,
    pub message: String,
}

/// A parsed GDScript file: its source, its tree and its errors.
pub struct SyntaxTree {
    source: String,
    root: SyntaxNode,
    errors: Vec<parser::ParseError>,
}

/// Parse GDScript source. Parsing never fails: code that is not valid
/// GDScript still ends up in the tree, and [`SyntaxTree::errors`] says what
/// is wrong, in the order Godot reports it.
pub fn parse(source: impl Into<String>) -> SyntaxTree {
    let source = source.into();
    let parse = parser::parse(&source);
    SyntaxTree {
        root: SyntaxNode::new_root(parse.green),
        errors: parse.errors,
        source,
    }
}

impl SyntaxTree {
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The root `SOURCE_FILE` node.
    pub fn root(&self) -> &SyntaxNode {
        &self.root
    }

    /// The root as a typed [`ast::SourceFile`].
    pub fn file(&self) -> ast::SourceFile {
        use ast::AstNode;
        ast::SourceFile::cast(self.root.clone()).expect("the root is always a SOURCE_FILE")
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// The errors, in the order Godot reports them; the first is the one
    /// Godot's editor shows.
    pub fn errors(&self) -> Vec<SyntaxError> {
        self.errors
            .iter()
            .map(|error| SyntaxError {
                start: self.position(error.start),
                end: self.position(error.end),
                range: error.start..error.end,
                message: error.message.clone(),
            })
            .collect()
    }

    /// The position of a byte offset in the source.
    pub fn position(&self, byte: usize) -> Position {
        let byte = byte.min(self.source.len());
        let before = &self.source[..byte];
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        Position {
            line: before.matches('\n').count() + 1,
            column: before[line_start..].chars().count() + 1,
        }
    }
}

/// The source as a flat tree of tokens, without parsing: one `SOURCE_FILE`
/// node holding every token, with the tokenizer's errors.
pub fn lossless_tree(source: &str) -> (SyntaxNode, Vec<lexer::LexError>) {
    let lexed = lexer::tokenize(source);
    let mut builder = rowan::GreenNodeBuilder::new();
    builder.start_node(SyntaxKind::SOURCE_FILE.into());
    let mut offset = 0;
    for token in &lexed.tokens {
        let end = offset + token.len as usize;
        builder.token(token.kind.into(), &source[offset..end]);
        offset = end;
    }
    builder.finish_node();
    (SyntaxNode::new_root(builder.finish()), lexed.errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_code_has_no_errors() {
        let tree = parse("extends Node\n\n\nfunc _ready() -> void:\n\tprint(\"hi\")\n");
        assert!(!tree.has_errors());
        assert_eq!(tree.root().kind(), SyntaxKind::SOURCE_FILE);
        assert_eq!(tree.root().to_string(), tree.source());
    }

    #[test]
    fn reports_godot_s_first_error_where_it_is() {
        let tree = parse("extends Node\n\nfunc f(:\n\tpass\n");
        let errors = tree.errors();
        assert_eq!(errors[0].message, "Expected parameter name.");
        assert_eq!(errors[0].start.line, 3);
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let tree = parse("var é = 1\n");
        let byte = tree.source().find('=').unwrap();
        // "var é " is 6 characters but 7 bytes.
        assert_eq!(tree.position(byte), Position { line: 1, column: 7 });
    }

    #[test]
    fn comments_are_in_the_tree() {
        let tree = parse("# note\nvar x = 1\n");
        let comment = tree.root().first_token().unwrap();
        assert_eq!(comment.kind(), SyntaxKind::COMMENT);
        assert_eq!(comment.text(), "# note");
    }
}

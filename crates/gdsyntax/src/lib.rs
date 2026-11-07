//! GDScript syntax trees and syntax errors, shared by the gdsuite tools.
//!
//! [`parser::parse`] is a port of Godot 4.7's GDScript parser that builds a
//! lossless tree, on top of [`lexer`], a port of its tokenizer (see
//! `rfcs/0001-gdsyntax-parser.md`). [`parse`] still uses tree-sitter-gdscript
//! until gdtidy moves to the new parser.

pub mod kind;
pub mod lexer;
pub mod parser;

pub use kind::{GDScript, SyntaxKind};
pub use tree_sitter::{Node, Tree, TreeCursor};

/// A node of a lossless GDScript tree.
pub type SyntaxNode = rowan::SyntaxNode<GDScript>;
/// A token of a lossless GDScript tree.
pub type SyntaxToken = rowan::SyntaxToken<GDScript>;

/// The source as a lossless tree: one `SOURCE_FILE` node holding every token,
/// trivia included, with the tokenizer's errors. Until the parser exists the
/// tree is flat; its text is always exactly `source`.
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

/// A place in the source. Both fields are 1-based; `column` counts characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// A part of the source the grammar could not make sense of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub start: Position,
    pub end: Position,
    pub message: String,
}

/// A parsed GDScript file, holding its source and its tree.
pub struct SyntaxTree {
    source: String,
    tree: Tree,
}

/// Parse GDScript source. Parsing never fails: code the grammar does not
/// understand becomes error nodes, which [`SyntaxTree::errors`] reports.
pub fn parse(source: impl Into<String>) -> SyntaxTree {
    let source = source.into();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_gdscript::LANGUAGE.into())
        .expect("the bundled GDScript grammar matches the tree-sitter version");
    let tree = parser
        .parse(&source, None)
        .expect("parsing without a timeout or cancellation always returns a tree");
    SyntaxTree { source, tree }
}

impl SyntaxTree {
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    /// The source text a node covers.
    pub fn text(&self, node: Node<'_>) -> &str {
        &self.source[node.byte_range()]
    }

    pub fn has_errors(&self) -> bool {
        self.root().has_error()
    }

    /// The syntax errors in source order. Only the outermost error node of a
    /// region is reported, so one mistake gives one error.
    pub fn errors(&self) -> Vec<SyntaxError> {
        let mut errors = Vec::new();
        if self.has_errors() {
            self.collect_errors(self.root(), &mut errors);
        }
        errors.sort_by_key(|error| error.start);
        errors
    }

    /// The position of a byte offset in the source.
    pub fn position(&self, byte: usize) -> Position {
        let before = &self.source[..byte];
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        Position {
            line: before.matches('\n').count() + 1,
            column: before[line_start..].chars().count() + 1,
        }
    }

    fn collect_errors(&self, node: Node<'_>, errors: &mut Vec<SyntaxError>) {
        if node.is_missing() {
            errors.push(self.error_at(node, format!("missing `{}`", node.kind())));
            return;
        }
        if node.is_error() {
            let text = self.text(node).trim();
            let first_line = text.lines().next().unwrap_or("");
            let message = if first_line.is_empty() {
                "unexpected end of input".to_string()
            } else if first_line.chars().count() > 40 {
                let short: String = first_line.chars().take(40).collect();
                format!("cannot parse `{short}...`")
            } else {
                format!("cannot parse `{first_line}`")
            };
            errors.push(self.error_at(node, message));
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.has_error() || child.is_missing() {
                self.collect_errors(child, errors);
            }
        }
    }

    fn error_at(&self, node: Node<'_>, message: String) -> SyntaxError {
        SyntaxError {
            start: self.position(node.start_byte()),
            end: self.position(node.end_byte()),
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_code_has_no_errors() {
        let tree = parse("extends Node\n\n\nfunc _ready() -> void:\n\tprint(\"hi\")\n");
        assert!(!tree.has_errors());
        assert!(tree.errors().is_empty());
        assert_eq!(tree.root().kind(), "source");
    }

    #[test]
    fn reports_where_the_code_stops_parsing() {
        let tree = parse("extends Node\n\nfunc f(:\n\tpass\n");
        let errors = tree.errors();
        assert!(!errors.is_empty());
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
    fn text_returns_the_source_of_a_node() {
        let tree = parse("# note\nvar x = 1\n");
        let comment = tree.root().child(0).unwrap();
        assert_eq!(comment.kind(), "comment");
        assert_eq!(tree.text(comment), "# note");
    }
}

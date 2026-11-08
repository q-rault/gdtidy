//! The tokens of a file as the printer sees them, after the spelling and
//! layout rewrites.

use gdsyntax::{SyntaxKind as K, SyntaxNode};

use super::{Item, Options};

/// One element of the token stream.
#[derive(Debug)]
pub(super) enum Piece {
    /// A line break in the source, with its byte range.
    Break {
        start: usize,
        end: usize,
    },
    /// Whitespace within a line.
    Space(String),
    /// A `\` that continues the line.
    Continuation,
    Item(Item),
}

pub(super) fn pieces(root: &SyntaxNode, _options: &Options) -> Vec<Piece> {
    let mut out = Vec::new();
    for token in root
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        let kind = token.kind();
        let range = token.text_range();
        match kind {
            K::NEWLINE | K::LINE_BREAK => {
                if token.text().contains('\n') {
                    out.push(Piece::Break {
                        start: range.start().into(),
                        end: range.end().into(),
                    });
                }
            }
            K::WHITESPACE => out.push(Piece::Space(token.text().to_string())),
            K::LINE_CONTINUATION => out.push(Piece::Continuation),
            K::INDENT | K::DEDENT | K::EOF | K::BYTE_ORDER_MARK => {}
            _ => out.push(Piece::Item(Item {
                text: token.text().trim_end().to_string(),
                kind,
                token,
                space: String::new(),
                inserted: false,
            })),
        }
    }
    out
}

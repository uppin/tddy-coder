//! A file's outline, read from its language's `outline.scm` (R3) — what Zed's outline panel shows,
//! and what item anchors walk for an extension language in place of rust-analyzer's
//! `documentSymbol`.

use tddy_code_restructuring::{Position, Range};
use tree_sitter::{Query, Tree};

/// One `@item` match of `outline.scm`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    /// The names of the entries whose `@item` node encloses this one, outermost first.
    pub path: Vec<String>,
    /// The text of the match's `@name` capture.
    pub name: String,
    /// The `@item` node, extended back over the match's leading `@annotation` nodes and ending at
    /// the node's last non-whitespace byte — a node that runs on over the blank lines before the
    /// next item (a TOML table does) would otherwise reach that item's line. One-based lines and
    /// byte columns, the end inclusive of its line, as [`Range`] is everywhere else.
    pub span: Range,
    /// Where the `@name` capture starts.
    pub name_at: Position,
}

impl OutlineEntry {
    /// `path` then `name`: the item path's segments below the file (R4).
    pub fn segments(&self) -> Vec<String> {
        let mut segments = self.path.clone();
        segments.push(self.name.clone());
        segments
    }
}

/// Every entry of one file's outline, in source order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    pub entries: Vec<OutlineEntry>,
}

impl Outline {
    /// The matches of `query` over `tree`, the parse of `text` (R3).
    pub fn of(tree: &Tree, text: &str, query: &Query) -> Outline {
        let _ = (tree, text, query);
        // TODO(reshape-extension-outline): implement
        todo!("Outline::of")
    }

    /// The entries whose segments are exactly `segments`: one for a sound item path, none for an
    /// absent one, several for an ambiguous one — the caller refuses both of the latter.
    pub fn entries_at(&self, segments: &[String]) -> Vec<&OutlineEntry> {
        let _ = segments;
        // TODO(reshape-extension-outline): implement
        todo!("Outline::entries_at")
    }

    /// The innermost entry whose span contains `range`.
    pub fn innermost_enclosing(&self, range: Range) -> Option<&OutlineEntry> {
        let _ = range;
        // TODO(reshape-extension-outline): implement
        todo!("Outline::innermost_enclosing")
    }
}

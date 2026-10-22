//! A file's outline, read from its language's `outline.scm` (R3) — what Zed's outline panel shows,
//! and what item anchors walk for an extension language in place of rust-analyzer's
//! `documentSymbol`.

use tddy_code_restructuring::{Position, Range};
use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

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
        let capture = |name: &str| query.capture_index_for_name(name);
        let (item, name, annotation) = (capture("item"), capture("name"), capture("annotation"));

        let mut found: Vec<Found> = Vec::new();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());
        while let Some(matched) = matches.next() {
            let node_of = |wanted: Option<u32>| {
                matched
                    .captures
                    .iter()
                    .find(|capture| Some(capture.index) == wanted)
                    .map(|capture| capture.node)
            };
            let (Some(item), Some(name)) = (node_of(item), node_of(name)) else {
                continue;
            };
            let leading_annotations = matched
                .captures
                .iter()
                .filter(|capture| Some(capture.index) == annotation)
                .map(|capture| capture.node.start_byte())
                .filter(|start| *start < item.start_byte());
            let from = leading_annotations
                .chain(std::iter::once(item.start_byte()))
                .min()
                .unwrap_or_else(|| item.start_byte());
            // A node can run on over the blank lines before the next item: the span ends where its
            // text does.
            let until =
                item.start_byte() + text[item.start_byte()..item.end_byte()].trim_end().len();
            found.push(Found {
                item: (item.start_byte(), item.end_byte()),
                name: text[name.start_byte()..name.end_byte()].to_string(),
                span: Range {
                    start: position_at(text, from),
                    end: position_at(text, until),
                },
                name_at: position_at(text, name.start_byte()),
            });
        }
        // Enclosing items first: by where the node starts, the longer node before the shorter.
        found.sort_by_key(|entry| (entry.item.0, std::cmp::Reverse(entry.item.1)));

        let entries = found
            .iter()
            .map(|entry| OutlineEntry {
                path: found
                    .iter()
                    .filter(|outer| outer.encloses(entry))
                    .map(|outer| outer.name.clone())
                    .collect(),
                name: entry.name.clone(),
                span: entry.span,
                name_at: entry.name_at,
            })
            .collect();
        Outline { entries }
    }

    /// The entries whose segments are exactly `segments`: one for a sound item path, none for an
    /// absent one, several for an ambiguous one — the caller refuses both of the latter.
    pub fn entries_at(&self, segments: &[String]) -> Vec<&OutlineEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.segments() == segments)
            .collect()
    }

    /// The innermost entry whose span contains `range`.
    pub fn innermost_enclosing(&self, range: Range) -> Option<&OutlineEntry> {
        let key = |position: Position| (position.line, position.col);
        // Entries run outermost first, so the last one that contains `range` is the innermost.
        self.entries.iter().rev().find(|entry| {
            key(entry.span.start) <= key(range.start) && key(range.end) <= key(entry.span.end)
        })
    }
}

/// An `@item` match before its context is known.
struct Found {
    /// The `@item` node's byte range, the measure of what encloses what.
    item: (usize, usize),
    name: String,
    span: Range,
    name_at: Position,
}

impl Found {
    fn encloses(&self, inner: &Found) -> bool {
        self.item != inner.item && self.item.0 <= inner.item.0 && inner.item.1 <= self.item.1
    }
}

/// The one-based line and byte column of the byte offset `at` in `text`.
fn position_at(text: &str, at: usize) -> Position {
    let before = &text.as_bytes()[..at];
    let line_start = before
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |newline| newline + 1);
    Position {
        line: before.iter().filter(|byte| **byte == b'\n').count() as u32 + 1,
        col: (at - line_start) as u32 + 1,
    }
}

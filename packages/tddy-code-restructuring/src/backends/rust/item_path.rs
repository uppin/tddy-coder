//! Resolving an item path through rust-analyzer's document outline.
//!
//! `textDocument/documentSymbol` answers a tree: modules, types and functions, with each `impl`
//! block a node of its own whose children are its members. An [`ItemPath`]'s segments walk that
//! tree — a type segment matches the type *and* every `impl` of it, a `<T as Trait>` segment only
//! that trait's `impl` — and the walk refuses rather than picks when a segment matches twice.

use serde_json::Value;

use super::RustBackend;
use crate::edit::Range;
use crate::item_anchor::{ItemAtResolver, ItemResolver, ResolvedItem};
use crate::plan::{ItemPath, ItemSegment};
use crate::Result;

/// Where a walk of the outline ended: the node's full range and its name's position, zero-based as
/// the server reports them.
// TODO(item-anchors): `resolve_item` walks the outline through this once implemented.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutlineHit {
    pub(crate) range: Value,
    pub(crate) selection_start: Value,
}

/// Walk a `documentSymbol` answer down `segments`.
///
/// Refuses a segment that no node answers to and one that more than one node answers to, naming
/// the segment and the item path in both cases.
// TODO(item-anchors): called by `resolve_item` once implemented.
#[allow(dead_code)]
pub(crate) fn walk_outline(
    symbols: &Value,
    item: &ItemPath,
    segments: &[ItemSegment],
) -> Result<OutlineHit> {
    // TODO(item-anchors): implement
    let _ = (symbols, item, segments);
    todo!("item-anchors: walk the outline by segment")
}

impl ItemResolver for RustBackend {
    fn resolve_item(&mut self, file: &str, item: &ItemPath) -> Result<ResolvedItem> {
        // TODO(item-anchors): implement
        let _ = (file, item);
        todo!("item-anchors: resolve an item path against rust-analyzer")
    }
}

impl ItemAtResolver for RustBackend {
    fn item_enclosing(&mut self, file: &str, range: Range) -> Result<(ItemPath, ResolvedItem)> {
        // TODO(item-anchors): implement
        let _ = (file, range);
        todo!("item-anchors: the innermost item enclosing a range")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// An outline node the way rust-analyzer reports one: zero-based range, children for members.
    fn a_node(name: &str, first: u32, last: u32, children: Value) -> Value {
        json!({
            "name": name,
            "range": { "start": { "line": first, "character": 0 }, "end": { "line": last, "character": 1 } },
            "selectionRange": { "start": { "line": first, "character": 4 }, "end": { "line": first, "character": 8 } },
            "children": children,
        })
    }

    /// `mod workflow` holding `Stack`, two `impl`s of it with a `fmt` each, and `impl Stack { new }`.
    fn an_outline() -> Value {
        json!([a_node(
            "workflow",
            0,
            40,
            json!([
                a_node("Stack", 1, 4, json!([])),
                a_node(
                    "impl Stack",
                    6,
                    11,
                    json!([a_node("new", 7, 10, json!([]))])
                ),
                a_node(
                    "impl Display for Stack",
                    13,
                    17,
                    json!([a_node("fmt", 14, 16, json!([]))])
                ),
                a_node(
                    "impl Debug for Stack",
                    19,
                    23,
                    json!([a_node("fmt", 20, 22, json!([]))])
                ),
            ])
        )])
    }

    fn segments_of(path: &ItemPath) -> Vec<ItemSegment> {
        path.segments()
    }

    #[test]
    fn an_inherent_method_is_found_through_its_types_impl() {
        let path = ItemPath::parse("stacks::workflow::Stack::new").unwrap();

        let hit = walk_outline(&an_outline(), &path, &segments_of(&path));

        assert_eq!(
            hit.ok().map(|hit| hit.range["start"]["line"].clone()),
            Some(json!(7))
        );
    }

    #[test]
    fn a_name_two_trait_impls_define_is_refused() {
        let path = ItemPath::parse("stacks::workflow::Stack::fmt").unwrap();

        let refused = walk_outline(&an_outline(), &path, &segments_of(&path));

        assert!(matches!(
            refused,
            Err(crate::RestructureError::MalformedPlan(_))
        ));
    }

    #[test]
    fn a_trait_qualified_segment_picks_that_traits_impl() {
        let path = ItemPath::parse("stacks::workflow::<Stack as Debug>::fmt").unwrap();

        let hit = walk_outline(&an_outline(), &path, &segments_of(&path));

        assert_eq!(
            hit.ok().map(|hit| hit.range["start"]["line"].clone()),
            Some(json!(20))
        );
    }

    #[test]
    fn a_segment_nothing_answers_to_is_refused() {
        let path = ItemPath::parse("stacks::workflow::Deque::new").unwrap();

        let refused = walk_outline(&an_outline(), &path, &segments_of(&path));

        assert!(matches!(
            refused,
            Err(crate::RestructureError::MalformedPlan(_))
        ));
    }
}

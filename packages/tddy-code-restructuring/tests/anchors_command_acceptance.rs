//! `restructure anchors` emits the anchors a plan should carry: an `items` anchor for named items,
//! and — with `--at` — the `item` anchor of the innermost item enclosing a position, against a live
//! rust-analyzer. This is also the empty-outline defect's regression test: it used to refuse every
//! item of every file.

mod harness;

use harness::{
    a_crate_with_two_inherent_news_and_two_fmts, at, the_anchor_command_emits, STACK_NEW, WORKFLOW,
};
use tddy_code_restructuring::{Anchor, Fingerprint, ItemPath, Range};

/// `pub struct Queue { … }` exactly as the fixture writes it.
const QUEUE_STRUCT: &str = "pub struct Queue {\n    items: Vec<u32>,\n}";

fn a_path(text: &str) -> ItemPath {
    ItemPath::parse(text).expect("the item path parses")
}

#[tokio::test(flavor = "multi_thread")]
async fn anchors_items_emits_an_items_anchor_for_adjacent_module_items() {
    // Given the module-level struct `Queue`
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();

    // When `anchors --items Queue` is asked for
    let anchor = the_anchor_command_emits(&workspace, WORKFLOW, &["Queue"], None).await;

    // Then it is an `items` anchor naming it by path, fingerprinted over its lines
    assert_eq!(
        anchor,
        Ok(Anchor::Items {
            file: WORKFLOW.to_string(),
            items: vec![a_path("stacks::workflow::Queue")],
            fingerprints: vec![Fingerprint::of(QUEUE_STRUCT)],
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn anchors_at_emits_an_item_anchor_relative_to_the_innermost_enclosing_item() {
    // Given a position over `Stack::new`'s two `let` lines, absolute lines 8–9
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    let lets = Range {
        start: at(8, 9),
        end: at(9, 33),
    };

    // When `anchors --at 8:9-9:33` is asked for
    let anchor = the_anchor_command_emits(&workspace, WORKFLOW, &[], Some(lets)).await;

    // Then it anchors `Stack::new`, relative to it, with the absolute start as the hint
    assert_eq!(
        anchor,
        Ok(Anchor::Item {
            item: a_path("stacks::workflow::Stack::new"),
            file: WORKFLOW.to_string(),
            start: Some(at(2, 9)),
            end: Some(at(3, 33)),
            fingerprint: Fingerprint::of(STACK_NEW),
            hint: Some(at(8, 9)),
        })
    );
}

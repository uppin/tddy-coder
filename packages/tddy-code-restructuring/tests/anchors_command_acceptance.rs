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

const TWO_ADJACENT_STRUCTS_AND_A_FUNCTION: &str =
    "pub struct Alpha;\n\npub struct Beta;\n\npub fn gamma() {}\n";

#[tokio::test(flavor = "multi_thread")]
async fn anchors_items_emits_an_items_anchor_for_adjacent_module_items() {
    // Given a module whose first two items are adjacent, and a third that is not asked for
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    workspace.rewriting(WORKFLOW, TWO_ADJACENT_STRUCTS_AND_A_FUNCTION);

    // When `anchors --items Alpha,Beta` is asked for
    let anchor = the_anchor_command_emits(&workspace, WORKFLOW, &["Alpha", "Beta"], None).await;

    // Then it is one `items` anchor naming both by path, each fingerprinted over its own lines
    assert_eq!(
        anchor,
        Ok(Anchor::Items {
            file: WORKFLOW.to_string(),
            items: vec![
                a_path("stacks::workflow::Alpha"),
                a_path("stacks::workflow::Beta")
            ],
            fingerprints: vec![
                Fingerprint::of("pub struct Alpha;"),
                Fingerprint::of("pub struct Beta;")
            ],
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn anchors_items_emits_an_items_anchor_for_a_single_module_item() {
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

const A_COUNTER_WITH_TWO_METHODS_A_GAP_APART: &str = "\
pub struct Counter {
    n: u32,
}

impl Counter {
    pub fn bump(&mut self) {
        self.n += 1;
    }

    pub fn reset(&mut self) {
        self.n = 0;
    }
}
";

const THE_IMPL_BLOCK_OF_THAT_COUNTER: &str = "\
impl Counter {
    pub fn bump(&mut self) {
        self.n += 1;
    }

    pub fn reset(&mut self) {
        self.n = 0;
    }
}";

#[tokio::test(flavor = "multi_thread")]
async fn anchors_at_between_two_members_of_an_inherent_impl_emits_the_impl_block() {
    // Given a position on the blank line between `bump` and `reset`
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    workspace.rewriting(WORKFLOW, A_COUNTER_WITH_TWO_METHODS_A_GAP_APART);
    let between = Range {
        start: at(9, 1),
        end: at(9, 1),
    };

    // When `anchors --at 9:1` is asked for
    let anchor = the_anchor_command_emits(&workspace, WORKFLOW, &[], Some(between)).await;

    // Then it anchors the block as `<Counter>`, rather than being refused
    assert_eq!(
        anchor,
        Ok(Anchor::Item {
            item: a_path("stacks::workflow::<Counter>"),
            file: WORKFLOW.to_string(),
            start: Some(at(5, 1)),
            end: Some(at(5, 1)),
            fingerprint: Fingerprint::of(THE_IMPL_BLOCK_OF_THAT_COUNTER),
            hint: Some(at(9, 1)),
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn anchors_at_inside_the_second_of_two_inherent_impls_emits_it_numbered() {
    // Given two inherent impls of `Counter`, and a position on the gap in the second
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    workspace.rewriting(
        WORKFLOW,
        &format!(
            "{A_COUNTER_WITH_TWO_METHODS_A_GAP_APART}\n{}",
            THE_IMPL_BLOCK_OF_THAT_COUNTER
                .replace("bump", "inc")
                .replace("reset", "clear")
        ),
    );
    let between = Range {
        start: at(19, 1),
        end: at(19, 1),
    };

    // When it is anchored
    let anchor = the_anchor_command_emits(&workspace, WORKFLOW, &[], Some(between)).await;

    // Then the block is named by its ordinal
    assert_eq!(
        anchor.map(|anchor| match anchor {
            Anchor::Item { item, .. } => item.to_string(),
            other => format!("{other:?}"),
        }),
        Ok("stacks::workflow::<Counter>#2".to_string())
    );
}

/// A module file that genuinely defines nothing: the server's outline for it is empty for ever, so
/// no number of retries turns it into one with items in it.
fn a_crate_whose_workflow_module_defines_nothing() -> harness::AFixtureWorkspace {
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    workspace.rewriting(WORKFLOW, "// nothing is declared here yet\n");
    workspace
}

#[tokio::test(flavor = "multi_thread")]
async fn anchors_over_a_file_that_defines_nothing_refuses_the_item_rather_than_waiting() {
    // Given a module whose outline is genuinely empty
    let workspace = a_crate_whose_workflow_module_defines_nothing();

    // When an item is anchored in it
    let anchor = the_anchor_command_emits(&workspace, WORKFLOW, &["Queue"], None).await;

    // Then the item is refused as absent — not an index that never finished, which is what an
    // endless wait on the empty outline ended as once the caller's token fired
    let refusal = anchor.expect_err("an item the file does not declare is refused");
    assert!(
        refusal.contains("Queue") && !refusal.contains("had not finished indexing"),
        "the refusal was not about the missing item:\n{refusal}"
    );
}

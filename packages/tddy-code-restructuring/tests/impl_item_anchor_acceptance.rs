//! An `items` anchor over a struct, its inherent `impl` block and the function after it, against a
//! live rust-analyzer — `<Type>` names the block, so a contiguous run that holds one anchors by
//! items instead of by hand-lined range.
//!
//! What must hold is that the run resolves through the server's own outline, that the adjacency
//! check treats the block like any sibling, and that `extract_module` moves the block *with* its
//! struct and leaves a tree that compiles.
//!
//! Load-sensitive: one server at a time, enforced by the harness.

mod harness;

use harness::{
    a_crate_with_two_inherent_news_and_two_fmts, applying_the_plan_at, assert_compiles,
    the_anchor_command_emits, WORKFLOW,
};
use tddy_code_restructuring::{Anchor, Fingerprint, ItemPath, Reexport, RefactorKind, RefactorOp};

const A_STRUCT_ITS_IMPL_AND_A_FUNCTION: &str = "\
pub const LIMIT: u32 = 9;

pub struct Counter {
    n: u32,
}

impl Counter {
    pub fn bump(&mut self) {
        self.n += 1;
    }
}

pub fn describe(c: &Counter) -> u32 {
    c.n
}

pub fn untouched() -> u32 {
    7
}
";

const THE_NAMES_OF_THAT_RUN: [&str; 3] = ["Counter", "<Counter>", "describe"];

fn a_crate_holding_a_counter_and_its_impl() -> harness::AFixtureWorkspace {
    let workspace = a_crate_with_two_inherent_news_and_two_fmts();
    workspace.rewriting(WORKFLOW, A_STRUCT_ITS_IMPL_AND_A_FUNCTION);
    workspace
}

/// The `extract_module` of the `items` anchor, into a file of its own, re-exported by a glob.
fn a_move_to_a_file_of(anchor: Anchor) -> RefactorOp {
    RefactorOp {
        id: None,
        op: RefactorKind::ExtractModule,
        anchor,
        name: Some("counting".to_string()),
        to: None,
        variant: None,
        with_private_deps: false,
        reexport: Some(Reexport::Glob),
        to_file: true,
        also: Vec::new(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_items_anchor_names_a_struct_its_inherent_impl_and_the_function_after_it() {
    // Given a struct, its `impl` block and a function laid out one after another
    let workspace = a_crate_holding_a_counter_and_its_impl();

    // When the run is anchored, naming the block `<Counter>`
    let anchor = the_anchor_command_emits(&workspace, WORKFLOW, &THE_NAMES_OF_THAT_RUN, None).await;

    // Then it is one `items` anchor, the block fingerprinted over its own lines
    assert_eq!(
        anchor,
        Ok(Anchor::Items {
            file: WORKFLOW.to_string(),
            items: ["Counter", "<Counter>", "describe"]
                .map(|name| ItemPath::parse(&format!("stacks::workflow::{name}")).unwrap())
                .to_vec(),
            fingerprints: vec![
                Fingerprint::of("pub struct Counter {\n    n: u32,\n}"),
                Fingerprint::of(
                    "impl Counter {\n    pub fn bump(&mut self) {\n        self.n += 1;\n    }\n}"
                ),
                Fingerprint::of("pub fn describe(c: &Counter) -> u32 {\n    c.n\n}"),
            ],
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_extract_module_over_such_a_run_moves_the_impl_with_its_struct_and_still_compiles() {
    // Given the anchored run, and a plan moving it into a file of its own
    let workspace = a_crate_holding_a_counter_and_its_impl();
    let anchor = the_anchor_command_emits(&workspace, WORKFLOW, &THE_NAMES_OF_THAT_RUN, None)
        .await
        .expect("the run is anchored");
    let plan = workspace.a_hinted_plan_of(&[a_move_to_a_file_of(anchor)]);

    // When it is applied
    let summary = applying_the_plan_at(&workspace, plan).await;

    // Then the struct, its block and the function live in the new module, which compiles
    assert_eq!(summary.map(|run| run.applied), Ok(1));
    let moved = workspace.read("crates/stacks/src/workflow/counting.rs");
    assert!(
        moved.contains("pub struct Counter") && moved.contains("impl Counter"),
        "the block did not move with its struct:\n{moved}"
    );
    let remaining = workspace.read(WORKFLOW);
    assert!(
        !remaining.contains("impl Counter") && remaining.contains("pub fn untouched"),
        "the block was left behind, or too much moved:\n{remaining}"
    );
    assert_compiles(&workspace);
}

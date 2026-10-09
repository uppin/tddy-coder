//! A static `check` verifies what it can of an item anchor without a server.
//!
//! Resolving an item anchor needs rust-analyzer's outline, so a static check used to report every
//! item-anchored operation as unexamined. One thing about such an anchor needs no server at all:
//! whether the item path lies in the module its file is. A mistyped crate or module used to surface
//! only after minutes of indexing in `check --deep` or `apply`; these suites pin that the cheap
//! check now says so, in the deep resolver's own words, and still says the rest needs `--deep`.
//!
//! No language server is started here: `checking_the_plan(…, false)` passes none.

mod harness;

use harness::{
    a_workspace_holding_files, an_extract_method_at, an_item_anchor, checking_the_plan,
    AFixtureWorkspace,
};
use tddy_code_restructuring::{Anchor, Fingerprint, ItemPath, RefactorKind, RefactorOp};

const WORKFLOW: &str = "src/workflow.rs";
const NOTES: &str = "tests/notes.rs";

const STACK: &str = "pub struct Stack;";

/// The package `stacks`: a crate root declaring `workflow`, a module with a type and its impl, and
/// an integration test file outside `src/`.
fn the_stacks_package() -> AFixtureWorkspace {
    a_workspace_holding_files(&[
        (
            "Cargo.toml",
            "[package]\nname = \"stacks\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("src/lib.rs", "pub mod workflow;\n"),
        (
            WORKFLOW,
            "pub struct Stack;\n\nimpl Stack {\n    pub fn new() -> Self {\n        Stack\n    }\n}\n",
        ),
        (NOTES, "pub fn note() {}\n"),
    ])
}

/// An `extract_method` anchored on the item `item` of `file`, the item itself.
fn an_extraction_anchored_on(file: &str, item: &str) -> RefactorOp {
    an_extract_method_at(an_item_anchor(file, item, STACK, None, None), "extracted")
}

/// An `extract_module` anchored on a run of `items` in `file`.
fn a_module_extraction_of(file: &str, items: &[&str]) -> RefactorOp {
    let mut op = an_extract_method_at(
        Anchor::Items {
            file: file.to_string(),
            items: items
                .iter()
                .map(|item| ItemPath::parse(item).expect("the item path parses"))
                .collect(),
            fingerprints: items.iter().map(|_| Fingerprint::of(STACK)).collect(),
        },
        "moved",
    );
    op.op = RefactorKind::ExtractModule;
    op
}

async fn the_static_findings_of(
    workspace: &AFixtureWorkspace,
    ops: &[RefactorOp],
) -> Result<Vec<String>, String> {
    let plan = workspace.a_hinted_plan_of(ops);
    checking_the_plan(workspace, plan, false).await
}

/// The finding an operation whose prefixes are sound still gets: only a deep check can say the rest.
fn the_deep_check_finding(op: &str, file: &str) -> String {
    format!(
        "{op} in `{file}` anchors by item, which only a deep check can resolve: its module prefix \
         matches the file, and whether the item is there, unambiguous and unchanged needs \
         `check --deep`"
    )
}

#[tokio::test]
async fn a_static_check_refuses_an_item_anchor_whose_module_is_not_its_files_in_the_words_a_deep_check_uses(
) {
    // Given an extraction anchored on a path whose module is not the file's
    let workspace = the_stacks_package();
    let op = an_extraction_anchored_on(WORKFLOW, "stacks::planning::Stack::new");

    // When the plan is checked without a server
    let findings = the_static_findings_of(&workspace, &[op]).await;

    // Then the mismatch is named exactly as `check --deep` names it
    assert_eq!(
        findings,
        Ok(vec![format!(
            "`stacks::planning::Stack::new` is not in {WORKFLOW}, which is module \
             `stacks::workflow`"
        )])
    );
}

#[tokio::test]
async fn a_static_check_refuses_an_item_anchor_whose_crate_is_not_its_files_package() {
    // Given an extraction anchored on a path whose crate is another package's
    let workspace = the_stacks_package();
    let op = an_extraction_anchored_on(WORKFLOW, "queues::workflow::Stack::new");

    // When the plan is checked without a server
    let findings = the_static_findings_of(&workspace, &[op]).await;

    // Then the mismatch is named
    assert_eq!(
        findings,
        Ok(vec![format!(
            "`queues::workflow::Stack::new` is not in {WORKFLOW}, which is module \
             `stacks::workflow`"
        )])
    );
}

#[tokio::test]
async fn a_static_check_refuses_an_item_path_that_names_the_files_module_rather_than_an_item_in_it()
{
    // Given an extraction anchored on the file's own module rather than an item in it
    let workspace = the_stacks_package();
    let op = an_extraction_anchored_on(WORKFLOW, "stacks::workflow");

    // When the plan is checked without a server
    let findings = the_static_findings_of(&workspace, &[op]).await;

    // Then it is refused for naming the module
    assert_eq!(
        findings,
        Ok(vec![format!(
            "`stacks::workflow` names the module {WORKFLOW} is, not an item in it"
        )])
    );
}

#[tokio::test]
async fn a_static_check_refuses_an_items_anchor_naming_the_one_name_outside_its_files_module() {
    // Given a run of two items, the second of another module
    let workspace = the_stacks_package();
    let op = a_module_extraction_of(
        WORKFLOW,
        &["stacks::workflow::Stack", "stacks::planning::Plan"],
    );

    // When the plan is checked without a server
    let findings = the_static_findings_of(&workspace, &[op]).await;

    // Then the one name outside the module is refused
    assert_eq!(
        findings,
        Ok(vec![format!(
            "`stacks::planning::Plan` is not in {WORKFLOW}, which is module `stacks::workflow`"
        )])
    );
}

#[tokio::test]
async fn a_static_check_refuses_an_item_anchor_in_a_file_outside_src() {
    // Given an extraction anchored in a file that is no module of its package
    let workspace = the_stacks_package();
    let op = an_extraction_anchored_on(NOTES, "stacks::notes::note");

    // When the plan is checked without a server
    let findings = the_static_findings_of(&workspace, &[op]).await;

    // Then the file is refused as no module, in the resolver's words
    assert_eq!(
        findings,
        Ok(vec![format!(
            "{NOTES} is not under src, so it is no module of the package `stacks`"
        )])
    );
}

#[tokio::test]
async fn a_static_check_of_an_item_anchor_with_a_sound_prefix_still_says_only_a_deep_check_can_resolve_it_and_that_its_module_was_verified(
) {
    // Given an extraction anchored on an item of the file's own module
    let workspace = the_stacks_package();
    let op = an_extraction_anchored_on(WORKFLOW, "stacks::workflow::Stack");

    // When the plan is checked without a server
    let findings = the_static_findings_of(&workspace, &[op]).await;

    // Then it is still a finding, saying the module was verified and the rest needs a deep check
    assert_eq!(
        findings,
        Ok(vec![the_deep_check_finding("ExtractMethod", WORKFLOW)])
    );
}

#[tokio::test]
async fn an_operation_refused_for_its_prefix_gets_one_finding_not_also_the_deep_check_one() {
    // Given a plan of an operation with a foreign prefix, then one whose prefix is sound
    let workspace = the_stacks_package();
    let foreign = an_extraction_anchored_on(WORKFLOW, "stacks::planning::Stack::new");
    let sound = an_extraction_anchored_on(WORKFLOW, "stacks::workflow::Stack");

    // When the plan is checked without a server
    let findings = the_static_findings_of(&workspace, &[foreign, sound]).await;

    // Then each operation has exactly one finding: the refusal, then the deep-check one
    assert_eq!(
        findings,
        Ok(vec![
            format!(
                "`stacks::planning::Stack::new` is not in {WORKFLOW}, which is module \
                 `stacks::workflow`"
            ),
            the_deep_check_finding("ExtractMethod", WORKFLOW),
        ])
    );
}

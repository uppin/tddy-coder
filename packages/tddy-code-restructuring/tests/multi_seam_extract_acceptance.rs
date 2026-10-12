//! A plan that cuts several `extract_module` seams out of one file, where a later seam reaches what
//! an earlier one wrote, against a live rust-analyzer.
//!
//! Every operation after the first is about a tree the earlier ones changed, and the server has to be
//! shown that tree: a rehearsal (`check --deep`, `apply --dry-run`) keeps the earlier seams' files in
//! memory only, and on this repository's workspace rust-analyzer's own watcher does not see an apply's
//! writes within one request. Unseen, a sibling seam's reference was missing from the survey, the
//! import pass and the visibility pass all at once (`#carve` 3: `check --deep` clean, 8 of 8 applied,
//! 13 compile errors; `#carve` 13: `use super::strip_resize;` unresolved).
//!
//! The rehearsal tests are the ones that fail without the projection on every machine: a rehearsal's
//! files never reach the disk. The apply tests are pins — on a fixture this small rust-analyzer's own
//! watcher may already see what an apply wrote.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_crate_cut_three_ways_like_carve_3, a_crate_whose_mover_calls_relative_from,
    a_crate_whose_pty_handle_calls_strip_resize, a_seam_of_outer, applying_keeping_the_account,
    assert_compiles, assert_compiles_with_its_tests, checking_deep_keeping_the_account,
    resolving_after_a_check_of, resolving_in_order, the_widenings_in, AFixtureWorkspace, MOVING,
    RELATIVE_FROM, STRIP_RESIZE, STRIP_RESIZE_AFTER_THE_REFUSALS, THE_MOVING_FILES_FILE,
    THE_PTY_HANDLE_AND_ITS_IMPL, THE_PTY_HANDLE_FILE, THE_REFUSALS, THE_RESIZE_FILE,
};
use tddy_code_restructuring::{Reexport, RefactorOp};

/// The widening the second seam owes `pty_handle.rs`, as a run's account states it.
const STRIP_RESIZE_WIDENED: &str = "   visibility: `strip_resize` private -> pub(crate)";

/// The pty plan: `PtyHandle` into `pty_handle` behind a glob, then `strip_resize` into `resize` behind
/// the facade `strip_resize_reexport` names.
fn the_pty_plan(workspace: &AFixtureWorkspace, strip_resize_reexport: Reexport) -> Vec<RefactorOp> {
    vec![
        a_seam_of_outer(
            workspace,
            THE_PTY_HANDLE_AND_ITS_IMPL,
            "pty_handle",
            Reexport::Glob,
        ),
        a_seam_of_outer(workspace, STRIP_RESIZE, "resize", strip_resize_reexport),
    ]
}

/// The mover plan: `relative_from` into `paths`, then `moving` into `moving_files`, both behind a glob.
fn the_mover_plan(workspace: &AFixtureWorkspace) -> Vec<RefactorOp> {
    vec![
        a_seam_of_outer(workspace, RELATIVE_FROM, "paths", Reexport::Glob),
        a_seam_of_outer(workspace, MOVING, "moving_files", Reexport::Glob),
    ]
}

/// The three-seam plan of the `#carve` 3 shape, every seam behind a glob.
fn the_three_seam_plan(workspace: &AFixtureWorkspace) -> Vec<RefactorOp> {
    vec![
        a_seam_of_outer(
            workspace,
            THE_PTY_HANDLE_AND_ITS_IMPL,
            "pty_handle",
            Reexport::Glob,
        ),
        a_seam_of_outer(workspace, THE_REFUSALS, "refusals", Reexport::Glob),
        a_seam_of_outer(
            workspace,
            STRIP_RESIZE_AFTER_THE_REFUSALS,
            "resize",
            Reexport::Glob,
        ),
    ]
}

/// The shape the second seam of the mover plan reaches `relative_from` by: the first seam's assist
/// wrote the parent's call as `paths::relative_from`, and the import pass binds the sibling module —
/// as `use super::paths;` or `use crate::outer::paths;`, whichever the server's answer leads to.
fn reaches_relative_from_through_its_sibling(text: &str) -> bool {
    let binds_the_sibling = text
        .lines()
        .any(|line| line.starts_with("use ") && line.ends_with("::paths;"));
    binds_the_sibling && text.contains("paths::relative_from(\"/\", path)")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_refuses_a_seam_that_strands_a_reference_from_a_file_an_earlier_seam_wrote() {
    // Given a plan whose second seam moves `strip_resize` with no facade, while the file the first
    // seam writes reaches it in the parent
    let workspace = a_crate_whose_pty_handle_calls_strip_resize();
    let plan = the_pty_plan(&workspace, Reexport::None);

    // When the plan is checked deep
    let (findings, _) = checking_deep_keeping_the_account(&workspace, &plan).await;

    // Then the second seam is refused, naming the item and the sibling file that would lose it
    let findings = findings.expect("the plan is checked");
    assert_eq!(findings.len(), 1, "expected one finding: {findings:#?}");
    assert!(
        findings[0].contains("`strip_resize`") && findings[0].contains("outer/pty_handle.rs"),
        "the finding does not name the stranded item and its file: {}",
        findings[0]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_apply_refuses_the_same_seam_after_applying_the_first() {
    // Given the same plan
    let workspace = a_crate_whose_pty_handle_calls_strip_resize();
    let plan = the_pty_plan(&workspace, Reexport::None);

    // When it is applied
    let (applied, _) = applying_keeping_the_account(&workspace, &plan, false).await;

    // Then the run stops at the second seam with the stranded refusal, after writing the first
    let refusal = applied.expect_err("the second seam is refused");
    assert!(
        refusal.contains("`strip_resize`") && refusal.contains("outer/pty_handle.rs"),
        "the refusal does not name the stranded item and its file: {refusal}"
    );
    assert!(workspace.holds(THE_PTY_HANDLE_FILE) && !workspace.holds(THE_RESIZE_FILE));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_reports_the_widening_a_file_an_earlier_seam_wrote_forces() {
    // Given the plan with `strip_resize` moved behind a glob
    let workspace = a_crate_whose_pty_handle_calls_strip_resize();
    let plan = the_pty_plan(&workspace, Reexport::Glob);

    // When it is applied as a dry run
    let (applied, account) = applying_keeping_the_account(&workspace, &plan, true).await;

    // Then `strip_resize` stays widened for `pty_handle.rs`, and the run says so
    assert_eq!(applied.map(|run| run.applied), Ok(2));
    assert!(
        the_widenings_in(&account).contains(&STRIP_RESIZE_WIDENED.to_string()),
        "the widening `pty_handle.rs` needs was not reported: {account:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_reports_the_same_widenings_a_dry_run_reports() {
    // Given the plan with `strip_resize` moved behind a glob
    let workspace = a_crate_whose_pty_handle_calls_strip_resize();
    let plan = the_pty_plan(&workspace, Reexport::Glob);

    // When it is checked deep and applied as a dry run
    let (findings, checked) = checking_deep_keeping_the_account(&workspace, &plan).await;
    let (_, rehearsed) = applying_keeping_the_account(&workspace, &plan, true).await;

    // Then the check found nothing and printed the very widenings the dry run printed
    assert_eq!(findings, Ok(Vec::new()));
    assert_eq!(
        the_widenings_in(&checked),
        vec![STRIP_RESIZE_WIDENED.to_string()]
    );
    assert_eq!(the_widenings_in(&checked), the_widenings_in(&rehearsed));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plan_whose_earlier_seam_reaches_what_a_later_seam_moves_applies_and_compiles() {
    // Given the plan with `strip_resize` moved behind a glob
    let workspace = a_crate_whose_pty_handle_calls_strip_resize();
    let plan = the_pty_plan(&workspace, Reexport::Glob);

    // When it is applied
    let (applied, _) = applying_keeping_the_account(&workspace, &plan, false).await;

    // Then both seams land, `strip_resize` keeps the reach `pty_handle.rs` needs, and it compiles
    assert_eq!(applied.map(|run| run.applied), Ok(2));
    let resize = workspace.read(THE_RESIZE_FILE);
    assert!(
        resize.contains("pub(crate) fn strip_resize"),
        "`strip_resize` was narrowed back to private:\n{resize}"
    );
    assert_compiles(&workspace);
}

/// Row 2 of the 2026-09-18 record ("helpers left unqualified in a later seam") no longer reproduces:
/// the assist qualifies every in-file use of what an earlier seam moved, and `inline_paths` re-roots
/// such a path in a later seam. A pin, so the projection does not disturb it.
#[tokio::test(flavor = "multi_thread")]
async fn a_later_seam_reaches_a_helper_an_earlier_seam_moved_when_the_plan_is_resolved_without_writing(
) {
    // Given a plan whose second seam moves code calling the helper the first seam moved
    let workspace = a_crate_whose_mover_calls_relative_from();
    let plan = the_mover_plan(&workspace);

    // When the plan is resolved without writing, as a deep check resolves it
    let resolved = resolving_in_order(&workspace, &plan)
        .await
        .unwrap_or_else(|refusal| panic!("the plan was refused: {refusal}"));

    // Then the second seam's module reaches the helper through its sibling module
    let moved = resolved.text(THE_MOVING_FILES_FILE);
    assert!(
        reaches_relative_from_through_its_sibling(&moved),
        "the second seam's module does not reach `relative_from`:\n{moved}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_plan_applied_reaches_the_helper_and_compiles() {
    // Given the same plan
    let workspace = a_crate_whose_mover_calls_relative_from();
    let plan = the_mover_plan(&workspace);

    // When it is applied
    let (applied, _) = applying_keeping_the_account(&workspace, &plan, false).await;

    // Then both seams land, the second reaching the helper through its sibling, and it compiles
    assert_eq!(applied.map(|run| run.applied), Ok(2));
    let moved = workspace.read(THE_MOVING_FILES_FILE);
    assert!(
        reaches_relative_from_through_its_sibling(&moved),
        "the second seam's module does not reach `relative_from`:\n{moved}"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_three_seam_plan_of_the_carve_shape_widens_the_same_items_in_its_deep_check_and_its_apply_and_compiles_with_its_tests(
) {
    // Given three seams that reach one another
    let workspace = a_crate_cut_three_ways_like_carve_3();
    let plan = the_three_seam_plan(&workspace);

    // When the plan is checked deep and then applied
    let (findings, checked) = checking_deep_keeping_the_account(&workspace, &plan).await;
    let (applied, account) = applying_keeping_the_account(&workspace, &plan, false).await;

    // Then the check is clean and printed the widenings the apply reported, and the tree compiles
    assert_eq!(findings, Ok(Vec::new()));
    assert_eq!(applied.map(|run| run.applied), Ok(3));
    assert!(
        the_widenings_in(&account).contains(&STRIP_RESIZE_WIDENED.to_string()),
        "the apply did not keep `strip_resize` widened for its two sibling seams: {account:#?}"
    );
    assert_eq!(the_widenings_in(&checked), the_widenings_in(&account));
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_check_leaves_no_projected_document_open_on_the_server_it_shares() {
    // Given a server a deep check of the pty plan has run against
    let workspace = a_crate_whose_pty_handle_calls_strip_resize();
    let rehearsed = the_pty_plan(&workspace, Reexport::Glob);
    let lone_seam = a_seam_of_outer(&workspace, STRIP_RESIZE, "resize", Reexport::None);

    // When the second seam alone is resolved on the untouched tree, with no facade
    let resolved = resolving_after_a_check_of(&workspace, &rehearsed, lone_seam).await;

    // Then it is not refused: the rehearsed `pty_handle.rs`, which would strand `strip_resize`, is
    // no longer in front of the server
    assert!(
        resolved.is_ok(),
        "the lone seam was refused, as though the rehearsed sibling were still open: {resolved:?}"
    );
}

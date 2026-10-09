//! A move of a file git does not track is refused before its operation writes anything.
//!
//! Moves go through `git mv`, so history survives. `git mv` refuses a file git has never seen, and
//! the apply used to find that out only after the operation's text edits were on disk: the tree was
//! half edited, the journal held an in-flight record that matched neither side, and the next run was
//! refused. `#unbundle` 4 hit it on a module authored earlier in the same PR.
//!
//! Driven through the library's `apply` and `check` with a test-binary move over the deterministic
//! fake server: the engine authors that move without asking the server anything, so the git
//! preflight is all these suites exercise.

mod harness;

use harness::{
    a_workspace_whose_test_binary_is_untracked, checking_a_move_of_the_test_binary,
    moving_the_test_binary, THE_TEST_BINARY,
};
use tddy_code_restructuring::runner::RunSummary;
use tddy_code_restructuring::{state_directory_for_plan, Journal};

/// What a refusal of an untracked move says, whichever entry point gives it.
const UNTRACKED: &str = "which git does not track";

/// The destination manifest a test-binary move may edit.
const DESTINATION_MANIFEST: &str = "crates/destination/Cargo.toml";

fn git_add(fixture: &harness::AFixtureWorkspace, relative: &str) {
    let status = std::process::Command::new("git")
        .args(["add", relative])
        .current_dir(fixture.path())
        .status()
        .expect("git runs");
    assert!(status.success(), "git add {relative} failed");
}

fn the_records_of_the_plans_journal(fixture: &harness::AFixtureWorkspace) -> usize {
    let state = state_directory_for_plan(fixture.path(), &fixture.path().join("plan.jsonl"))
        .expect("the plan has a state directory");
    Journal::load(&state.join("journal.jsonl"))
        .expect("the journal loads")
        .records
        .len()
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_to_move_a_test_binary_git_does_not_track_naming_it_and_git_add() {
    // Given a test binary on disk that was never added to git
    let workspace = a_workspace_whose_test_binary_is_untracked();

    // When its move is applied
    let (outcome, _) = moving_the_test_binary(&workspace, false).await;

    // Then the apply is refused, naming the file and the remedy
    let refusal = outcome.expect_err("a move of an untracked file is refused");
    assert!(refusal.contains(UNTRACKED), "{refusal}");
    assert!(refusal.contains(THE_TEST_BINARY), "{refusal}");
    assert!(refusal.contains("`git add` it"), "{refusal}");
}

#[tokio::test(flavor = "multi_thread")]
async fn leaves_every_file_byte_identical_when_an_untracked_move_is_refused() {
    // Given an untracked test binary, and what the files it and the destination hold
    let workspace = a_workspace_whose_test_binary_is_untracked();
    let before = (
        workspace.read(THE_TEST_BINARY),
        workspace.read(DESTINATION_MANIFEST),
    );

    // When its move is applied and refused
    let _refused = moving_the_test_binary(&workspace, false).await;

    // Then both files are as they were, and the journal records no operation in flight
    assert_eq!(
        (
            workspace.read(THE_TEST_BINARY),
            workspace.read(DESTINATION_MANIFEST)
        ),
        before
    );
    assert_eq!(the_records_of_the_plans_journal(&workspace), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn applies_the_same_plan_once_the_file_is_added_without_resume_or_clean_up() {
    // Given a move refused because its test binary was untracked
    let workspace = a_workspace_whose_test_binary_is_untracked();
    let _refused = moving_the_test_binary(&workspace, false).await;

    // When the file is added and the same plan is applied again, fresh
    git_add(&workspace, THE_TEST_BINARY);
    let (outcome, _) = moving_the_test_binary(&workspace, false).await;

    // Then the move lands
    assert_eq!(
        outcome,
        Ok(RunSummary {
            applied: 1,
            total: 1,
            stopped_early: false,
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_refuses_an_untracked_move_the_same_way() {
    // Given an untracked test binary
    let workspace = a_workspace_whose_test_binary_is_untracked();

    // When its move is rehearsed with --dry-run
    let (outcome, _) = moving_the_test_binary(&workspace, true).await;

    // Then the dry run gives the refusal the apply would
    let refusal = outcome.expect_err("a dry run refuses what the apply would");
    assert!(refusal.contains(UNTRACKED), "{refusal}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deep_check_reports_an_untracked_move_as_a_finding() {
    // Given an untracked test binary
    let workspace = a_workspace_whose_test_binary_is_untracked();

    // When the plan is checked deep
    let findings = checking_a_move_of_the_test_binary(&workspace, true)
        .await
        .expect("a check reports findings rather than failing");

    // Then one finding says the file is untracked
    assert!(
        findings.iter().any(|finding| finding.contains(UNTRACKED)),
        "{findings:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_static_check_reports_an_untracked_anchor_file() {
    // Given an untracked test binary
    let workspace = a_workspace_whose_test_binary_is_untracked();

    // When the plan is checked statically
    let findings = checking_a_move_of_the_test_binary(&workspace, false)
        .await
        .expect("a check reports findings rather than failing");

    // Then one finding names the untracked anchor file
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains(UNTRACKED) && finding.contains(THE_TEST_BINARY)),
        "{findings:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_resolving_line_names_the_operations_id() {
    // Given a tracked test binary, moved by a plan the run gives ids
    let workspace = harness::a_workspace_whose_test_binary_stands_alone();

    // When its move is applied
    let (_, said) = moving_the_test_binary(&workspace, false).await;

    // Then the line that announces the operation names its id beside its index
    let resolving: Vec<&String> = said
        .iter()
        .filter(|line| line.contains(": resolving"))
        .collect();
    assert!(
        resolving
            .iter()
            .any(|line| line.starts_with("op 0 (") && line.contains(") of 1: resolving")),
        "{resolving:#?}"
    );
}

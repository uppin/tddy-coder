//! What the coder stamps on every activity record it writes — AC1 and AC2 of
//! `docs/ft/daemon/session-worktree-sync.md`.
//!
//! Real git repositories in temp directories, and the commit each record carries is compared with
//! what `git rev-parse HEAD` says for the same checkout — so the test cannot agree with a wrong
//! implementation by sharing its mistake.
//!
//! A child module of `presenter_impl` rather than a file under `tests/` because the presenter's
//! event input is private: an integration test can neither reach `workflow_event_rx` nor build the
//! `#[cfg(test)]` recipe a `Presenter` needs, and widening either to public would shape the shipped
//! API around a test. The events are still fed the way the workflow thread feeds them, so what is
//! exercised is the real `ProgressEvent` path and not a helper called directly.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;

use pretty_assertions::assert_eq;

use super::Presenter;
use crate::agent_activity::{read_agent_activity, AgentActivityRecord, STATUS_COMPLETED};
use crate::presenter::presenter_test_recipe::EmptyPresenterTestRecipe;
use crate::presenter::WorkflowEvent;
use crate::workflow::recipe::WorkflowRecipe;
use crate::ProgressEvent;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A checkout with one commit on `main`, as a session worktree is. Returns the commit it is on.
fn a_worktree_with_one_commit(root: &Path) -> String {
    git(root, &["init", "--initial-branch=main"]);
    git(root, &["config", "user.email", "agent@example.com"]);
    git(root, &["config", "user.name", "Agent"]);
    a_commit_adding(root, "README.md", "one\n")
}

/// One more commit on top, so a test can tell a stamp that was *kept* from one that was re-read.
fn a_later_commit(root: &Path) -> String {
    a_commit_adding(root, "second.txt", "two\n")
}

fn a_commit_adding(root: &Path, file: &str, contents: &str) -> String {
    std::fs::write(root.join(file), contents).expect("write the file to commit");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", file]);
    head_according_to_git(root)
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// What git itself says HEAD is, which is the only thing worth comparing a stamp against.
fn head_according_to_git(root: &Path) -> String {
    git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

/// A presenter that persists this session's activity and knows which checkout the agent edits —
/// the wiring `tddy-coder` does for a tool / cursor-cli session.
fn a_presenter_stamping_against(session_dir: &Path, worktree: &Path) -> Presenter {
    a_presenter(session_dir, Some(worktree.to_path_buf()))
}

/// A presenter told where to log but never told which checkout the calls run in.
fn a_presenter_with_no_worktree(session_dir: &Path) -> Presenter {
    a_presenter(session_dir, None)
}

fn a_presenter(session_dir: &Path, worktree: Option<PathBuf>) -> Presenter {
    let mut presenter = Presenter::new(
        "cursor",
        "model",
        std::sync::Arc::new(EmptyPresenterTestRecipe) as std::sync::Arc<dyn WorkflowRecipe>,
        session_dir.to_path_buf(),
    );
    presenter.set_agent_activity_context(session_dir.to_path_buf(), worktree, "coder");
    presenter
}

fn a_tool_use(call_id: &str, tool_name: &str, input: serde_json::Value) -> WorkflowEvent {
    WorkflowEvent::Progress(ProgressEvent::ToolUse {
        name: tool_name.to_string(),
        detail: None,
        input_json: Some(input.to_string()),
        call_id: Some(call_id.to_string()),
    })
}

fn a_tool_result(call_id: &str) -> WorkflowEvent {
    WorkflowEvent::Progress(ProgressEvent::ToolResult {
        call_id: call_id.to_string(),
        result_json: r#"{"stdout":"ok"}"#.to_string(),
        is_error: false,
    })
}

/// An absolute path inside `worktree`, the form a tool reports the file it will write.
fn inside(worktree: &Path, rel: &str) -> serde_json::Value {
    serde_json::json!({ "file_path": worktree.join(rel).to_string_lossy() })
}

// ---------------------------------------------------------------------------
// Driving the presenter
// ---------------------------------------------------------------------------

/// Hand the presenter the progress events the workflow thread would send, and let it process them.
fn the_agent_makes(presenter: &mut Presenter, events: Vec<WorkflowEvent>) {
    let (tx, rx) = mpsc::channel();
    for event in events {
        tx.send(event).expect("queue the workflow event");
    }
    drop(tx);
    presenter.workflow_event_rx = Some(rx);
    presenter.poll_workflow();
}

/// The single call recorded in this session's `agent-activity.jsonl`, running and terminal rows
/// already coalesced by the read side.
fn the_recorded_call(session_dir: &Path) -> AgentActivityRecord {
    let records = read_agent_activity(session_dir).expect("must read the activity log");
    assert_eq!(records.len(), 1, "expected exactly one recorded call");
    records.into_iter().next().expect("the one recorded call")
}

// ---------------------------------------------------------------------------
// The commit a call ran upon
// ---------------------------------------------------------------------------

#[test]
fn stamps_a_call_with_the_commit_its_worktree_is_on() {
    // Given a session whose agent works in a real checkout
    let session = tempfile::tempdir().expect("tempdir");
    let worktree = tempfile::tempdir().expect("tempdir");
    a_worktree_with_one_commit(worktree.path());
    let mut presenter = a_presenter_stamping_against(session.path(), worktree.path());

    // When the agent makes a call
    the_agent_makes(
        &mut presenter,
        vec![a_tool_use(
            "call-1",
            "Read",
            inside(worktree.path(), "README.md"),
        )],
    );

    // Then the record names the commit git names for that checkout. Read from the filesystem
    // rather than by `rev-parse`, but it has to be the same answer or a consumer places the call's
    // change against a state it was never cut from.
    assert_eq!(
        the_recorded_call(session.path()).head_commit,
        head_according_to_git(worktree.path())
    );
}

#[test]
fn keeps_the_commit_a_call_started_on_when_that_call_finishes() {
    // Given a call that started on one commit
    let session = tempfile::tempdir().expect("tempdir");
    let worktree = tempfile::tempdir().expect("tempdir");
    let started_on = a_worktree_with_one_commit(worktree.path());
    let mut presenter = a_presenter_stamping_against(session.path(), worktree.path());
    the_agent_makes(
        &mut presenter,
        vec![a_tool_use(
            "call-1",
            "Bash",
            serde_json::json!({ "command": "git commit -am work" }),
        )],
    );
    // and a checkout whose HEAD moved while the call ran — the case a commit made by the call
    // itself produces, and the only one that tells a kept stamp from a re-read one.
    let finished_on = a_later_commit(worktree.path());
    assert_ne!(
        started_on, finished_on,
        "the setup must move HEAD or this test proves nothing"
    );

    // When the call finishes
    the_agent_makes(&mut presenter, vec![a_tool_result("call-1")]);

    // Then the terminal row still names the commit the call ran upon, not the one it left behind.
    let record = the_recorded_call(session.path());
    assert_eq!(record.status, STATUS_COMPLETED);
    assert_eq!(record.head_commit, started_on);
}

#[test]
fn leaves_the_commit_empty_when_it_was_never_told_which_checkout_to_read() {
    // Given a presenter wired to log but not to a worktree
    let session = tempfile::tempdir().expect("tempdir");
    let mut presenter = a_presenter_with_no_worktree(session.path());

    // When the agent makes a call
    the_agent_makes(
        &mut presenter,
        vec![a_tool_use(
            "call-1",
            "Edit",
            serde_json::json!({ "file_path": "/somewhere/src/lib.rs" }),
        )],
    );

    // Then the record says it does not know its base rather than naming some other directory's
    // HEAD, which would be a real sha for the wrong tree.
    assert_eq!(the_recorded_call(session.path()).head_commit, "");
}

// ---------------------------------------------------------------------------
// The paths a call is credited with
// ---------------------------------------------------------------------------

#[test]
fn credits_an_edit_with_the_file_it_declared_relative_to_the_worktree() {
    // Given an agent editing a file in its checkout
    let session = tempfile::tempdir().expect("tempdir");
    let worktree = tempfile::tempdir().expect("tempdir");
    a_worktree_with_one_commit(worktree.path());
    let mut presenter = a_presenter_stamping_against(session.path(), worktree.path());

    // When
    the_agent_makes(
        &mut presenter,
        vec![a_tool_use(
            "call-1",
            "Edit",
            inside(worktree.path(), "src/lib.rs"),
        )],
    );

    // Then the call is credited with that file, relative to the worktree — the form a pathspec and
    // a diff header both speak, and an absolute path matches neither.
    assert_eq!(
        the_recorded_call(session.path()).changed_paths,
        vec!["src/lib.rs".to_string()]
    );
}

#[test]
fn credits_a_bash_with_no_paths() {
    // Given a shell command, which declares nothing about what it writes
    let session = tempfile::tempdir().expect("tempdir");
    let worktree = tempfile::tempdir().expect("tempdir");
    a_worktree_with_one_commit(worktree.path());
    let mut presenter = a_presenter_stamping_against(session.path(), worktree.path());

    // When
    the_agent_makes(
        &mut presenter,
        vec![a_tool_use(
            "call-1",
            "Bash",
            serde_json::json!({ "command": "cargo fmt" }),
        )],
    );

    // Then nothing is credited to it. Whatever a formatter changed reaches a consumer through the
    // tick's residual delta; guessing here would hand this call a patch belonging to another.
    assert_eq!(
        the_recorded_call(session.path()).changed_paths,
        Vec::<String>::new()
    );
}

#[test]
fn credits_no_paths_when_it_was_never_told_which_checkout_to_read() {
    // Given a presenter wired to log but not to a worktree
    let session = tempfile::tempdir().expect("tempdir");
    let mut presenter = a_presenter_with_no_worktree(session.path());

    // When the agent edits a file
    the_agent_makes(
        &mut presenter,
        vec![a_tool_use(
            "call-1",
            "Edit",
            serde_json::json!({ "file_path": "/somewhere/src/lib.rs" }),
        )],
    );

    // Then it is credited with nothing: these paths exist only as a relation to a checkout, and
    // with no checkout there is nothing to express them against.
    assert_eq!(
        the_recorded_call(session.path()).changed_paths,
        Vec::<String>::new()
    );
}

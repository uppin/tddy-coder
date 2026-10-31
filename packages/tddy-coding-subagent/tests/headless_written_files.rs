//! Unit tests: the files a headless conversation's plan wrote **outside** its directory — a
//! worktree's `.cursor/mcp.json` — are recorded with the conversation, and its close and the
//! start-up sweep remove what is recorded (`#agent-tools 9/12`).
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-cursor-codex-oz-from-specs.md`;
//! `docs/dev/todo/2026-10-25-cursor-project-mcp-config-outlives-its-conversation.md`.

use std::path::PathBuf;

use pretty_assertions::assert_eq;
use tddy_coding_subagent::launcher::{
    claim_headless_conversation_dir, close_headless_conversation, headless_conversation_dir,
    record_written_files, remove_recorded_files, sweep_headless_dirs, HEADLESS_WRITTEN_FILES,
};

const THIS_DAEMON: &str = "mac-desktop";
const CONVERSATION: &str = "conv-1";

/// A data directory and a worktree, with conversation [`CONVERSATION`] of this daemon claimed and
/// its worktree `.cursor/mcp.json` written and recorded.
struct AConversationThatWroteIntoItsWorktree {
    data_dir: tempfile::TempDir,
    worktree: tempfile::TempDir,
}

fn a_conversation_that_wrote_into_its_worktree() -> AConversationThatWroteIntoItsWorktree {
    let data_dir = tempfile::tempdir().expect("data dir");
    let worktree = tempfile::tempdir().expect("worktree");
    let dir = headless_conversation_dir(data_dir.path(), CONVERSATION);
    claim_headless_conversation_dir(&dir, THIS_DAEMON).expect("claim the conversation dir");
    let written = worktree.path().join(".cursor").join("mcp.json");
    std::fs::create_dir_all(written.parent().expect("a parent")).expect(".cursor");
    record_written_files(&dir, std::slice::from_ref(&written)).expect("record");
    std::fs::write(&written, "{\"mcpServers\":{}}").expect("the worktree file");
    AConversationThatWroteIntoItsWorktree { data_dir, worktree }
}

impl AConversationThatWroteIntoItsWorktree {
    fn dir(&self) -> PathBuf {
        headless_conversation_dir(self.data_dir.path(), CONVERSATION)
    }

    fn worktree_file(&self) -> PathBuf {
        self.worktree.path().join(".cursor").join("mcp.json")
    }
}

#[test]
fn recorded_files_are_listed_one_absolute_path_per_line_after_what_an_earlier_plan_recorded() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    let dir = headless_conversation_dir(data_dir.path(), CONVERSATION);
    claim_headless_conversation_dir(&dir, THIS_DAEMON).expect("claim");

    // When
    record_written_files(&dir, &[PathBuf::from("/w/a/.cursor/mcp.json")]).expect("first record");
    record_written_files(&dir, &[PathBuf::from("/w/b/.cursor/mcp.json")]).expect("second record");

    // Then
    assert_eq!(
        std::fs::read_to_string(dir.join(HEADLESS_WRITTEN_FILES)).expect("the record"),
        "/w/a/.cursor/mcp.json\n/w/b/.cursor/mcp.json\n"
    );
}

#[test]
fn removing_recorded_files_removes_each_one_and_skips_one_already_gone() {
    // Given — two recorded files, one of them already removed by hand
    let given = a_conversation_that_wrote_into_its_worktree();
    let gone = given.worktree.path().join("gone.json");
    record_written_files(&given.dir(), std::slice::from_ref(&gone)).expect("record");

    // When
    let removed = remove_recorded_files(&given.dir()).expect("the removal succeeds");

    // Then
    assert_eq!(
        (removed, given.worktree_file().exists()),
        (vec![given.worktree_file()], false)
    );
}

#[test]
fn a_conversation_without_a_record_removes_nothing() {
    // Given — the positive control: a claimed directory, nothing recorded
    let data_dir = tempfile::tempdir().expect("data dir");
    let dir = headless_conversation_dir(data_dir.path(), CONVERSATION);
    claim_headless_conversation_dir(&dir, THIS_DAEMON).expect("claim");

    // When
    let removed = remove_recorded_files(&dir).expect("the removal succeeds");

    // Then
    assert_eq!(removed, Vec::<PathBuf>::new());
}

#[test]
fn closing_a_conversation_removes_its_recorded_worktree_file_and_its_directory() {
    // Given
    let given = a_conversation_that_wrote_into_its_worktree();
    assert!(given.worktree_file().exists(), "the positive control");

    // When
    let removed =
        close_headless_conversation(given.data_dir.path(), CONVERSATION).expect("the close");

    // Then
    assert_eq!(
        (
            removed,
            given.worktree_file().exists(),
            given.dir().exists()
        ),
        (vec![given.worktree_file()], false, false)
    );
}

#[test]
fn closing_a_conversation_that_has_no_directory_is_not_an_error() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");

    // When
    let removed = close_headless_conversation(data_dir.path(), CONVERSATION);

    // Then
    assert_eq!(
        removed.map_err(|e| e.to_string()),
        Ok(Vec::<PathBuf>::new())
    );
}

#[test]
fn the_start_up_sweep_removes_a_recorded_worktree_file_of_a_conversation_this_daemon_lost() {
    // Given
    let given = a_conversation_that_wrote_into_its_worktree();

    // When
    let swept = sweep_headless_dirs(given.data_dir.path(), THIS_DAEMON).expect("the sweep");

    // Then
    assert_eq!(
        (swept, given.worktree_file().exists(), given.dir().exists()),
        (vec![CONVERSATION.to_string()], false, false)
    );
}

#[test]
fn the_start_up_sweep_leaves_another_daemons_recorded_worktree_file_alone() {
    // Given
    let given = a_conversation_that_wrote_into_its_worktree();

    // When
    let swept = sweep_headless_dirs(given.data_dir.path(), "another-daemon").expect("the sweep");

    // Then
    assert_eq!(
        (swept, given.worktree_file().exists()),
        (Vec::<String>::new(), true)
    );
}

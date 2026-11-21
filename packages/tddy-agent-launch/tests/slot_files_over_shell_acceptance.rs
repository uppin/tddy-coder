//! An agent whose worktree is on a remote target gets its slot files and its session pointer there
//! through the remote-shell port — here the in-memory shell, so nothing is spawned.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-shell-port.md

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_agent_launch::slot_files_over_shell::{
    place_session_pointer_through, write_slot_files_through,
};
use tddy_core::session_pointer::{SessionPointer, SessionPointerOwner};
use tddy_remote_shell::{ExecOutput, FileMode, InMemoryShell};

const WORKTREE: &str = "/home/dev/repo/.worktrees/sess";
const HOOKS_FILE: &str = "/home/dev/repo/.worktrees/sess/.tddy/hooks/sess/claude-settings.json";
const POINTER_FILE: &str = "/home/dev/repo/.worktrees/sess/.tddy/session.json";

/// A pointer for an agent that notifies its operator, owned by a daemon on another host.
fn a_session_pointer() -> SessionPointer {
    SessionPointer {
        socket: None,
        repo_dir: PathBuf::from(WORKTREE),
        session_dir: None,
        remote: BTreeMap::new(),
        notify: BTreeMap::from([("TDDY_SESSION_ID".to_string(), "sess".to_string())]),
        owner: Some(SessionPointerOwner {
            pid: 4242,
            host: "daemon-host".to_string(),
        }),
    }
}

/// A target whose worktree is a git checkout: the command that keeps the pointer out of commits
/// succeeds there.
fn a_target_with_a_checkout() -> InMemoryShell {
    let shell = InMemoryShell::new("buildbox");
    shell.answers_when(|request| {
        (request.command.contains("info/exclude") && request.command.contains(WORKTREE))
            .then(|| ExecOutput::succeeded(""))
    });
    shell
}

#[tokio::test]
async fn slot_files_and_the_session_pointer_are_written_through_the_remote_shell() {
    // Given a remote target, a launch's hooks slot file, and its session pointer
    let shell = a_target_with_a_checkout();
    let slot_files = vec![(PathBuf::from(HOOKS_FILE), b"{\"hooks\":{}}".to_vec())];
    let pointer = a_session_pointer();

    // When both are written through the shell
    write_slot_files_through(&shell, &slot_files)
        .await
        .expect("the slot files are written");
    let placed = place_session_pointer_through(&shell, &pointer, Path::new(WORKTREE))
        .await
        .expect("the session pointer is placed");

    // Then the target holds the slot file as rendered
    assert_eq!(shell.file(HOOKS_FILE), Some(b"{\"hooks\":{}}".to_vec()));
    assert_eq!(shell.file_mode(HOOKS_FILE), Some(FileMode::TargetDefault));

    // And the pointer, owner-only, where the worktree's tools look for it
    assert_eq!(placed, PathBuf::from(POINTER_FILE));
    let on_target: SessionPointer = serde_json::from_slice(
        &shell
            .file(POINTER_FILE)
            .expect("the pointer is on the target"),
    )
    .expect("the pointer on the target is a session pointer");
    assert_eq!(on_target, pointer);
    assert_eq!(shell.file_mode(POINTER_FILE), Some(FileMode::OwnerOnly));

    // And the one command it ran kept the pointer out of the checkout's commits
    let commands = shell.commands_run();
    assert_eq!(commands.len(), 1, "commands run: {commands:?}");
    assert!(
        commands[0].contains("info/exclude") && commands[0].contains("'/.tddy/session.json'"),
        "the command excludes the pointer from git: {}",
        commands[0]
    );
}

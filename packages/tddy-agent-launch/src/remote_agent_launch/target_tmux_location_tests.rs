//! Unit tests: the tmux of a session's target as a [`TmuxLocation`] (`#target-reconnect` 2/3) —
//! the tri-state probe (an ssh exit 255 is *unreachable*, never *gone*), an attach client that
//! only attaches, and the resolver that needs the record's owner subject.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_cli_sessions::cli_session_manager::ReattachSkip;
use tddy_cli_sessions::tmux_location::TmuxLocation;
use tddy_core::SessionTarget;
use tddy_remote_shell::{ExecOutput, InMemoryShell, RemoteShell};
use tddy_rpc::Status;
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

use super::super::target_tmux_tab::target_tmux_session_lives;
use super::{tmux_location_of, TargetTmux};
use crate::tab_target_shell::{TabTarget, TabTargetShell};

const WORKTREE: &str = "/home/dev/repo/.worktrees/s1";

/// Shells answering every session's target with one fixed shell, or refusing with `status`.
struct FixedShells {
    shell: Option<Arc<dyn RemoteShell>>,
    refuses: Option<String>,
}

impl FixedShells {
    fn reaching(shell: Arc<dyn RemoteShell>) -> Arc<Self> {
        Arc::new(Self {
            shell: Some(shell),
            refuses: None,
        })
    }

    fn refusing(reason: &str) -> Arc<Self> {
        Arc::new(Self {
            shell: None,
            refuses: Some(reason.to_string()),
        })
    }
}

impl TabTargetShell for FixedShells {
    fn target_of(&self, _session_dir: &Path, _subject: &str) -> Result<TabTarget, Status> {
        match (&self.shell, &self.refuses) {
            (Some(shell), _) => Ok(TabTarget {
                shell: Arc::clone(shell),
                worktree: PathBuf::from(WORKTREE),
            }),
            (None, Some(reason)) => Err(Status::failed_precondition(reason.clone())),
            (None, None) => Err(Status::failed_precondition("no target scripted")),
        }
    }
}

fn a_location_reaching(shell: InMemoryShell) -> TargetTmux {
    TargetTmux::new(
        PathBuf::from("/sessions/s1"),
        "ada".to_string(),
        FixedShells::reaching(Arc::new(shell)),
    )
}

/// A target answering every probe with `output`.
fn a_target_answering(output: ExecOutput) -> InMemoryShell {
    let target = InMemoryShell::new("buildbox");
    target.answers_when(move |_| Some(output.clone()));
    target
}

#[tokio::test]
async fn the_target_probe_reads_exit_255_as_unreachable_not_gone() {
    // Given a transport that failed after connecting (ssh's own failure status)
    let target = a_target_answering(ExecOutput::exited(255, "ssh: connect failed\n"));

    // When the tmux session's life is asked
    let lives = target_tmux_session_lives(&target, "tddy-s1").await;

    // Then it could not be told — never read as gone
    assert!(lives.is_err(), "exit 255 is unreachable, not gone: {lives:?}");
}

#[tokio::test]
async fn the_target_probe_reads_cant_find_session_as_gone() {
    // Given a target whose tmux answers that the session is not there
    let target = a_target_answering(ExecOutput::exited(1, "can't find session: tddy-s1\n"));

    // When
    let lives = target_tmux_session_lives(&target, "tddy-s1").await;

    // Then it is gone
    assert_eq!(lives, Ok(false));
}

#[tokio::test]
async fn the_target_probe_reads_a_missing_tmux_as_unreachable() {
    // Given a target whose login shell finds no tmux (127)
    let target = a_target_answering(ExecOutput::exited(127, "tmux: command not found\n"));

    // When
    let lives = target_tmux_session_lives(&target, "tddy-s1").await;

    // Then it could not be told
    assert!(lives.is_err(), "exit 127 is unreachable, not gone: {lives:?}");
}

#[tokio::test]
async fn the_target_location_reads_an_unreachable_probe_as_unreachable() {
    // Given a location whose probe the transport fails (255)
    let location = a_location_reaching(a_target_answering(ExecOutput::exited(
        255,
        "ssh: connect failed\n",
    )));

    // When
    let lives = location.session_lives("tddy-s1").await;

    // Then the port answers Err, never false
    assert!(lives.is_err(), "255 through the port is Err: {lives:?}");
}

#[tokio::test]
async fn the_target_attach_client_attaches_and_never_creates() {
    // Given a reachable target
    let location = a_location_reaching(InMemoryShell::new("buildbox"));

    // When a client of the session's tmux is asked for
    let client = location
        .attach_client("tddy-s1")
        .await
        .expect("a client is built");

    // Then the remote command attaches to exactly that session and never creates one,
    // and the local client runs as the daemon (no host OS user)
    let command = client.argv.join(" ");
    assert!(
        command.contains("attach-session") && command.contains("tddy-s1"),
        "the client attaches: {command}"
    );
    assert!(
        !command.contains("new-session"),
        "a re-attach never creates: {command}"
    );
    assert_eq!(client.run_as, None);
}

/// A session directory whose record names `target` and owner subject `owner`.
fn a_session_dir(target: Option<SessionTarget>, owner: Option<&str>) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a session directory");
    let session_dir = dir.path().join("s1");
    std::fs::create_dir_all(&session_dir).expect("the session directory");
    let mut metadata = a_session_metadata()
        .with_session_id("s1")
        .with_repo_path(WORKTREE)
        .build();
    metadata.target = target;
    metadata.owner_subject = owner.map(str::to_string);
    write_session_yaml(&session_dir, &metadata);
    (dir, session_dir)
}

#[test]
fn a_session_without_an_owner_subject_resolves_no_target_location() {
    // Given a target session recorded before owner subjects were kept
    let (_dir, session_dir) = a_session_dir(Some(SessionTarget::ssh("buildbox")), None);
    let shells: Arc<dyn TabTargetShell> =
        FixedShells::reaching(Arc::new(InMemoryShell::new("buildbox")));

    // When its location is resolved
    let location = tmux_location_of(&session_dir, "dev", &shells);

    // Then it is left for Resume, with no guess at whose vault reaches it
    assert_eq!(
        location.map(|_| ()),
        Err(ReattachSkip::NoOwnerSubject)
    );
}

#[test]
fn a_locked_vault_resolves_as_unreachable_naming_the_vault() {
    // Given a target session whose owner's vault is locked
    let (_dir, session_dir) = a_session_dir(Some(SessionTarget::ssh("buildbox")), Some("ada"));
    let shells: Arc<dyn TabTargetShell> =
        FixedShells::refusing("the Coder vault of ada is locked; unlock it");

    // When
    let location = tmux_location_of(&session_dir, "dev", &shells);

    // Then the skip names the vault's refusal
    assert_eq!(
        location.map(|_| ()),
        Err(ReattachSkip::TargetUnreachable(
            "the Coder vault of ada is locked; unlock it".to_string()
        ))
    );
}

#[test]
fn a_host_session_resolves_to_the_host_location() {
    // Given a session that runs on this host
    let (_dir, session_dir) = a_session_dir(None, None);
    let shells: Arc<dyn TabTargetShell> =
        FixedShells::reaching(Arc::new(InMemoryShell::new("buildbox")));

    // When
    let location = tmux_location_of(&session_dir, "dev", &shells).expect("a location resolves");

    // Then it is the host's tmux, as its OS user
    assert_eq!(location.describe(), "the tmux of dev");
}

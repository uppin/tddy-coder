//! Unit tests: the daemon's [`SessionTargetShells`] resolves a session's recorded target for its
//! tabs — the transport and the worktree from `.session.yaml`, a Coder account from the vault of the
//! caller's subject — and refuses what it cannot reach.

// Test-only: included via `mod` under `#[cfg(test)]`, so the filesystem-port guard reads
// the whole file as test code and the real-disk fixtures below are the tests' fixtures, not
// production I/O.
#![cfg(test)]

use std::path::PathBuf;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_core::SessionTarget;
use tddy_credentials::SecretString;
use tddy_host_service::coder_derived_hosts::CoderAccountRef;
use tddy_host_service::coder_workspaces::{CoderCredentialSource, CoderCredentials};
use tddy_host_service::test_util::{a_coder_vault_of, ACoderVault};
use tddy_remote_shell::ShellAddress;
use tddy_rpc::Code;
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

use super::{SessionTargetShells, TabTargetShell};

const SESSION_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e70";
/// Who the caller's session token resolved to: the owner of the vault a Coder account is read from.
const OWNER: &str = "ada-on-github";
/// Somebody else, whose vault holds no Coder account.
const SOMEBODY_ELSE: &str = "grace-on-github";
const ADAS_ACCOUNT: &str = "coder-3f1c9a52@coder.example.com";
/// The session's worktree as its record names it: a path on the target.
const TARGET_WORKTREE: &str = "/home/dev/repo/.worktrees/019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e70";

/// A daemon's directory of one session.
struct ADaemon {
    session_dir: PathBuf,
    _dirs: Vec<tempfile::TempDir>,
}

fn a_daemon_whose_session_records(
    target: Option<SessionTarget>,
    repo_path: Option<&str>,
) -> ADaemon {
    let sessions_dir = tempfile::tempdir().expect("a sessions directory");
    let session_dir = sessions_dir.path().join(SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("the session directory");
    let builder = a_session_metadata()
        .with_session_id(SESSION_ID)
        .with_session_type("claude-cli")
        .with_status("active")
        .without_pid();
    let mut metadata = match repo_path {
        Some(path) => builder.with_repo_path(path).build(),
        None => builder.build(),
    };
    metadata.target = target;
    write_session_yaml(&session_dir, &metadata);
    ADaemon {
        session_dir,
        _dirs: vec![sessions_dir],
    }
}

impl ADaemon {
    /// The daemon's target shells, reading Coder accounts from `vault`.
    fn target_shells_reading(&self, vault: ACoderVault) -> SessionTargetShells {
        SessionTargetShells::new(Arc::new(vault) as Arc<dyn CoderCredentialSource>)
    }
}

fn the_owners_open_vault() -> ACoderVault {
    a_coder_vault_of(OWNER).holding(
        CoderAccountRef::new(ADAS_ACCOUNT, "work"),
        CoderCredentials::new(
            "https://coder.example.com",
            SecretString::new("adas-coder-session-token"),
        ),
    )
}

fn adas_devbox() -> SessionTarget {
    SessionTarget::coder(ADAS_ACCOUNT, "ada", "devbox").on_agent("main")
}

#[test]
fn an_ssh_target_is_reached_on_its_alias_in_the_recorded_worktree() {
    // Given a session recorded on the ssh target `buildbox`
    let daemon =
        a_daemon_whose_session_records(Some(SessionTarget::ssh("buildbox")), Some(TARGET_WORKTREE));
    let shells = daemon.target_shells_reading(the_owners_open_vault());

    // When
    let target = shells
        .target_of(&daemon.session_dir, OWNER)
        .expect("the session's target resolves");

    // Then
    assert_eq!(
        (target.shell.address(), target.worktree),
        (
            ShellAddress::new("ssh", "buildbox"),
            PathBuf::from(TARGET_WORKTREE)
        )
    );
}

#[test]
fn a_coder_target_is_reached_with_the_account_its_owners_vault_holds() {
    // Given a session recorded on Ada's Coder workspace, and her open vault holding the account
    let daemon = a_daemon_whose_session_records(Some(adas_devbox()), Some(TARGET_WORKTREE));
    let shells = daemon.target_shells_reading(the_owners_open_vault());

    // When the target is resolved as Ada
    let target = shells
        .target_of(&daemon.session_dir, OWNER)
        .expect("the session's target resolves");

    // Then it is the workspace the target names
    assert_eq!(
        (target.shell.address(), target.worktree),
        (
            ShellAddress::new("coder", "ada/devbox.main"),
            PathBuf::from(TARGET_WORKTREE)
        )
    );
}

#[test]
fn a_coder_target_whose_owners_vault_is_locked_is_refused_saying_so() {
    // Given a session recorded on Ada's Coder workspace, and her vault locked
    let daemon = a_daemon_whose_session_records(Some(adas_devbox()), Some(TARGET_WORKTREE));
    let shells = daemon.target_shells_reading(the_owners_open_vault().locked());

    // When
    let refusal = shells
        .target_of(&daemon.session_dir, OWNER)
        .err()
        .expect("a locked vault refuses the target");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains("locked"),
        "the refusal says the vault is locked: {}",
        refusal.message()
    );
}

#[test]
fn a_session_recording_no_target_is_refused_naming_the_session() {
    // Given a session that runs on this host
    let daemon = a_daemon_whose_session_records(None, Some("/home/dev/repo"));
    let shells = daemon.target_shells_reading(the_owners_open_vault());

    // When
    let refusal = shells
        .target_of(&daemon.session_dir, OWNER)
        .err()
        .expect("a session with no target has none to resolve");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains(SESSION_ID),
        "the refusal names the session: {}",
        refusal.message()
    );
}

#[test]
fn a_target_session_recording_no_worktree_is_refused_naming_the_session() {
    // Given a session recorded on an ssh target, but with no worktree there
    let daemon = a_daemon_whose_session_records(Some(SessionTarget::ssh("buildbox")), None);
    let shells = daemon.target_shells_reading(the_owners_open_vault());

    // When
    let refusal = shells
        .target_of(&daemon.session_dir, OWNER)
        .err()
        .expect("a target session with no worktree has nowhere to open a tab");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains(SESSION_ID),
        "the refusal names the session: {}",
        refusal.message()
    );
}

#[test]
fn a_coder_target_is_read_from_the_vault_of_the_caller_not_of_anybody_else() {
    // Given a session recorded on Ada's Coder workspace, and her open vault holding the account
    let daemon = a_daemon_whose_session_records(Some(adas_devbox()), Some(TARGET_WORKTREE));
    let shells = daemon.target_shells_reading(the_owners_open_vault());

    // When the target is resolved as somebody else
    let refusal = shells
        .target_of(&daemon.session_dir, SOMEBODY_ELSE)
        .err()
        .expect("somebody else's vault does not hold Ada's account");

    // Then it is refused naming the account their vault does not hold
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(
        refusal.message().contains(ADAS_ACCOUNT),
        "the refusal names the account: {}",
        refusal.message()
    );
}

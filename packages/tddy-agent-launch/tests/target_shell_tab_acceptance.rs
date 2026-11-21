//! Acceptance: a shell tab of a target session is launched **on the target** — in the session's
//! worktree there, through the session's remote shell — as a local tab terminal that is not the
//! session's main: it writes no pid file, leaves the session pointer as it was, and a target it
//! cannot reach fails the tab alone (`#session-liveness` 6/8).
//!
//! The ssh target is the shared stand-in machine (`tddy_testing_commons::remote_target`): a stand-in
//! `ssh` that runs the remote command on this machine under a login shell of the target's own. The
//! Coder target is the stand-in workspace (`coder_workspace_support`) reached by the real
//! `CoderShell` with an account a stand-in vault holds. Each target's login shell, run as `-l` — as
//! a shell tab runs it — records the directory it started in and stays up on its terminal.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-target-shell-tab.md

mod coder_workspace_support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_agent_launch::coder_target::shell_for_coder_target_through;
use tddy_agent_launch::remote_agent_launch::{
    launch_tab_on_target, target_shell_tab_command, RemoteLaunchError, TargetTab,
};
use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_core::SessionTarget;
use tddy_host_service::coder_derived_hosts::CoderAccountRef;
use tddy_host_service::coder_workspaces::CoderCredentials;
use tddy_host_service::test_util::a_coder_vault_of;
use tddy_remote_shell::{RemoteShell, SshShell};
use tddy_testing_commons::remote_target::{write_executable, StandInSshTarget};
use tddy_testing_commons::wait::eventually;

use coder_workspace_support::{a_running_workspace, AWorkspace};

const SESSION_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e60";
/// The id the tab is opened under.
const TAB_ID: &str = "019d2c7e-51aa-7c40-8f0e-2b1d6a9c3e61";
/// The ssh `Host` alias the session's target is known by.
const TARGET_ALIAS: &str = "buildbox";
/// What OpenSSH says when it cannot reach its target.
const SSH_COMPLAINT: &str = "ssh: Could not resolve hostname buildbox: Name or service not known";

/// Who the session belongs to: the owner of the vault the Coder account is read from.
const OWNER: &str = "ada-on-github";
const ADAS_ACCOUNT: &str = "coder-3f1c9a52@coder.example.com";
const ADAS_DEPLOYMENT: &str = "https://coder.example.com";
const ADAS_TOKEN: &str = "adas-coder-session-token";
/// The workspace the target names, as `coder ssh` is given it.
const ADAS_DEVBOX: &str = "ada/devbox.main";

/// A safety net for a stand-in to report, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(15);

/// The stand-in ssh machine sets the process's `PATH`: its tests take turns.
static ONE_SSH_MACHINE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A daemon holding one session's terminals, and the session's directory on this host.
struct ADaemon {
    manager: Arc<CliSessionManager>,
    session_dir: PathBuf,
    _session_dir: tempfile::TempDir,
}

fn a_daemon() -> ADaemon {
    let dir = tempfile::tempdir().expect("a session directory");
    ADaemon {
        manager: Arc::new(CliSessionManager::new()),
        session_dir: std::fs::canonicalize(dir.path()).expect("the session directory resolves"),
        _session_dir: dir,
    }
}

impl ADaemon {
    /// The session's main, running on this host: what a tab must leave alone.
    async fn runs_the_sessions_main(&self) -> Arc<PtyHandle> {
        self.manager
            .spawn_rendered_main(
                SESSION_ID,
                "acme-cli",
                self.session_dir.clone(),
                "",
                vec!["/bin/sleep".to_string(), "300".to_string()],
                Vec::new(),
                None,
                None,
            )
            .await
            .expect("the session's main starts")
    }

    /// Open the session's shell tab [`TAB_ID`] on the target `shell` reaches, in `worktree` there.
    async fn opens_a_tab_on(
        &self,
        shell: Arc<dyn RemoteShell>,
        worktree: &Path,
    ) -> Result<Arc<PtyHandle>, RemoteLaunchError> {
        launch_tab_on_target(
            &self.manager,
            SESSION_ID,
            TAB_ID,
            TargetTab {
                shell,
                worktree,
                session_dir: &self.session_dir,
            },
        )
        .await
    }

    /// Whether the session's main still runs, as the one this daemon started.
    async fn main_still_runs(&self, main: &PtyHandle) -> bool {
        match self.manager.get(SESSION_ID).await {
            Some(handle) => {
                handle.pid == main.pid
                    && matches!(handle.task_status(), tddy_task::TaskStatus::Running)
            }
            None => false,
        }
    }
}

/// The session's worktree on the stand-in ssh machine.
fn worktree_on(machine: &StandInSshTarget) -> PathBuf {
    let worktree = machine.worktree_of(SESSION_ID);
    std::fs::create_dir_all(&worktree).expect("the session's worktree on the target");
    worktree
}

/// The remote shell that reaches the stand-in ssh machine.
fn the_shell_reaching(machine: &StandInSshTarget) -> Arc<dyn RemoteShell> {
    Arc::new(SshShell::through(
        machine.root.join("bin/ssh"),
        TARGET_ALIAS,
    ))
}

/// The directory a target's login shell recorded it started in, once it has.
async fn where_the_login_shell_started(record: PathBuf) -> PathBuf {
    eventually("the target's login shell to start", OBSERVE, || {
        std::fs::read_to_string(&record)
            .map(|cwd| PathBuf::from(cwd.trim_end()))
            .map_err(|_| "the target's login shell has not started yet".to_string())
    })
    .await
}

/// A target ssh cannot reach: its `ssh` fails as OpenSSH does — [`SSH_COMPLAINT`] on stderr,
/// status 255 — and runs nothing.
struct AnUnreachableTarget {
    ssh: PathBuf,
    worktree: PathBuf,
    _dir: tempfile::TempDir,
}

fn an_unreachable_target() -> AnUnreachableTarget {
    let dir = tempfile::tempdir().expect("a directory for the stand-in ssh");
    let ssh = dir.path().join("ssh");
    write_executable(
        &ssh,
        &format!("#!/bin/sh\necho '{SSH_COMPLAINT}' >&2\nexit 255\n"),
    );
    AnUnreachableTarget {
        ssh,
        worktree: PathBuf::from("/home/dev/repo/.worktrees").join(SESSION_ID),
        _dir: dir,
    }
}

/// The vault of the session's owner, open and holding Ada's Coder account.
fn the_owners_open_vault() -> tddy_host_service::test_util::ACoderVault {
    a_coder_vault_of(OWNER).holding(
        CoderAccountRef::new(ADAS_ACCOUNT.to_string(), "work".to_string()),
        CoderCredentials::new(
            ADAS_DEPLOYMENT,
            tddy_credentials::SecretString::new(ADAS_TOKEN),
        ),
    )
}

/// Ada's workspace `devbox` of her account, on its agent `main`.
fn adas_devbox() -> SessionTarget {
    SessionTarget::coder(ADAS_ACCOUNT, "ada", "devbox").on_agent("main")
}

/// The shell that reaches `workspace` as the target names it, read with the owner's `vault`.
fn the_shell_reaching_the_workspace(
    workspace: &AWorkspace,
    vault: &tddy_host_service::test_util::ACoderVault,
) -> Arc<dyn RemoteShell> {
    shell_for_coder_target_through(
        workspace.coder(),
        workspace.ssh(),
        &adas_devbox(),
        OWNER,
        vault,
    )
    .expect("the owner's vault opens the target")
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_shell_tab_opened_on_target_runs_in_the_target_worktree() {
    // Given a session whose target is an ssh machine holding its worktree
    let _turn = ONE_SSH_MACHINE_AT_A_TIME.lock().await;
    let machine = StandInSshTarget::behind_ssh();
    let worktree = worktree_on(&machine);
    let daemon = a_daemon();

    // When a shell tab is opened on the target
    let tab = daemon
        .opens_a_tab_on(the_shell_reaching(&machine), &worktree)
        .await
        .expect("the tab starts on the target");

    // Then the target's login shell runs in the session's worktree there, and the tab is a shell
    // tab of the session under the id it was opened with — not its main
    assert_eq!(
        where_the_login_shell_started(machine.login_shell_started_in()).await,
        worktree
    );
    assert_eq!(
        (tab.terminal_id.as_str(), tab.kind.as_str()),
        (TAB_ID, "bash")
    );
    assert!(
        daemon
            .manager
            .get_terminal(SESSION_ID, TAB_ID)
            .await
            .is_some(),
        "the tab is served under its id"
    );
    assert!(
        daemon.manager.get(SESSION_ID).await.is_none(),
        "a tab is never the session's main"
    );
}

#[tokio::test]
async fn a_target_tab_writes_no_pid_file_and_leaves_the_session_pointer() {
    // Given a target session whose main runs, and whose worktree on the target holds the pointer
    // its start placed
    let _turn = ONE_SSH_MACHINE_AT_A_TIME.lock().await;
    let machine = StandInSshTarget::behind_ssh();
    let worktree = worktree_on(&machine);
    let pointer = machine.pointer_of(SESSION_ID);
    std::fs::create_dir_all(pointer.parent().expect("the pointer has a directory"))
        .expect("the pointer's directory on the target");
    let placed = br#"{"repo_dir":"/home/dev/repo","owner":{"pid":4242,"host":"buildbox"}}"#;
    std::fs::write(&pointer, placed).expect("the pointer is placed on the target");
    let daemon = a_daemon();
    let main = daemon.runs_the_sessions_main().await;

    // When a shell tab is opened on the target, and its shell has started
    daemon
        .opens_a_tab_on(the_shell_reaching(&machine), &worktree)
        .await
        .expect("the tab starts on the target");
    where_the_login_shell_started(machine.login_shell_started_in()).await;

    // Then no agent pid was written on the target, the pointer is byte for byte as placed, and
    // the session's main is the one that was running
    assert!(
        !worktree.join(".tddy/agents").exists(),
        "a tab records no agent pid on the target"
    );
    assert_eq!(
        std::fs::read(&pointer).expect("the pointer is on the target"),
        placed.to_vec()
    );
    assert!(daemon.main_still_runs(&main).await);
}

#[tokio::test]
async fn a_target_tab_that_cannot_connect_ends_only_itself() {
    // Given a target session whose main runs, and a target ssh cannot reach
    let target = an_unreachable_target();
    let daemon = a_daemon();
    let main = daemon.runs_the_sessions_main().await;

    // When a shell tab is opened on the target
    let shell: Arc<dyn RemoteShell> = Arc::new(SshShell::through(&target.ssh, TARGET_ALIAS));
    let opened = daemon.opens_a_tab_on(shell, &target.worktree).await;

    // Then the tab fails with what ssh said, no tab is left behind, and the main still runs
    let failure = opened
        .err()
        .expect("a tab on an unreachable target fails")
        .to_string();
    assert!(failure.contains(SSH_COMPLAINT), "{failure}");
    assert!(
        daemon
            .manager
            .get_terminal(SESSION_ID, TAB_ID)
            .await
            .is_none(),
        "no tab is registered"
    );
    assert!(daemon.main_still_runs(&main).await);
}

#[tokio::test]
async fn a_coder_target_tab_runs_through_the_coder_client() {
    // Given a session whose target is a running Coder workspace holding its worktree, and its
    // owner's open vault holding the account the target names
    let workspace = a_running_workspace(SESSION_ID);
    let vault = the_owners_open_vault();
    let daemon = a_daemon();

    // When a shell tab is opened on the target
    daemon
        .opens_a_tab_on(
            the_shell_reaching_the_workspace(&workspace, &vault),
            &workspace.worktree(),
        )
        .await
        .expect("the tab starts in the workspace");

    // Then the workspace's login shell runs in the session's worktree there ...
    assert_eq!(
        where_the_login_shell_started(workspace.login_shell_started_in()).await,
        workspace.worktree()
    );
    // ... reached by one interactive `coder ssh` to the target's workspace, run in the session's
    // directory on this host, as the account the owner's vault holds
    let calls = workspace.calls_carrying(&target_shell_tab_command(&workspace.worktree()));
    let interactive: Vec<_> = calls
        .iter()
        .map(|call| {
            (
                call.cwd.clone(),
                call.args.contains(ADAS_DEVBOX),
                call.deployment.as_str(),
                call.token.as_str(),
            )
        })
        .collect();
    assert_eq!(
        interactive,
        vec![(
            daemon.session_dir.clone(),
            true,
            ADAS_DEPLOYMENT,
            ADAS_TOKEN
        )]
    );
}

//! The CLI session pointer: what it carries, where it is written, and when it goes.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tddy_core::session_pointer::{find_session_pointer, SessionPointer, SessionPointerOwner};

use super::{
    place_session_pointer, record_remote_owner, removing_session_pointer_on_exit,
    session_pointer_of, PlacedSessionPointer, PointerHost, REMOTE_AGENT_PID_FILE,
};

const SOCKET: &str = "/run/tddy/managed.sock";
const SESSION_DIR: &str = "/var/tddy/sessions/s1";

fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

fn the_managed_tool_server_env(worktree: &Path) -> Vec<(String, String)> {
    pairs(&[
        ("TDDY_SOCKET", SOCKET),
        ("TDDY_SESSION_DIR", SESSION_DIR),
        ("TDDY_REPO_DIR", &worktree.to_string_lossy()),
    ])
}

fn the_notify_env() -> Vec<(String, String)> {
    pairs(&[
        ("TDDY_SESSION_ID", "s1"),
        ("TDDY_HOOK_TOKEN", "the-hook-token"),
    ])
}

fn a_checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(dir.path())
        .status()
        .unwrap();
    assert!(status.success());
    dir
}

fn the_pointer_in(worktree: &Path) -> SessionPointer {
    session_pointer_of(
        &the_managed_tool_server_env(worktree),
        the_notify_env(),
        worktree,
    )
    .expect("a managed, notifying session has a pointer")
}

#[test]
fn the_pointer_carries_the_managed_socket_and_the_notify_session() {
    // Given
    let worktree = PathBuf::from("/srv/worktrees/s1");

    // When
    let pointer = the_pointer_in(&worktree);

    // Then
    assert_eq!(
        pointer,
        SessionPointer {
            socket: Some(PathBuf::from(SOCKET)),
            repo_dir: worktree,
            session_dir: Some(PathBuf::from(SESSION_DIR)),
            remote: BTreeMap::new(),
            notify: the_notify_env().into_iter().collect(),
            owner: Some(SessionPointerOwner::this_process()),
        }
    );
}

#[test]
fn a_notify_only_session_points_at_no_socket_and_a_session_with_neither_has_no_pointer() {
    // Given
    let worktree = Path::new("/srv/worktrees/s1");

    // When
    let notify_only = session_pointer_of(&[], the_notify_env(), worktree);
    let neither = session_pointer_of(
        &pairs(&[("TDDY_TOOLS_LOG_FILE", "/x")]),
        Vec::new(),
        worktree,
    );

    // Then
    assert_eq!(
        (
            notify_only.map(|pointer| (pointer.socket, pointer.repo_dir)),
            neither
        ),
        (Some((None, worktree.to_path_buf())), None)
    );
}

#[test]
fn a_placed_pointer_is_found_from_the_worktree_owner_only_and_kept_out_of_git() {
    // Given
    let checkout = a_checkout();
    let pointer = the_pointer_in(checkout.path());

    // When
    let placed = place_session_pointer(Some(&pointer), checkout.path(), &PointerHost::Local)
        .expect("the pointer is written")
        .expect("a local pointer is the launch's to remove")
        .path;

    // Then
    let found = find_session_pointer(&checkout.path().join("src/deep")).unwrap();
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(checkout.path())
        .args(["status", "--porcelain", "--untracked-files=all"])
        .output()
        .unwrap();
    assert_eq!(
        (
            found,
            std::fs::metadata(&placed).unwrap().permissions().mode() & 0o777,
            String::from_utf8_lossy(&status.stdout).into_owned(),
        ),
        (Some((placed.clone(), pointer)), 0o600, String::new())
    );
}

#[test]
fn a_worktree_that_is_no_git_checkout_gets_its_pointer_with_nothing_to_exclude_it_from() {
    // Given — a plain directory: no repository holds it, so none can commit the pointer
    let not_a_checkout = tempfile::tempdir().unwrap();
    let pointer = the_pointer_in(not_a_checkout.path());

    // When
    let placed = place_session_pointer(Some(&pointer), not_a_checkout.path(), &PointerHost::Local)
        .expect("the pointer is written")
        .map(|placed| placed.path);

    // Then
    assert_eq!(
        find_session_pointer(not_a_checkout.path()).unwrap(),
        Some((placed.expect("a local pointer"), pointer))
    );
}

#[tokio::test]
async fn the_agents_exit_removes_the_pointer_then_runs_the_launchs_own_hook() {
    // Given
    let checkout = a_checkout();
    let placed = place_session_pointer(
        Some(&the_pointer_in(checkout.path())),
        checkout.path(),
        &PointerHost::Local,
    )
    .unwrap();
    let ran = Arc::new(Mutex::new(false));
    let ran_by_hook = Arc::clone(&ran);
    let own: tddy_cli_sessions::cli_session_manager::ExitHook = Box::new(move |_| {
        Box::pin(async move {
            *ran_by_hook.lock().unwrap() = true;
        })
    });

    // When
    let hook =
        removing_session_pointer_on_exit(Some(own), placed.clone(), false).expect("an exit hook");
    hook(Arc::new(Mutex::new(tddy_task::TerminalCapture::new()))).await;

    // Then
    assert_eq!(
        (placed.unwrap().path.exists(), *ran.lock().unwrap()),
        (false, true)
    );
}

#[test]
fn a_tmux_agent_without_an_exit_hook_is_given_none_for_the_pointer() {
    // Given — an agent in tmux whose launch has no exit hook: a hook would keep its pane
    let pointer = Some(PlacedSessionPointer {
        path: PathBuf::from("/srv/worktrees/s1/.tddy/session.json"),
        host: PointerHost::Local,
    });

    // When
    let in_tmux = removing_session_pointer_on_exit(None, pointer.clone(), true);
    let as_pty_child = removing_session_pointer_on_exit(None, pointer, false);

    // Then
    assert_eq!((in_tmux.is_none(), as_pty_child.is_some()), (true, true));
}

// --- `#agent-tools 21/28`: an ssh session's pointer is git-excluded, owner-only and removed ---

const SSH_TARGET: &str = "buildbox";

/// An `ssh` stand-in that appends each invocation's arguments, one line per invocation, to
/// `invocations`, then runs the remote command here: the "target" is this host's filesystem.
struct AStubSsh {
    dir: tempfile::TempDir,
}

fn a_stub_ssh() -> AStubSsh {
    let dir = tempfile::tempdir().unwrap();
    let ssh = dir.path().join("ssh");
    std::fs::write(
        &ssh,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\nfor last; do :; done\nexec sh -c \"$last\"\n",
            dir.path().join("invocations").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
    AStubSsh { dir }
}

impl AStubSsh {
    fn host(&self) -> PointerHost {
        PointerHost::Ssh {
            ssh: self.dir.path().join("ssh"),
            alias: SSH_TARGET.to_string(),
        }
    }

    /// Each invocation's arguments.
    fn invocations(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("invocations"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// Whether every invocation ran with a connection that cannot hang, against the target.
    fn every_invocation_is_bounded(&self) -> bool {
        self.invocations().iter().all(|invocation| {
            [
                "BatchMode=yes",
                "ConnectTimeout=15",
                "ServerAliveInterval=5",
                "ServerAliveCountMax=3",
                "-- buildbox",
            ]
            .iter()
            .all(|option| invocation.contains(option))
        })
    }
}

/// `git status --porcelain` of `checkout`, untracked files included.
fn untracked_in(checkout: &Path) -> String {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(["status", "--porcelain", "--untracked-files=all"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&status.stdout).into_owned()
}

/// A session directory whose `.session.yaml` records `worktree` on the ssh target.
fn a_session_recording_an_ssh_worktree(worktree: &Path) -> tempfile::TempDir {
    let sessions = tempfile::tempdir().unwrap();
    let session_dir = sessions.path().join("s1");
    std::fs::create_dir_all(&session_dir).unwrap();
    tddy_core::session_metadata::write_initial_tool_session_metadata(
        &session_dir,
        tddy_core::session_metadata::InitialToolSessionMetadataOpts {
            project_id: "p1".to_string(),
            repo_path: Some(worktree.to_string_lossy().into_owned()),
            ..Default::default()
        },
    )
    .unwrap();
    let mut metadata = tddy_core::read_session_metadata(&session_dir).unwrap();
    metadata.ssh_config_host = Some(SSH_TARGET.to_string());
    tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();
    sessions
}

/// The name this host gives itself, as `hostname` prints it: the stub ssh's "target" is this host.
fn the_targets_hostname() -> String {
    let output = std::process::Command::new("hostname").output().unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The pid the remote launch records for its agent on the target.
const REMOTE_AGENT_PID: u32 = 4242;

#[test]
fn an_ssh_pointer_is_excluded_owner_only_and_bounded() {
    // Given — an ssh session whose worktree is a checkout on the target
    let target_checkout = a_checkout();
    let ssh = a_stub_ssh();
    let pointer = the_pointer_in(target_checkout.path());

    // When — the pointer is placed, the agent starts and records its pid, and its owner is recorded
    let placed = place_session_pointer(Some(&pointer), target_checkout.path(), &ssh.host())
        .expect("the pointer is written on the target")
        .expect("an ssh pointer is the launch's to remove");
    std::fs::write(
        target_checkout.path().join(REMOTE_AGENT_PID_FILE),
        REMOTE_AGENT_PID.to_string(),
    )
    .unwrap();
    let owner = record_remote_owner(&placed, &pointer).expect("the remote agent owns the pointer");

    // Then — owned by the remote agent on the target, owner-only, kept out of git, and each step
    // one bounded ssh command
    let remote_owner = SessionPointerOwner {
        pid: REMOTE_AGENT_PID,
        host: the_targets_hostname(),
    };
    let on_the_target = target_checkout.path().join(".tddy/session.json");
    assert_eq!(
        (
            owner,
            find_session_pointer(target_checkout.path())
                .unwrap()
                .map(|(_, found)| found),
            std::fs::metadata(&on_the_target)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            untracked_in(target_checkout.path()),
            ssh.invocations().len(),
            ssh.every_invocation_is_bounded(),
        ),
        (
            remote_owner.clone(),
            Some(SessionPointer {
                owner: Some(remote_owner),
                ..pointer
            }),
            0o600,
            String::new(),
            2,
            true
        )
    );
}

#[test]
fn the_owner_of_a_pointer_on_this_host_is_not_looked_up_over_ssh() {
    // Given — a pointer placed on this host
    let checkout = a_checkout();
    let pointer = the_pointer_in(checkout.path());
    let placed = place_session_pointer(Some(&pointer), checkout.path(), &PointerHost::Local)
        .unwrap()
        .expect("a local pointer");

    // When
    let refused = record_remote_owner(&placed, &pointer).map_err(|status| status.code());

    // Then
    assert_eq!(refused, Err(tddy_rpc::Code::FailedPrecondition));
}

#[tokio::test]
async fn an_ssh_pointer_is_removed_when_the_agent_exits_and_when_the_session_is_deleted() {
    // Given — an ssh session's pointer, placed on the target
    let target_checkout = a_checkout();
    let ssh = a_stub_ssh();
    let on_the_target = target_checkout.path().join(".tddy/session.json");
    let pointer = the_pointer_in(target_checkout.path());
    let placed = place_session_pointer(Some(&pointer), target_checkout.path(), &ssh.host())
        .expect("the pointer is written on the target");

    // When — its agent exits
    let on_exit = removing_session_pointer_on_exit(None, placed, false)
        .expect("an ssh pointer is removed when its agent exits");
    on_exit(Arc::new(Mutex::new(tddy_task::TerminalCapture::new()))).await;
    let after_the_exit = on_the_target.exists();

    // ...and, placed again by a resume, its session is deleted
    place_session_pointer(Some(&pointer), target_checkout.path(), &ssh.host())
        .expect("the pointer is written again");
    let sessions = a_session_recording_an_ssh_worktree(target_checkout.path());
    crate::svc_signal_delete_session::remove_recorded_session_pointer(
        &sessions.path().join("s1"),
        &ssh.dir.path().join("ssh"),
    );
    let after_the_delete = on_the_target.exists();

    // Then — gone both times, through bounded ssh commands
    assert_eq!(
        (
            after_the_exit,
            after_the_delete,
            ssh.every_invocation_is_bounded()
        ),
        (false, false, true)
    );
}

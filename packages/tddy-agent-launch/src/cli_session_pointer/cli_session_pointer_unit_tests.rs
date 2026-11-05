//! The CLI session pointer: what it carries, where it is written, and when it goes.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tddy_core::session_pointer::{find_session_pointer, SessionPointer};

use super::{place_session_pointer, removing_session_pointer_on_exit, session_pointer_of};

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
    let placed = place_session_pointer(Some(&pointer), checkout.path(), None)
        .expect("the pointer is written")
        .expect("a local pointer is the launch's to remove");

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
fn a_pointer_is_refused_in_a_worktree_that_is_not_a_git_checkout() {
    // Given
    let not_a_checkout = tempfile::tempdir().unwrap();
    let pointer = the_pointer_in(not_a_checkout.path());

    // When
    let placed = place_session_pointer(Some(&pointer), not_a_checkout.path(), None);

    // Then
    assert_eq!(
        (
            placed.map_err(|status| status.code()).err(),
            not_a_checkout.path().join(".tddy").exists()
        ),
        (Some(tddy_rpc::Code::Internal), false)
    );
}

#[tokio::test]
async fn the_agents_exit_removes_the_pointer_then_runs_the_launchs_own_hook() {
    // Given
    let checkout = a_checkout();
    let placed = place_session_pointer(
        Some(&the_pointer_in(checkout.path())),
        checkout.path(),
        None,
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
        (placed.unwrap().exists(), *ran.lock().unwrap()),
        (false, true)
    );
}

#[test]
fn a_tmux_agent_without_an_exit_hook_is_given_none_for_the_pointer() {
    // Given — an agent in tmux whose launch has no exit hook: a hook would keep its pane
    let pointer = Some(PathBuf::from("/srv/worktrees/s1/.tddy/session.json"));

    // When
    let in_tmux = removing_session_pointer_on_exit(None, pointer.clone(), true);
    let as_pty_child = removing_session_pointer_on_exit(None, pointer, false);

    // Then
    assert_eq!((in_tmux.is_none(), as_pty_child.is_some()), (true, true));
}

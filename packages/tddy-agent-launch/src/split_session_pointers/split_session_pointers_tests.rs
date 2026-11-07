//! A split agent's pointer, placed in its context dir and removed when it exits (`#agent-tools 25/28`).

use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use tddy_cli_sessions::cli_session_manager::ExitHook;
use tddy_core::session_pointer::{SessionPointer, SessionPointerOwner, SESSION_POINTER_FILE};
use tddy_session_split::split_ports::SplitSessionPointers;

use super::HostSplitSessionPointers;

/// A split agent's pointer, naming the codebase session it reaches.
fn a_split_pointer(context_dir: &std::path::Path) -> SessionPointer {
    SessionPointer {
        socket: None,
        repo_dir: context_dir.to_path_buf(),
        session_dir: None,
        remote: [(
            "TDDY_REMOTE_SESSION_ID".to_string(),
            "the-codebase-session".to_string(),
        )]
        .into(),
        notify: Default::default(),
        owner: Some(SessionPointerOwner::this_process()),
    }
}

/// An exit hook of the spawn's own, recording that it ran.
fn an_exit_hook_recording_into(ran: &Arc<Mutex<bool>>) -> ExitHook {
    let ran = Arc::clone(ran);
    Box::new(move |_| {
        Box::pin(async move {
            *ran.lock().unwrap() = true;
        })
    })
}

async fn the_agent_exits(hook: ExitHook) {
    hook(Arc::new(Mutex::new(tddy_task::TerminalCapture::new()))).await;
}

#[test]
fn a_split_agents_context_dir_holds_its_pointer_owner_only() {
    // Given a split agent's context dir, which is no checkout
    let context_dir = tempfile::tempdir().expect("a context dir");
    let pointer = a_split_pointer(context_dir.path());

    // When its pointer is placed
    HostSplitSessionPointers
        .place(&pointer, context_dir.path(), None)
        .expect("the pointer is placed");

    // Then the context dir holds it, readable by its owner alone, naming the transport
    let path = context_dir.path().join(SESSION_POINTER_FILE);
    let written: Option<(SessionPointer, u32)> = std::fs::read(&path).ok().map(|bytes| {
        (
            serde_json::from_slice(&bytes).expect("the pointer is JSON"),
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        )
    });
    assert_eq!(written, Some((pointer, 0o600)));
}

#[tokio::test]
async fn a_split_agents_pointer_is_removed_when_it_exits() {
    // Given a placed split pointer, and a spawn with an exit hook of its own
    let context_dir = tempfile::tempdir().expect("a context dir");
    let ran = Arc::new(Mutex::new(false));
    let hook = HostSplitSessionPointers
        .place(
            &a_split_pointer(context_dir.path()),
            context_dir.path(),
            Some(an_exit_hook_recording_into(&ran)),
        )
        .expect("the pointer is placed")
        .expect("an exit hook");
    let path = context_dir.path().join(SESSION_POINTER_FILE);
    assert!(
        path.exists(),
        "the pointer must be placed before the agent exits"
    );

    // When the agent exits
    the_agent_exits(hook).await;

    // Then the pointer is gone, and the spawn's own hook still ran
    assert_eq!((path.exists(), *ran.lock().unwrap()), (false, true));
}

#[test]
fn a_split_agents_pointer_is_removed_when_its_spawn_fails() {
    // Given a placed split pointer whose agent never started
    let context_dir = tempfile::tempdir().expect("a context dir");
    HostSplitSessionPointers
        .place(&a_split_pointer(context_dir.path()), context_dir.path(), None)
        .expect("the pointer is placed");
    let path = context_dir.path().join(SESSION_POINTER_FILE);

    // When the spawn's failure removes it
    HostSplitSessionPointers.remove(context_dir.path());

    // Then the context dir holds no pointer
    assert_eq!(path.exists(), false);
}

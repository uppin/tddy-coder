//! The session pointer's owner (`#agent-tools 21/28`): a backend does not overwrite a pointer that
//! another live session holds, and still replaces one its owner left behind.
//!
//! Changeset: `docs/dev/1-WIP/2026-11-06-session-pointer-ownership.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_agent_backend::session_pointer::{
    find_session_pointer, SessionPointer, SessionPointerHeld, SessionPointerOwner,
    SESSION_POINTER_FILE,
};
use tddy_agent_backend::turn_files::TurnFiles;

const FIRST_SOCKET: &str = "/run/tddy/first-session.sock";
const SECOND_SOCKET: &str = "/run/tddy/second-session.sock";

/// A git checkout two headless sessions both run their agents in.
fn a_checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("checkout");
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("git runs");
    assert!(status.success(), "git init");
    dir
}

/// The pointer of the session serving `socket` in `checkout`, owned by `owner`.
fn a_pointer(checkout: &Path, socket: &str, owner: Option<SessionPointerOwner>) -> SessionPointer {
    SessionPointer {
        socket: Some(PathBuf::from(socket)),
        repo_dir: checkout.to_path_buf(),
        session_dir: None,
        remote: BTreeMap::new(),
        notify: BTreeMap::new(),
        owner,
    }
}

/// A process on this host that has exited and been reaped.
fn an_exited_process_on_this_host() -> SessionPointerOwner {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    child.wait().expect("reap");
    SessionPointerOwner {
        pid,
        host: SessionPointerOwner::this_process().host,
    }
}

/// The turn files of one turn that wrote `pointer` in `checkout`, kept for as long as the value is.
fn a_turn_holding(checkout: &Path, pointer: &SessionPointer) -> TurnFiles {
    let mut turn = TurnFiles::default();
    turn.write_session_pointer(checkout, pointer)
        .expect("the first session's pointer is written");
    turn
}

/// The pointer in `checkout`, as a `tddy-tools` there would find it.
fn the_pointer_found_in(checkout: &Path) -> Option<SessionPointer> {
    find_session_pointer(checkout)
        .expect("the pointer is readable")
        .map(|(_, pointer)| pointer)
}

#[test]
fn a_held_pointer_for_another_session_is_not_overwritten() {
    // Given — a session whose owner (this process) is alive holds the checkout's pointer
    let checkout = a_checkout();
    let held = a_pointer(
        checkout.path(),
        FIRST_SOCKET,
        Some(SessionPointerOwner::this_process()),
    );
    let _first_turn = a_turn_holding(checkout.path(), &held);

    // When — a second session's turn starts in the same checkout
    let mut second_turn = TurnFiles::default();
    let refused = second_turn.write_session_pointer(
        checkout.path(),
        &a_pointer(
            checkout.path(),
            SECOND_SOCKET,
            Some(SessionPointerOwner::this_process()),
        ),
    );

    // Then — refused, naming the holder, and the first session's pointer is untouched
    assert_eq!(
        (
            refused.map_err(|e| e.to_string()),
            the_pointer_found_in(checkout.path())
        ),
        (
            Err(SessionPointerHeld {
                path: checkout.path().join(SESSION_POINTER_FILE),
                socket: Some(PathBuf::from(FIRST_SOCKET)),
                owner: SessionPointerOwner::this_process(),
            }
            .to_string()),
            Some(held)
        )
    );
}

#[test]
fn a_pointer_whose_owner_is_gone_is_replaced_by_the_next_session() {
    // Given — a pointer a crashed daemon left: its owner has exited
    let checkout = a_checkout();
    let stale = a_pointer(
        checkout.path(),
        FIRST_SOCKET,
        Some(an_exited_process_on_this_host()),
    );
    std::fs::create_dir_all(checkout.path().join(".tddy")).expect(".tddy");
    std::fs::write(
        checkout.path().join(SESSION_POINTER_FILE),
        serde_json::to_vec(&stale).expect("encode"),
    )
    .expect("the stale pointer");
    let next = a_pointer(
        checkout.path(),
        SECOND_SOCKET,
        Some(SessionPointerOwner::this_process()),
    );

    // When
    let mut turn = TurnFiles::default();
    let written = turn.write_session_pointer(checkout.path(), &next);

    // Then
    assert_eq!(
        (
            written.map_err(|e| e.to_string()),
            the_pointer_found_in(checkout.path())
        ),
        (Ok(()), Some(next))
    );
}

#[test]
fn the_same_sessions_pointer_is_replaced_on_its_next_turn() {
    // Given — the pointer this session's previous turn left
    let checkout = a_checkout();
    let previous = a_pointer(
        checkout.path(),
        FIRST_SOCKET,
        Some(SessionPointerOwner::this_process()),
    );
    let _previous_turn = a_turn_holding(checkout.path(), &previous);
    let mut again = previous.clone();
    again.session_dir = Some(PathBuf::from("/var/tddy/sessions/first"));

    // When
    let mut turn = TurnFiles::default();
    let written = turn.write_session_pointer(checkout.path(), &again);

    // Then
    assert_eq!(
        (
            written.map_err(|e| e.to_string()),
            the_pointer_found_in(checkout.path())
        ),
        (Ok(()), Some(again))
    );
}

#[test]
fn a_pointer_without_an_owner_is_written_and_read_without_the_field() {
    // Given — what an older daemon wrote, and an owner-less pointer this one writes
    let older = r#"{"socket":"/run/tddy/older.sock","repo_dir":"/srv/checkouts/app"}"#;
    let ownerless = a_pointer(Path::new("/srv/checkouts/app"), FIRST_SOCKET, None);

    // When
    let read: SessionPointer = serde_json::from_str(older).expect("an older pointer parses");
    let written = serde_json::to_value(&ownerless).expect("encode");

    // Then
    assert_eq!((read.owner, written.get("owner")), (None, None));
}

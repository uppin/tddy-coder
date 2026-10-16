//! Attaching to a session — AC21 of `docs/ft/daemon/session-worktree-sync.md`, and the room and
//! identity the attach is built from.
//!
//! What is under test is everything the attach decides *before* it touches a network: which room a
//! session is in, which identity the syncer joins under, and which entry of a session list — if any
//! — describes the session that was asked for. Joining a real room and calling a real daemon are
//! not tested here and are not faked here; see the crate README's Status table.

use pretty_assertions::assert_eq;
use tddy_service::proto::session::SessionEntry;
use tddy_session_sync::{
    daemon_identity, resolve_session, session_room_name, syncer_identity, AttachError,
    SessionAddress,
};

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A session as the daemon lists it: on a project, in a worktree, owned by a named daemon.
fn a_listed_session(session_id: &str) -> SessionEntry {
    SessionEntry {
        session_id: session_id.to_string(),
        project_id: "my-app".to_string(),
        repo_path: "/home/dev/.tddy/worktrees/my-app/feat-x".to_string(),
        daemon_instance_id: "udoo-1780828020298".to_string(),
        session_type: "claude-cli".to_string(),
        ..Default::default()
    }
}

/// The address `a_listed_session` resolves to.
fn the_address_of(session_id: &str) -> SessionAddress {
    SessionAddress {
        session_id: session_id.to_string(),
        project_id: "my-app".to_string(),
        worktree_path: "/home/dev/.tddy/worktrees/my-app/feat-x".to_string(),
        daemon_instance_id: "udoo-1780828020298".to_string(),
    }
}

/// The refusal a resolution produced, for a test that is about the refusal rather than the result.
fn refused(sessions: &[SessionEntry], session_id: &str) -> AttachError {
    match resolve_session(sessions, session_id) {
        Err(e) => e,
        Ok(address) => panic!("expected {session_id} to be refused, resolved {address:?}"),
    }
}

// ---------------------------------------------------------------------------
// Resolving the session
// ---------------------------------------------------------------------------

#[test]
fn resolves_the_project_worktree_and_owning_daemon_of_the_named_session() {
    // Given two sessions, one of them ours
    let sessions = [
        a_listed_session("1780828020298-zzz"),
        a_listed_session("1780828020298-abc"),
    ];

    // When
    let address = resolve_session(&sessions, "1780828020298-abc").expect("must resolve");

    // Then all three are taken from the daemon rather than from a flag (AC21).
    assert_eq!(address, the_address_of("1780828020298-abc"));
}

#[test]
fn refuses_a_session_the_daemon_does_not_know_by_naming_it() {
    // Given a daemon that knows a different session
    let sessions = [a_listed_session("1780828020298-zzz")];

    // When
    let error = refused(&sessions, "1780828020298-abc");

    // Then it is a hard error naming the session id — never the only session that was listed.
    assert_eq!(
        error,
        AttachError::SessionNotFound {
            session_id: "1780828020298-abc".to_string(),
        }
    );
}

#[test]
fn refuses_a_workspace_session_because_it_has_no_room_to_watch() {
    // Given a workspace session — no facilitating daemon, and therefore no room
    let sessions = [SessionEntry {
        session_type: "workspace".to_string(),
        ..a_listed_session("1780828020298-abc")
    }];

    // When
    let error = refused(&sessions, "1780828020298-abc");

    // Then it is refused by name rather than waited on for a room that never opens.
    assert_eq!(
        error,
        AttachError::SessionHasNoRoom {
            session_id: "1780828020298-abc".to_string(),
            session_type: "workspace".to_string(),
        }
    );
}

#[test]
fn refuses_a_session_listed_without_a_project() {
    // Given a session entry carrying no project
    let sessions = [SessionEntry {
        project_id: String::new(),
        ..a_listed_session("1780828020298-abc")
    }];

    // When
    let error = refused(&sessions, "1780828020298-abc");

    // Then it names the missing field rather than cloning from `":"`.
    assert_eq!(
        error,
        AttachError::SessionIncomplete {
            session_id: "1780828020298-abc".to_string(),
            field: "project_id",
        }
    );
}

#[test]
fn refuses_a_session_listed_without_an_owning_daemon() {
    // Given a session entry carrying no daemon instance id
    let sessions = [SessionEntry {
        daemon_instance_id: String::new(),
        ..a_listed_session("1780828020298-abc")
    }];

    // When
    let error = refused(&sessions, "1780828020298-abc");

    // Then it names the missing field rather than waiting for participant "daemon-".
    assert_eq!(
        error,
        AttachError::SessionIncomplete {
            session_id: "1780828020298-abc".to_string(),
            field: "daemon_instance_id",
        }
    );
}

#[test]
fn refuses_a_session_listed_without_a_worktree_path() {
    // Given a session entry carrying no checkout
    let sessions = [SessionEntry {
        repo_path: String::new(),
        ..a_listed_session("1780828020298-abc")
    }];

    // When
    let error = refused(&sessions, "1780828020298-abc");

    // Then
    assert_eq!(
        error,
        AttachError::SessionIncomplete {
            session_id: "1780828020298-abc".to_string(),
            field: "worktree path",
        }
    );
}

// ---------------------------------------------------------------------------
// The room and the identities in it
// ---------------------------------------------------------------------------

#[test]
fn joins_the_room_named_after_the_session() {
    // Given a session id
    // When
    let room = session_room_name("1780828020298-abc");

    // Then
    assert_eq!(room, "session-1780828020298-abc");
}

#[test]
fn addresses_the_facilitating_daemon_by_its_instance_id() {
    // Given the daemon that owns the session
    // When
    let identity = daemon_identity("udoo-1780828020298");

    // Then
    assert_eq!(identity, "daemon-udoo-1780828020298");
}

#[test]
fn joins_under_an_identity_outside_the_prefix_the_token_service_reserves_for_daemons() {
    // Given a session and this process's nonce
    // When
    let identity = syncer_identity("1780828020298-abc", "1780828020298-4242");

    // Then the identity says which program joined, and is not one every peer would address as a
    // daemon — `tddy_service::RESERVED_DAEMON_IDENTITY_PREFIX` is exactly that prefix.
    assert_eq!(
        identity,
        "session-sync-1780828020298-abc-1780828020298-4242"
    );
    assert!(
        !identity.starts_with(tddy_service::RESERVED_DAEMON_IDENTITY_PREFIX),
        "the syncer must not join under the reserved daemon prefix, got {identity}"
    );
}

#[test]
fn joins_under_a_different_identity_per_nonce_so_a_second_syncer_does_not_evict_the_first() {
    // Given two syncers mirroring the same session
    let first = syncer_identity("1780828020298-abc", "1780828020298-4242");

    // When the second joins
    let second = syncer_identity("1780828020298-abc", "1780828020299-4243");

    // Then their identities differ — LiveKit disconnects the older participant when a second one
    // joins with the same identity, so a shared identity is two syncers evicting each other.
    assert_ne!(first, second);
}

// ---------------------------------------------------------------------------
// Opening the session's room before joining it
//
// `ConnectSession` is local work: a daemon answers it without opening a room for a caller that did
// not arrive over LiveKit, and a mirror is such a caller. The LiveKit integration's own
// `SessionRoomService.OpenSessionRoom` is what puts the daemon in the room a mirror then joins.
// ---------------------------------------------------------------------------

const OPEN_SESSION_ROOM: &str = "/rpc/session_room.SessionRoomService/OpenSessionRoom";

/// Patience for a call to a daemon on loopback.
const A_LOOPBACK_CALL: std::time::Duration = std::time::Duration::from_secs(5);

#[tokio::test]
async fn asks_the_daemon_to_open_the_session_room_through_the_room_service() {
    // Given a daemon that serves the room service (and nothing else: any other path is a 404)
    let daemon =
        tddy_testing_commons::stub_http_routed::a_stub_http_endpoint_routing(&[(
            OPEN_SESSION_ROOM,
            "",
        )])
        .await;

    // When a mirror asks for the session's room
    tddy_session_sync::open_session_room(
        &daemon.base_url(),
        "an-access-token",
        "1780828020298-abc",
        A_LOOPBACK_CALL,
    )
    .await
    .expect("the room service answered");

    // Then the one call made was to the room service, naming the session and carrying the token
    let sent = daemon.requests_to(OPEN_SESSION_ROOM);
    assert_eq!(daemon.paths(), vec![OPEN_SESSION_ROOM.to_string()]);
    assert_eq!(
        <tddy_service::proto::session_room::OpenSessionRoomRequest as prost::Message>::decode(
            sent[0].body.as_bytes()
        )
        .expect("the request body is an OpenSessionRoomRequest"),
        tddy_service::proto::session_room::OpenSessionRoomRequest {
            session_token: "an-access-token".to_string(),
            session_id: "1780828020298-abc".to_string(),
        }
    );
}

#[tokio::test]
async fn reports_a_daemon_without_the_room_service_as_refusing_open_session_room() {
    // Given a daemon with no LiveKit integration wired: it serves no room service at all
    let daemon = tddy_testing_commons::stub_http_routed::a_stub_http_endpoint_routing(&[]).await;

    // When a mirror asks for the session's room
    let refused = tddy_session_sync::open_session_room(
        &daemon.base_url(),
        "an-access-token",
        "1780828020298-abc",
        A_LOOPBACK_CALL,
    )
    .await
    .expect_err("a daemon without the room service cannot open a room");

    // Then the refusal names the call that was refused, so the operator learns the daemon has no
    // LiveKit integration rather than that a room is missing
    assert_eq!(
        refused,
        tddy_session_sync::DaemonHttpError::Refused {
            method: "OpenSessionRoom",
            code: "404".to_string(),
            message: String::new(),
        }
    );
}

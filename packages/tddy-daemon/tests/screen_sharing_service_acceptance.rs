//! Acceptance tests: Screen Sharing control-plane service.
//!
//! PRD: docs/ft/web/screen-sharing-sessions.md (AC-SS-2 through AC-SS-9).
//!
//! These tests verify that `ScreenSharingServiceImpl` correctly:
//!   1. Rejects unauthenticated requests.
//!   2. Requires vault unlock before operations that need the key.
//!   3. Accepts the correct passphrase via `UnlockVault` and caches the key.
//!   4. Adds, lists, and removes targets, preserving the `protocol` field.
//!   5. Returns generalized `screenshare:`-prefixed coordinates from `StartStream`.
//!
//! All tests reference `tddy_service::proto::screen_sharing` and
//! `tddy_daemon::screen_sharing_service` which do not yet exist — they will
//! fail to compile until the green phase implements the renamed modules.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

use tddy_daemon::screen_sharing_service::{ScreenSharingKeyCache, ScreenSharingServiceImpl};
use tddy_rpc::{Code, Request};
use tddy_service::proto::screen_sharing::{
    AddTargetRequest, ListTargetsRequest, Protocol, ScreenSharingService, StartStreamRequest,
    UnlockVaultRequest,
};

const VALID_TOKEN: &str = "valid-token";
const SESSION_ID: &str = "ss-test-session-aabbccdd";
const PASSPHRASE: &str = "hunter2-passphrase";

type SessionsBaseFn = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolverFn = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_service(
    sessions_dir: &std::path::Path,
) -> (ScreenSharingServiceImpl, ScreenSharingKeyCache) {
    let sessions_path = sessions_dir.to_path_buf();
    let sessions_base: SessionsBaseFn = Arc::new(move |_user| Some(sessions_path.clone()));
    let user_resolver: UserResolverFn = Arc::new(|token| {
        if token == VALID_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    let key_cache: ScreenSharingKeyCache = Arc::new(Mutex::new(HashMap::new()));
    let svc = ScreenSharingServiceImpl::new(user_resolver, sessions_base, Arc::clone(&key_cache));
    (svc, key_cache)
}

fn session_dir(base: &std::path::Path) -> PathBuf {
    let dir = base.join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------
// SS-Svc-1: unauthenticated token is rejected
// ---------------------------------------------------------------------------

/// **invalid_token_is_rejected**: all ScreenSharingService RPCs must return
/// `UNAUTHENTICATED` when presented with an invalid session token.
#[tokio::test]
async fn invalid_token_is_rejected() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let (svc, _cache) = make_service(tmp.path());

    // When — list targets with a bad token
    let err = svc
        .list_targets(Request::new(ListTargetsRequest {
            session_token: "bad-token".to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect_err("must fail with invalid token");

    // Then
    assert_eq!(
        err.code,
        Code::Unauthenticated,
        "invalid token must yield Unauthenticated; got {:?}",
        err.code
    );
}

// ---------------------------------------------------------------------------
// SS-Svc-2: add_target requires vault to be unlocked
// ---------------------------------------------------------------------------

/// **add_target_before_unlock_is_rejected**: `AddTarget` before unlocking the vault
/// must return `FAILED_PRECONDITION` (vault locked).
#[tokio::test]
async fn add_target_before_unlock_is_rejected() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let _session = session_dir(tmp.path());
    let (svc, _cache) = make_service(tmp.path());

    // When — add a target without first unlocking
    let err = svc
        .add_target(Request::new(AddTargetRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            label: "Test VM".to_string(),
            host: "192.168.1.1".to_string(),
            port: 5900,
            password: "secret".to_string(),
            protocol: Protocol::Vnc as i32,
            username: String::new(),
        }))
        .await
        .expect_err("add without unlock must fail");

    // Then
    assert_eq!(
        err.code,
        Code::FailedPrecondition,
        "locked vault must yield FailedPrecondition; got {:?}",
        err.code
    );
}

// ---------------------------------------------------------------------------
// SS-Svc-3: unlock → add VNC target → list returns protocol=VNC
// ---------------------------------------------------------------------------

/// **unlock_then_add_vnc_target_then_list_returns_vnc_protocol**: unlocking the vault,
/// adding a VNC target, then listing must return the target with `protocol == VNC`.
#[tokio::test]
async fn unlock_then_add_vnc_target_then_list_returns_vnc_protocol() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let _session = session_dir(tmp.path());
    let (svc, _cache) = make_service(tmp.path());

    // When — unlock
    svc.unlock_vault(Request::new(UnlockVaultRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        passphrase: PASSPHRASE.to_string(),
    }))
    .await
    .expect("unlock must succeed");

    // When — add a VNC target
    svc.add_target(Request::new(AddTargetRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        label: "VNC Dev Box".to_string(),
        host: "10.0.0.5".to_string(),
        port: 5900,
        password: String::new(),
        protocol: Protocol::Vnc as i32,
        username: String::new(),
    }))
    .await
    .expect("add VNC target must succeed");

    // When — list
    let list_resp = svc
        .list_targets(Request::new(ListTargetsRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect("list must succeed")
        .into_inner();

    // Then
    assert_eq!(
        list_resp.targets.len(),
        1,
        "list must contain exactly one target"
    );
    assert_eq!(
        list_resp.targets[0].protocol,
        Protocol::Vnc as i32,
        "VNC target must have protocol=VNC; got {}",
        list_resp.targets[0].protocol
    );
    assert_eq!(list_resp.targets[0].label, "VNC Dev Box");
}

// ---------------------------------------------------------------------------
// SS-Svc-4: unlock → add RDP target → list returns protocol=RDP
// ---------------------------------------------------------------------------

/// **unlock_then_add_rdp_target_then_list_returns_rdp_protocol**: unlocking the vault,
/// adding an RDP target, then listing must return the target with `protocol == RDP`.
#[tokio::test]
async fn unlock_then_add_rdp_target_then_list_returns_rdp_protocol() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let _session = session_dir(tmp.path());
    let (svc, _cache) = make_service(tmp.path());

    // When — unlock
    svc.unlock_vault(Request::new(UnlockVaultRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        passphrase: PASSPHRASE.to_string(),
    }))
    .await
    .expect("unlock must succeed");

    // When — add an RDP target
    svc.add_target(Request::new(AddTargetRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        label: "Windows Dev Box".to_string(),
        host: "10.0.0.10".to_string(),
        port: 3389,
        password: String::new(),
        protocol: Protocol::Rdp as i32,
        username: "tester".to_string(),
    }))
    .await
    .expect("add RDP target must succeed");

    // When — list
    let list_resp = svc
        .list_targets(Request::new(ListTargetsRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
        }))
        .await
        .expect("list must succeed")
        .into_inner();

    // Then
    assert_eq!(
        list_resp.targets.len(),
        1,
        "list must contain exactly one target"
    );
    assert_eq!(
        list_resp.targets[0].protocol,
        Protocol::Rdp as i32,
        "RDP target must have protocol=RDP; got {}",
        list_resp.targets[0].protocol
    );
    assert_eq!(list_resp.targets[0].label, "Windows Dev Box");
}

// ---------------------------------------------------------------------------
// SS-Svc-5: wrong passphrase is rejected by unlock
// ---------------------------------------------------------------------------

/// **wrong_passphrase_is_rejected_by_unlock**: `UnlockVault` with an incorrect
/// passphrase (after a vault already exists) must return `UNAUTHENTICATED`.
#[tokio::test]
async fn wrong_passphrase_is_rejected_by_unlock() {
    // Given — create the vault with the correct passphrase first
    let tmp = tempfile::tempdir().unwrap();
    let _session = session_dir(tmp.path());
    let (svc, _cache) = make_service(tmp.path());

    svc.unlock_vault(Request::new(UnlockVaultRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        passphrase: PASSPHRASE.to_string(),
    }))
    .await
    .expect("initial unlock must succeed");

    // Re-create the service to reset the key cache (simulate a new session start)
    let (svc2, _cache2) = make_service(tmp.path());

    // When — try unlocking with wrong passphrase
    let err = svc2
        .unlock_vault(Request::new(UnlockVaultRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            passphrase: "wrong-passphrase".to_string(),
        }))
        .await
        .expect_err("wrong passphrase must fail");

    // Then
    assert_eq!(
        err.code,
        Code::Unauthenticated,
        "wrong passphrase must yield Unauthenticated; got {:?}",
        err.code
    );
}

// ---------------------------------------------------------------------------
// SS-Svc-6: start_stream returns screenshare-prefixed track name and identity
// ---------------------------------------------------------------------------

/// **start_stream_returns_screenshare_prefixed_track_and_identity**: `StartStream`
/// for an added target must return `track_name` starting with `"screenshare:"` and
/// `bridge_identity` starting with `"screenshare-"`.
#[tokio::test]
async fn start_stream_returns_screenshare_prefixed_track_and_identity() {
    use tddy_core::session_metadata::{
        write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
    };

    let tmp = tempfile::tempdir().unwrap();
    let session_dir_path = session_dir(tmp.path());

    // Write a minimal .session.yaml so the service can find the livekit_room.
    write_initial_tool_session_metadata(
        &session_dir_path,
        InitialToolSessionMetadataOpts {
            project_id: "proj-ss-1".to_string(),
            livekit_room: Some("room-ss-test".to_string()),
            ..Default::default()
        },
    )
    .expect("session metadata must write");

    let (svc, _cache) = make_service(tmp.path());

    // Unlock the vault first
    svc.unlock_vault(Request::new(UnlockVaultRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        passphrase: PASSPHRASE.to_string(),
    }))
    .await
    .expect("unlock must succeed");

    // Add a target (password-less VNC for simplicity)
    let target = svc
        .add_target(Request::new(AddTargetRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            label: "Test VM".to_string(),
            host: "127.0.0.1".to_string(),
            port: 5900,
            password: String::new(),
            protocol: Protocol::Vnc as i32,
            username: String::new(),
        }))
        .await
        .expect("add must succeed")
        .into_inner();
    let target_id = target.target.expect("must have target").id;

    // When — start stream
    let resp = svc
        .start_stream(Request::new(StartStreamRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            target_id: target_id.clone(),
        }))
        .await
        .expect("start_stream must succeed")
        .into_inner();

    // Then — generalized screenshare: prefix (not vnc: or rdp:)
    assert!(
        resp.track_name.starts_with("screenshare:"),
        "track_name must start with 'screenshare:'; got '{}'",
        resp.track_name
    );
    assert!(
        resp.bridge_identity.starts_with("screenshare-"),
        "bridge_identity must start with 'screenshare-'; got '{}'",
        resp.bridge_identity
    );
    assert!(
        !resp.livekit_room.is_empty(),
        "livekit_room must be non-empty"
    );
}

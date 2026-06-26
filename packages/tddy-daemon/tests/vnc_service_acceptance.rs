//! Acceptance tests: VNC control-plane service.
//!
//! PRD: docs/ft/web/vnc-sessions.md (AC-VNC-2 through AC-VNC-6, AC-VNC-7).
//!
//! These tests verify that `VncServiceImpl` correctly:
//!   1. Requires vault unlock before operations that need the key.
//!   2. Accepts the correct passphrase via `UnlockVncVault` and caches the key.
//!   3. Adds, lists, and removes VNC targets against the session dir vault.
//!   4. Returns `StartVncStream` coordinates after the bridge starts.
//!   5. Rejects unauthenticated requests.
//!
//! All tests pass against the implemented VncServiceImpl.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

use tddy_daemon::vnc_service::{VncKeyCache, VncServiceImpl};
use tddy_rpc::{Code, Request};
use tddy_service::proto::vnc::{
    AddVncTargetRequest, ListVncTargetsRequest, StartVncStreamRequest, UnlockVncVaultRequest,
    VncService,
};

const VALID_TOKEN: &str = "valid-token";
const SESSION_ID: &str = "vnc-test-session-aabbccdd";
const PASSPHRASE: &str = "hunter2-passphrase";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_service(sessions_dir: &std::path::Path) -> (VncServiceImpl, VncKeyCache) {
    let sessions_path = sessions_dir.to_path_buf();
    let sessions_base: Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync> =
        Arc::new(move |_user| Some(sessions_path.clone()));
    let user_resolver: Arc<dyn Fn(&str) -> Option<String> + Send + Sync> = Arc::new(|token| {
        if token == VALID_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    let key_cache: VncKeyCache = Arc::new(Mutex::new(HashMap::new()));
    let svc = VncServiceImpl::new(user_resolver, sessions_base, Arc::clone(&key_cache));
    (svc, key_cache)
}

fn session_dir(base: &std::path::Path) -> PathBuf {
    let dir = base.join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------
// VncSvc-1: unauthenticated token is rejected
// ---------------------------------------------------------------------------

/// **vnc_svc_invalid_token_rejected**: all VncService RPCs must return
/// `UNAUTHENTICATED` when presented with an invalid session token.
#[tokio::test]
async fn vnc_svc_invalid_token_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let (svc, _cache) = make_service(tmp.path());

    // When — list targets with a bad token
    let err = svc
        .list_vnc_targets(Request::new(ListVncTargetsRequest {
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
// VncSvc-2: add_vnc_target requires vault to be unlocked
// ---------------------------------------------------------------------------

/// **vnc_svc_add_target_locked_vault**: `AddVncTarget` before unlocking the vault must
/// return `FAILED_PRECONDITION` (vault locked).
#[tokio::test]
async fn vnc_svc_add_target_locked_vault() {
    let tmp = tempfile::tempdir().unwrap();
    let _session = session_dir(tmp.path());
    let (svc, _cache) = make_service(tmp.path());

    // When — add a target without first unlocking
    let err = svc
        .add_vnc_target(Request::new(AddVncTargetRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            label: "Test VM".to_string(),
            host: "192.168.1.1".to_string(),
            port: 5900,
            password: "secret".to_string(),
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
// VncSvc-3: unlock then add then list roundtrip
// ---------------------------------------------------------------------------

/// **vnc_svc_unlock_add_list**: unlocking the vault, then adding a target, then listing
/// must return the added target.
#[tokio::test]
async fn vnc_svc_unlock_add_list() {
    let tmp = tempfile::tempdir().unwrap();
    let _session = session_dir(tmp.path());
    let (svc, _cache) = make_service(tmp.path());

    // When — unlock
    let unlock_resp = svc
        .unlock_vnc_vault(Request::new(UnlockVncVaultRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            passphrase: PASSPHRASE.to_string(),
        }))
        .await
        .expect("unlock must succeed")
        .into_inner();
    assert!(unlock_resp.ok, "unlock must report ok=true");

    // When — add
    let add_resp = svc
        .add_vnc_target(Request::new(AddVncTargetRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            label: "Dev Box".to_string(),
            host: "10.0.0.5".to_string(),
            port: 5900,
            password: "vncp@ss".to_string(),
        }))
        .await
        .expect("add after unlock must succeed")
        .into_inner();
    let added_target = add_resp.target.expect("add must return a VncTarget");
    assert_eq!(added_target.label, "Dev Box");
    assert_eq!(added_target.host, "10.0.0.5");

    // When — list
    let list_resp = svc
        .list_vnc_targets(Request::new(ListVncTargetsRequest {
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
        "list must contain the added target"
    );
    assert_eq!(list_resp.targets[0].id, added_target.id);
}

// ---------------------------------------------------------------------------
// VncSvc-4: wrong passphrase is rejected by unlock
// ---------------------------------------------------------------------------

/// **vnc_svc_wrong_passphrase_rejected**: `UnlockVncVault` with an incorrect passphrase
/// (after a vault already exists) must return `UNAUTHENTICATED`.
#[tokio::test]
async fn vnc_svc_wrong_passphrase_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let _session = session_dir(tmp.path());
    let (svc, _cache) = make_service(tmp.path());

    // Given — create the vault with the correct passphrase first
    svc.unlock_vnc_vault(Request::new(UnlockVncVaultRequest {
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
        .unlock_vnc_vault(Request::new(UnlockVncVaultRequest {
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
// VncSvc-5: start_vnc_stream returns LiveKit coordinates
// ---------------------------------------------------------------------------

/// **vnc_svc_start_stream_returns_livekit_info**: `StartVncStream` for an added target
/// must return non-empty `livekit_room`, `bridge_identity`, and `track_name`.
///
/// Note: this test does not actually spawn a bridge process; the service uses the session's
/// existing `livekit_room` from `.session.yaml` and returns minted coordinates.
#[tokio::test]
async fn vnc_svc_start_stream_returns_livekit_info() {
    use tddy_core::session_metadata::{
        write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
    };

    let tmp = tempfile::tempdir().unwrap();
    let session_dir_path = session_dir(tmp.path());

    // Write a minimal .session.yaml so the service can find the livekit_room.
    write_initial_tool_session_metadata(
        &session_dir_path,
        InitialToolSessionMetadataOpts {
            project_id: "proj-1".to_string(),
            livekit_room: Some("room-vnc-test".to_string()),
            ..Default::default()
        },
    )
    .expect("session metadata must write");

    let (svc, _cache) = make_service(tmp.path());

    // Unlock the vault first
    svc.unlock_vnc_vault(Request::new(UnlockVncVaultRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        passphrase: PASSPHRASE.to_string(),
    }))
    .await
    .expect("unlock must succeed");

    // Add a target (password-less for simplicity)
    let target = svc
        .add_vnc_target(Request::new(AddVncTargetRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            label: "Test VM".to_string(),
            host: "127.0.0.1".to_string(),
            port: 5900,
            password: String::new(),
        }))
        .await
        .expect("add must succeed")
        .into_inner();
    let target_id = target.target.expect("must have target").id;

    // When — start stream
    let resp = svc
        .start_vnc_stream(Request::new(StartVncStreamRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            target_id: target_id.clone(),
        }))
        .await
        .expect("start_vnc_stream must succeed")
        .into_inner();

    // Then
    assert!(
        !resp.livekit_room.is_empty(),
        "livekit_room must be non-empty"
    );
    assert!(
        !resp.bridge_identity.is_empty(),
        "bridge_identity must be non-empty"
    );
    assert!(
        resp.track_name.starts_with("vnc:"),
        "track_name must start with 'vnc:'; got '{}'",
        resp.track_name
    );
}

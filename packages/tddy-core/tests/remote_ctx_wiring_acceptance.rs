//! Acceptance tests: ctx → InvokeRequest.remote population (Phase 5 follow-up).
//!
//! AC: when `WorkflowContext` has `"remote_daemon_url"`, `"remote_session_id"`, and
//! `"remote_session_token"` keys set, `BackendInvokeTask` constructs `InvokeRequest.remote`
//! as `Some(RemoteToolEnv)` with all fields populated.

use tddy_core::backend::{InvokeRequest, RemoteToolEnv};
use tddy_core::workflow::extract_remote_env_from_ctx;

/// Phase 5 AC: `extract_remote_env_from_ctx` returns `Some(RemoteToolEnv)` when required keys present.
#[test]
fn extract_remote_env_returns_some_when_required_keys_set() {
    // Given
    let mut ctx: std::collections::HashMap<String, String> = Default::default();
    ctx.insert(
        "remote_daemon_url".to_string(),
        "http://relay.local:9000".to_string(),
    );
    ctx.insert("remote_session_id".to_string(), "sess-abc123".to_string());
    ctx.insert("remote_session_token".to_string(), "tok-xyz".to_string());
    ctx.insert(
        "remote_daemon_instance_id".to_string(),
        "relay-local".to_string(),
    );

    // When
    let env = extract_remote_env_from_ctx(&ctx)
        .expect("extract_remote_env_from_ctx must return Some when required keys are present");

    // Then
    assert_eq!(env.daemon_url, "http://relay.local:9000");
    assert_eq!(env.session_id, "sess-abc123");
    assert_eq!(env.session_token, "tok-xyz");
    assert_eq!(env.daemon_instance_id.as_deref(), Some("relay-local"));
}

/// Phase 5 AC: `extract_remote_env_from_ctx` returns `None` when required keys are absent.
#[test]
fn extract_remote_env_returns_none_when_keys_absent() {
    // Given
    let ctx: std::collections::HashMap<String, String> = Default::default();

    // When
    let env = extract_remote_env_from_ctx(&ctx);

    // Then
    assert!(
        env.is_none(),
        "extract_remote_env_from_ctx must return None when no remote keys are set"
    );
}

/// Phase 5 AC: `extract_remote_env_from_ctx` returns `None` when only SOME required keys are
/// set (partial config is unusable and must not produce a partial RemoteToolEnv).
#[test]
fn extract_remote_env_returns_none_on_partial_keys() {
    // Given — only daemon_url, missing session_id and session_token
    let mut ctx: std::collections::HashMap<String, String> = Default::default();
    ctx.insert(
        "remote_daemon_url".to_string(),
        "http://relay.local:9000".to_string(),
    );

    // When
    let env = extract_remote_env_from_ctx(&ctx);

    // Then
    assert!(
        env.is_none(),
        "extract_remote_env_from_ctx must return None when required session keys are absent; \
         partial config must not produce an unusable RemoteToolEnv"
    );
}

/// Phase 5 AC: optional LiveKit keys are captured when present, ignored when absent.
#[test]
fn extract_remote_env_captures_optional_livekit_keys() {
    // Given
    let mut ctx: std::collections::HashMap<String, String> = Default::default();
    ctx.insert(
        "remote_daemon_url".to_string(),
        "http://relay.local:9000".to_string(),
    );
    ctx.insert("remote_session_id".to_string(), "sess-789".to_string());
    ctx.insert("remote_session_token".to_string(), "tok-abc".to_string());
    ctx.insert(
        "remote_livekit_url".to_string(),
        "ws://lk.example.com".to_string(),
    );
    ctx.insert("remote_livekit_room".to_string(), "common-room".to_string());
    ctx.insert(
        "remote_server_identity".to_string(),
        "relay-local-sess-789".to_string(),
    );

    // When
    let env = extract_remote_env_from_ctx(&ctx).expect("must return Some with all keys set");

    // Then
    assert_eq!(env.livekit_url.as_deref(), Some("ws://lk.example.com"));
    assert_eq!(env.livekit_room.as_deref(), Some("common-room"));
    assert_eq!(env.server_identity.as_deref(), Some("relay-local-sess-789"));
}

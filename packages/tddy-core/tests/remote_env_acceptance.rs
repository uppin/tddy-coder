//! Acceptance tests: RemoteToolEnv in InvokeRequest (PRD: docs/ft/daemon/remote-codebase-mode.md, Phase 5).
//!
//! AC: when `InvokeRequest.remote` contains a `RemoteToolEnv`, the Claude backend must export
//! all TDDY_REMOTE_* env vars before spawning the subprocess — so that the inherited
//! `tddy-tools --mcp` can route calls to the relay.

use tddy_core::backend::{InvokeRequest, RemoteToolEnv};

/// Phase 5 AC: `RemoteToolEnv` struct exists with the expected fields.
#[test]
fn remote_tool_env_struct_has_required_fields() {
    // Given
    let env = RemoteToolEnv {
        daemon_url: "http://127.0.0.1:9000".to_string(),
        session_id: "sess-abc123".to_string(),
        session_token: "tok-xyz".to_string(),
        daemon_instance_id: Some("relay-local".to_string()),
        livekit_url: Some("ws://lk.example.com".to_string()),
        livekit_room: Some("common-room".to_string()),
        server_identity: Some("relay-local-sess-abc123".to_string()),
    };

    // Then
    assert_eq!(env.daemon_url, "http://127.0.0.1:9000");
    assert_eq!(env.session_id, "sess-abc123");
    assert_eq!(env.session_token, "tok-xyz");
    assert_eq!(env.daemon_instance_id.as_deref(), Some("relay-local"));
}

/// Phase 5 AC: `InvokeRequest` has a `remote: Option<RemoteToolEnv>` field.
#[test]
fn invoke_request_has_remote_field() {
    // Given
    let env = RemoteToolEnv {
        daemon_url: "http://127.0.0.1:9000".to_string(),
        session_id: "sess-abc123".to_string(),
        session_token: "tok-xyz".to_string(),
        daemon_instance_id: None,
        livekit_url: None,
        livekit_room: None,
        server_identity: None,
    };

    // When
    let req = InvokeRequest {
        prompt: "test".to_string(),
        remote: Some(env),
        ..InvokeRequest::default()
    };

    // Then
    assert!(
        req.remote.is_some(),
        "InvokeRequest.remote must be Some when set"
    );
    assert_eq!(
        req.remote.as_ref().unwrap().session_id,
        "sess-abc123",
        "session_id must survive the round-trip through InvokeRequest"
    );
}

/// Phase 5 AC: `InvokeRequest.remote = None` means no remote env — field must default to None.
#[test]
fn invoke_request_remote_defaults_to_none() {
    // When
    let req = InvokeRequest {
        prompt: "test".to_string(),
        remote: None,
        ..InvokeRequest::default()
    };

    // Then
    assert!(
        req.remote.is_none(),
        "InvokeRequest.remote must default to None"
    );
}

/// Phase 5 AC: `RemoteToolEnv::env_pairs()` (or equivalent) returns all TDDY_REMOTE_* key-value
/// pairs that the Claude backend needs to export before spawning the subprocess.
#[test]
fn remote_tool_env_env_pairs_covers_all_required_vars() {
    // Given
    let env = RemoteToolEnv {
        daemon_url: "http://relay.local:9000".to_string(),
        session_id: "sess-789".to_string(),
        session_token: "tok-abc".to_string(),
        daemon_instance_id: Some("relay-local".to_string()),
        livekit_url: Some("ws://lk.example.com".to_string()),
        livekit_room: Some("common-room".to_string()),
        server_identity: Some("relay-local-sess-789".to_string()),
    };

    // When
    let pairs = env.env_pairs();
    let map: std::collections::HashMap<String, String> = pairs.into_iter().collect();

    // Then
    assert!(
        map.contains_key("TDDY_REMOTE_DAEMON_URL"),
        "env_pairs must include TDDY_REMOTE_DAEMON_URL; got keys: {:?}",
        map.keys().collect::<Vec<_>>()
    );
    assert!(
        map.contains_key("TDDY_REMOTE_SESSION_ID"),
        "env_pairs must include TDDY_REMOTE_SESSION_ID"
    );
    assert!(
        map.contains_key("TDDY_REMOTE_SESSION_TOKEN"),
        "env_pairs must include TDDY_REMOTE_SESSION_TOKEN"
    );
    assert_eq!(map["TDDY_REMOTE_DAEMON_URL"], "http://relay.local:9000");
    assert_eq!(map["TDDY_REMOTE_SESSION_ID"], "sess-789");
    assert_eq!(map["TDDY_REMOTE_SESSION_TOKEN"], "tok-abc");
}

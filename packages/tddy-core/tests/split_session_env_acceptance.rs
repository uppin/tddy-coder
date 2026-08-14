//! Unit tests: the environment a **split** session's agent is spawned with.
//!
//! In a split session the agent runs on one host and its git worktree lives on another
//! (docs/ft/daemon/remote-managed-worktree.md). `tddy-tools --mcp` inherits `TDDY_REMOTE_*` from the
//! agent process and uses it to open a LiveKit RPC client to the *codebase* daemon.
//!
//! The LiveKit fields on `RemoteToolEnv` have existed since remote-codebase mode but were always
//! `None` — nothing populated them. Split placement is the first thing that does, and it adds the
//! one field the transport cannot work without: a scoped join token. The daemon must never hand the
//! agent `livekit.api_secret` instead, which would let it join any room as any identity.

use tddy_core::backend::RemoteToolEnv;

/// The env a split session's agent receives: a scoped join token plus the identity of the daemon
/// that holds the worktree.
fn a_split_session_env() -> RemoteToolEnv {
    RemoteToolEnv {
        daemon_url: String::new(),
        session_id: "019d105b-ac0f-78d3-9a89-409731145a40".to_string(),
        session_token: "caller-session-token".to_string(),
        daemon_instance_id: Some("workstation-b".to_string()),
        livekit_url: Some("wss://livekit.example.invalid".to_string()),
        livekit_room: Some("tddy-lobby".to_string()),
        server_identity: Some("daemon-workstation-b".to_string()),
        livekit_token: Some("a-scoped-join-jwt".to_string()),
    }
}

/// A co-located managed session: reached over the relay's HTTP endpoint, no LiveKit at all.
fn a_co_located_env() -> RemoteToolEnv {
    RemoteToolEnv {
        daemon_url: "http://127.0.0.1:9321".to_string(),
        session_id: "019d105b-ac0f-78d3-9a89-409731145a41".to_string(),
        session_token: "caller-session-token".to_string(),
        daemon_instance_id: None,
        livekit_url: None,
        livekit_room: None,
        server_identity: None,
        livekit_token: None,
    }
}

fn env_value(env: &RemoteToolEnv, key: &str) -> Option<String> {
    env.env_pairs()
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

#[test]
fn a_split_session_env_carries_the_scoped_livekit_join_token() {
    // When
    let value = env_value(&a_split_session_env(), "TDDY_REMOTE_LIVEKIT_TOKEN");

    // Then — without it the agent cannot join the room, and the only alternative would be handing
    // it the daemon's API secret
    assert_eq!(value, Some("a-scoped-join-jwt".to_string()));
}

#[test]
fn a_split_session_env_carries_the_room_url_and_the_codebase_daemon_identity() {
    // Given
    let env = a_split_session_env();

    // Then — the client needs all three to address the right participant on the right room
    assert_eq!(
        env_value(&env, "TDDY_REMOTE_LIVEKIT_URL"),
        Some("wss://livekit.example.invalid".to_string())
    );
    assert_eq!(
        env_value(&env, "TDDY_REMOTE_LIVEKIT_ROOM"),
        Some("tddy-lobby".to_string())
    );
    assert_eq!(
        env_value(&env, "TDDY_REMOTE_SERVER_IDENTITY"),
        Some("daemon-workstation-b".to_string())
    );
}

#[test]
fn a_split_session_env_carries_the_codebase_session_id_as_the_remote_session() {
    // When
    let value = env_value(&a_split_session_env(), "TDDY_REMOTE_SESSION_ID");

    // Then — the codebase daemon resolves the worktree from its *own* sessions base keyed by this
    // id, so it must be the workspace session on that host, never the agent's own session id
    assert_eq!(
        value,
        Some("019d105b-ac0f-78d3-9a89-409731145a40".to_string())
    );
}

#[test]
fn a_co_located_env_emits_no_livekit_token() {
    // When
    let value = env_value(&a_co_located_env(), "TDDY_REMOTE_LIVEKIT_TOKEN");

    // Then — an absent token must be absent from the environment, not present and empty: the
    // transport detector treats an empty value as "configured but broken"
    assert_eq!(value, None);
}

#[test]
fn a_co_located_env_is_unchanged_by_the_new_field() {
    // When
    let keys: Vec<String> = a_co_located_env()
        .env_pairs()
        .into_iter()
        .map(|(k, _)| k)
        .collect();

    // Then — every session that is not split must see exactly the environment it saw before split
    // placement existed
    assert_eq!(
        keys,
        vec![
            "TDDY_REMOTE_DAEMON_URL".to_string(),
            "TDDY_REMOTE_SESSION_ID".to_string(),
            "TDDY_REMOTE_SESSION_TOKEN".to_string(),
        ]
    );
}

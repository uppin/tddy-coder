//! Unit tests: the environment `tddy-tools notify` reads its session from (`#home-inbox` 1/8).

use std::path::Path;

use crate::notify_env::notify_env_pairs;
use tddy_service::session_activity::notify_env;

const SESSION: &str = "01a19c2e-55d0-7a41-9b3e-4f1d2c3b4a59";
const OS_USER: &str = "ada";
const HOOK_TOKEN: &str = "the-token-its-hooks-present";
const DAEMON_URL: &str = "http://127.0.0.1:8899";

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn a_daemon_serving_http_is_named_by_its_url() {
    // When the environment is built for a daemon with no agent-tool socket
    let env = notify_env_pairs(SESSION, OS_USER, HOOK_TOKEN, DAEMON_URL, None);

    // Then it names the session, its owner, its token and the URL — and nothing else
    assert_eq!(
        env,
        pairs(&[
            (notify_env::SESSION_ID, SESSION),
            (notify_env::OS_USER, OS_USER),
            (notify_env::HOOK_TOKEN, HOOK_TOKEN),
            (notify_env::DAEMON_URL, DAEMON_URL),
        ])
    );
}

#[test]
fn a_daemon_with_an_agent_tool_socket_is_named_by_the_socket_and_never_the_url() {
    // When the environment is built for an embedded daemon, whose URL answers no RPC
    let socket = Path::new("/tmp/tddy-agent-tools-0123.sock");
    let env = notify_env_pairs(SESSION, OS_USER, HOOK_TOKEN, DAEMON_URL, Some(socket));

    // Then the socket replaces the URL
    assert_eq!(
        env,
        pairs(&[
            (notify_env::SESSION_ID, SESSION),
            (notify_env::OS_USER, OS_USER),
            (notify_env::HOOK_TOKEN, HOOK_TOKEN),
            (notify_env::DAEMON_SOCKET, "/tmp/tddy-agent-tools-0123.sock"),
        ])
    );
}

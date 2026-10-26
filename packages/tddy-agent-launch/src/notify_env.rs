//! What a daemon-launched claude-cli or cursor-cli agent is given so it can notify its operator
//! with `tddy-tools notify` (`#home-inbox` 1/8).
//!
//! Two parts:
//!
//! - **The environment** ([`notify_env_pairs`]): the session it runs in, the OS user that owns it,
//!   its hook token and how to reach the daemon. These are the credentials its hooks already carry
//!   in `.claude/settings.local.json`, so exporting them grants the agent nothing it could not
//!   already do.
//! - **The guidance** ([`AGENT_NOTIFY_GUIDANCE`]): one paragraph appended to a claude-cli agent's
//!   system prompt, telling it the command exists and when — and when not — to use it.

use std::path::Path;

use tddy_service::session_activity::notify_env;

/// The paragraph a claude-cli agent's system prompt is given about `tddy-tools notify`.
pub const AGENT_NOTIFY_GUIDANCE: &str = include_str!("agent_notify_guidance.md");

/// The `(name, value)` pairs `tddy-tools notify` reads its session from
/// ([`tddy_service::session_activity::notify_env`]).
///
/// The daemon is named by exactly one of two variables: `TDDY_DAEMON_SOCKET` when `daemon_socket`
/// is given — an embedded daemon serves no HTTP RPC, so a URL there would reach nothing — and
/// `TDDY_DAEMON_URL` otherwise.
#[must_use]
pub fn notify_env_pairs(
    session_id: &str,
    os_user: &str,
    hook_token: &str,
    daemon_url: &str,
    daemon_socket: Option<&Path>,
) -> Vec<(String, String)> {
    let daemon = match daemon_socket {
        Some(socket) => (
            notify_env::DAEMON_SOCKET,
            socket.to_string_lossy().into_owned(),
        ),
        None => (notify_env::DAEMON_URL, daemon_url.to_string()),
    };
    [
        (notify_env::SESSION_ID, session_id.to_string()),
        (notify_env::OS_USER, os_user.to_string()),
        (notify_env::HOOK_TOKEN, hook_token.to_string()),
        daemon,
    ]
    .into_iter()
    .map(|(name, value)| (name.to_string(), value))
    .collect()
}

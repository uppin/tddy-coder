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
    // TODO(agent-notify): implement
    let _ = (session_id, os_user, hook_token, daemon_url, daemon_socket);
    Vec::new()
}

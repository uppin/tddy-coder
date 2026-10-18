//! Starting a session from a harness profile: which session type or tool-session agent runs, on
//! which account, with which model — the `agent_definition` level of the route.

use tddy_core::provider_route::RouteChoice;
use tddy_discovery::harness_profile::HarnessProfile;

/// What a profile starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileStart {
    /// `Some("claude-cli" | "opencode-cli" | "muse-cli")` for a CLI harness; `None` for a tool session.
    pub session_type: Option<String>,
    /// The tool-session agent (`claude`, `codex`, …); `None` for a CLI harness.
    pub agent: Option<String>,
    /// The route choice the profile makes — the account it names, by id or label as written.
    pub choice: RouteChoice,
    pub model: String,
}

/// What `profile` starts; `Err` when its `harness` names no harness this daemon runs.
pub fn profile_start(profile: &HarnessProfile) -> Result<ProfileStart, String> {
    let _ = profile;
    // TODO(agent-def-route): implement
    todo!()
}

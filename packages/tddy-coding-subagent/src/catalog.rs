//! Which coding agents a daemon offers as subagents.

use tddy_discovery::harness_profile::HarnessProfile;

/// The harnesses spoken to over ACP — the ones this crate can run as a subagent.
pub const ACP_HARNESSES: &[&str] = &["claude-acp", "codex-acp", "opencode"];

/// One coding agent a daemon offers as a subagent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodingAgentDef {
    /// The roster key: a harness profile's name, or the `allowed_agents` id.
    pub name: String,
    pub label: Option<String>,
    /// The harness it runs (`claude-acp`, `codex-acp`, `opencode`).
    pub harness: String,
    /// The model it runs; a bare `allowed_agents` entry gets the harness's default model.
    pub model: String,
    /// The provider account a profile names; `None` for the vendor's own login.
    pub provider_account: Option<String>,
}

/// The coding agents offered from `profiles` (every one whose harness is in [`ACP_HARNESSES`]) and
/// `allowed_agent_ids` (`(id, label)` pairs whose id is one), profiles first.
pub fn coding_agent_defs(
    _profiles: &[HarnessProfile],
    _allowed_agent_ids: &[(String, Option<String>)],
) -> Vec<CodingAgentDef> {
    // TODO(acp-subagent): implement
    Vec::new()
}

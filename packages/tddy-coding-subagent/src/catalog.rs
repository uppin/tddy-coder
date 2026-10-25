//! Which coding agents a daemon offers as subagents.

use tddy_discovery::harness_profile::HarnessProfile;

/// Claude Code over ACP (`claude-agent-acp`).
pub const CLAUDE_ACP: &str = "claude-acp";
/// Codex over ACP (`codex-acp`).
pub const CODEX_ACP: &str = "codex-acp";
/// OpenCode, which speaks ACP itself (`opencode acp`).
pub const OPENCODE: &str = "opencode";

/// The harnesses spoken to over ACP — the ones this crate can run as a subagent.
pub const ACP_HARNESSES: &[&str] = &[CLAUDE_ACP, CODEX_ACP, OPENCODE];

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
///
/// A name is offered once: an allowed id that a profile of the same name already took is skipped,
/// since two defs under one name would attach as whichever resolved first.
pub fn coding_agent_defs(
    profiles: &[HarnessProfile],
    allowed_agent_ids: &[(String, Option<String>)],
) -> Vec<CodingAgentDef> {
    let from_profiles: Vec<CodingAgentDef> = profiles
        .iter()
        .filter(|profile| is_acp_harness(&profile.harness))
        .map(profile_def)
        .collect();
    let from_allowed: Vec<CodingAgentDef> = allowed_agent_ids
        .iter()
        .filter(|(id, _)| is_acp_harness(id))
        .filter(|(id, _)| !from_profiles.iter().any(|def| &def.name == id))
        .map(|(id, label)| allowed_agent_def(id, label))
        .collect();
    from_profiles.into_iter().chain(from_allowed).collect()
}

fn is_acp_harness(harness: &str) -> bool {
    ACP_HARNESSES.contains(&harness)
}

fn profile_def(profile: &HarnessProfile) -> CodingAgentDef {
    CodingAgentDef {
        name: profile.name.clone(),
        label: profile.label.clone(),
        harness: profile.harness.clone(),
        model: profile.model.clone(),
        provider_account: Some(profile.provider_account.clone())
            .filter(|account| !account.is_empty()),
    }
}

/// A bare `allowed_agents` id: the harness of that name, on its default model and its own login.
fn allowed_agent_def(id: &str, label: &Option<String>) -> CodingAgentDef {
    CodingAgentDef {
        name: id.to_string(),
        label: label.clone(),
        harness: id.to_string(),
        model: tddy_agent_backend::default_model_for_agent(id).to_string(),
        provider_account: None,
    }
}

/// [`coding_agent_defs`] over the harness profiles in `agents_dir` (`<tddyhome>/agents`) — what a
/// daemon resolves a listing, an attach and an open against, so the three read one list.
pub fn coding_agent_defs_in(
    agents_dir: &std::path::Path,
    allowed_agent_ids: &[(String, Option<String>)],
) -> Vec<CodingAgentDef> {
    let profiles = tddy_discovery::harness_profile::load_harness_profiles(agents_dir);
    coding_agent_defs(&profiles, allowed_agent_ids)
}

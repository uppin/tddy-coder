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
    profiles: &[HarnessProfile],
    allowed_agent_ids: &[(String, Option<String>)],
) -> Vec<CodingAgentDef> {
    let from_profiles = profiles
        .iter()
        .filter(|profile| ACP_HARNESSES.contains(&profile.harness.as_str()))
        .map(|profile| CodingAgentDef {
            name: profile.name.clone(),
            label: profile.label.clone(),
            harness: profile.harness.clone(),
            model: profile.model.clone(),
            provider_account: Some(profile.provider_account.clone())
                .filter(|account| !account.is_empty()),
        });
    let from_allowed = allowed_agent_ids
        .iter()
        .filter(|(id, _)| ACP_HARNESSES.contains(&id.as_str()))
        .map(|(id, label)| CodingAgentDef {
            name: id.clone(),
            label: label.clone(),
            harness: id.clone(),
            model: tddy_agent_backend::default_model_for_agent(id).to_string(),
            provider_account: None,
        });
    from_profiles.chain(from_allowed).collect()
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

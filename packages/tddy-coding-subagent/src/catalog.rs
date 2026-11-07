//! Which coding agents a daemon offers as subagents.

use tddy_discovery::harness_profile::HarnessProfile;

use tddy_coding_agent_specs::CodingAgentRegistry;

use crate::transports::{transport_of_spec, Transport};

/// One coding agent a daemon offers as a subagent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodingAgentDef {
    /// The roster key: a harness profile's name, or the `allowed_agents` id.
    pub name: String,
    pub label: Option<String>,
    /// The harness it runs: an ACP harness (`claude-acp`, `codex-acp`, `opencode`) or a headless one
    /// (`claude`, `cursor`, `codex`, `oz`).
    pub harness: String,
    /// The model it runs; a bare `allowed_agents` entry gets the harness's default model.
    pub model: String,
    /// The provider account a profile names; `None` for the vendor's own login.
    pub provider_account: Option<String>,
}

/// [`coding_agent_defs_from`] over the built-in agents' specs: for a caller with no host registry.
pub fn coding_agent_defs(
    profiles: &[HarnessProfile],
    allowed_agent_ids: &[(String, Option<String>)],
) -> Vec<CodingAgentDef> {
    coding_agent_defs_from(
        CodingAgentRegistry::builtin_cached(),
        profiles,
        allowed_agent_ids,
    )
}

/// The coding agents offered from `profiles` and `allowed_agent_ids` (`(id, label)` pairs), profiles
/// first, read from `registry` (`#agent-tools 27/28`): a profile is offered when the registry
/// declares its harness an agent with a transport ([`transport_of_spec`]); a bare `allowed_agents`
/// id only when the registry declares it an ACP agent, on that spec's default model, so a headless
/// harness is offered through a profile alone, which names the model it runs. A harness the
/// registry does not declare is never offered.
///
/// A name is offered once: an allowed id that a profile of the same name already took is skipped,
/// since two defs under one name would attach as whichever resolved first.
pub fn coding_agent_defs_from(
    registry: &CodingAgentRegistry,
    profiles: &[HarnessProfile],
    allowed_agent_ids: &[(String, Option<String>)],
) -> Vec<CodingAgentDef> {
    let from_profiles: Vec<CodingAgentDef> = profiles
        .iter()
        .filter(|profile| {
            registry
                .get(&profile.harness)
                .and_then(transport_of_spec)
                .is_some()
        })
        .map(profile_def)
        .collect();
    let from_allowed: Vec<CodingAgentDef> = allowed_agent_ids
        .iter()
        .filter_map(|(id, label)| {
            let spec = registry.get(id)?;
            (transport_of_spec(spec) == Some(Transport::Acp))
                .then(|| allowed_agent_def(id, label, &spec.agent.models.default))
        })
        .filter(|def| !from_profiles.iter().any(|taken| taken.name == def.name))
        .collect();
    from_profiles.into_iter().chain(from_allowed).collect()
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

/// A bare `allowed_agents` id: the harness of that name, on `default_model` and its own login.
fn allowed_agent_def(id: &str, label: &Option<String>, default_model: &str) -> CodingAgentDef {
    CodingAgentDef {
        name: id.to_string(),
        label: label.clone(),
        harness: id.to_string(),
        model: default_model.to_string(),
        provider_account: None,
    }
}

/// [`coding_agent_defs_from`] over the harness profiles in `agents_dir` (`<tddyhome>/agents`) — what
/// a daemon resolves a listing, an attach and an open against, so the three read one list.
pub fn coding_agent_defs_in(
    registry: &CodingAgentRegistry,
    agents_dir: &std::path::Path,
    allowed_agent_ids: &[(String, Option<String>)],
) -> Vec<CodingAgentDef> {
    let profiles = tddy_discovery::harness_profile::load_harness_profiles(agents_dir);
    coding_agent_defs_from(registry, &profiles, allowed_agent_ids)
}

//! Starting a session from a harness profile: which session type or tool-session agent runs, on
//! which account, with which model — the `agent_definition` level of the route.

use tddy_coding_agent_specs::CodingAgentRegistry;
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

/// What `profile` starts; `Err` when its `harness` names no coding agent this host has. A name
/// ending `-cli` is a CLI session type, launched from the spec of the agent it runs; any other name
/// is a tool-session agent, which must be one the host can run headless or over ACP. A CLI
/// profile's start is then the session type's start.
pub fn profile_start(
    profile: &HarnessProfile,
    agents: &CodingAgentRegistry,
) -> Result<ProfileStart, String> {
    let name = profile.harness.as_str();
    let (session_type, agent) = if agents.for_session_type(name).is_some() {
        (Some(name.to_string()), None)
    } else if agents
        .get(name)
        .is_some_and(|spec| spec.acp().is_some() || spec.headless().is_some())
    {
        (None, Some(name.to_string()))
    } else {
        return Err(format!(
            "profile {:?} names {:?}, which is no harness this daemon runs",
            profile.name, profile.harness
        ));
    };
    Ok(ProfileStart {
        session_type,
        agent,
        choice: RouteChoice::Account(profile.provider_account.clone()),
        model: profile.model.clone(),
    })
}

/// The id of the agent spec `profile`'s harness launches from (`#agent-tools 8/12`): its CLI
/// session type through [`CodingAgentRegistry::for_session_type`] (`opencode-cli` → `opencode`);
/// `None` for a profile that starts a tool session, or names no harness.
pub fn profile_spec_id(profile: &HarnessProfile, agents: &CodingAgentRegistry) -> Option<String> {
    let start = profile_start(profile, agents).ok()?;
    agents
        .for_session_type(start.session_type.as_deref()?)
        .map(|spec| spec.id.clone())
}

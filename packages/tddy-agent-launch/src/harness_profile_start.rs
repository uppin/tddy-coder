//! Starting a session from a harness profile: which session type or tool-session agent runs, on
//! which account, with which model — the `agent_definition` level of the route.

use tddy_core::provider_route::{Harness, RouteChoice};
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

// TODO(harness-clis): a CLI profile's start launches from `profile_spec_id`'s spec.
/// What `profile` starts; `Err` when its `harness` names no harness this daemon runs.
pub fn profile_start(profile: &HarnessProfile) -> Result<ProfileStart, String> {
    let harness = Harness::from_agent_or_session_type(&profile.harness).ok_or_else(|| {
        format!(
            "profile {:?} names {:?}, which is no harness this daemon runs",
            profile.name, profile.harness
        )
    })?;
    let (session_type, agent) = match harness {
        Harness::ClaudeCli
        | Harness::OpenCodeCli
        | Harness::MuseCli
        | Harness::WarpCli
        | Harness::CursorCli => (Some(harness.id().to_string()), None),
        Harness::Claude
        | Harness::ClaudeAcp
        | Harness::Codex
        | Harness::CodexAcp
        | Harness::OpenCode
        | Harness::Cursor => (None, Some(harness.id().to_string())),
    };
    Ok(ProfileStart {
        session_type,
        agent,
        choice: RouteChoice::Account(profile.provider_account.clone()),
        model: profile.model.clone(),
    })
}

/// The id of the agent spec `profile`'s harness launches from (`#agent-tools 8/12`): its CLI
/// session type through `tddy_coding_agent_specs::spec_for_session_type` (`opencode-cli` →
/// `opencode`); `None` for a profile that starts a tool session, or names no harness.
pub fn profile_spec_id(profile: &HarnessProfile) -> Option<&'static str> {
    // TODO(harness-clis): implement — `profile_start(profile)`'s `session_type`, resolved through
    // `spec_for_session_type`; the CLI start then launches from that spec.
    let _ = profile;
    todo!("harness-clis: profile_spec_id")
}

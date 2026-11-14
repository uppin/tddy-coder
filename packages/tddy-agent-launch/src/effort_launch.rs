//! How a session's reasoning effort reaches its agent's process (`#orchestrators` 3/11).
//!
//! The agent's spec declares the carry (`coding_agent.effort`: a `flag` or an `env` variable), so
//! nothing here names an agent: a chosen level becomes the flag and the level, or the variable set
//! to the level, and joins the launch beside the other slot flags and the session's environment.

use tddy_coding_agent_specs::{CodingAgentSpec, EffortCarry};

/// What a chosen effort adds to a launch.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct EffortLaunch {
    /// Arguments appended to the spec's slot flags.
    pub args: Vec<String>,
    /// Environment variables set for the agent's process.
    pub env: Vec<(String, String)>,
}

/// The launch additions that carry `effort` for `spec`: nothing for a blank effort, and nothing for
/// an agent whose spec declares no effort slot (a start naming one there is refused before it gets
/// here; a resume of a session recorded under an older spec simply runs without it).
pub(crate) fn effort_launch(spec: &CodingAgentSpec, effort: &str) -> EffortLaunch {
    let level = effort.trim();
    let Some(slot) = spec.effort().filter(|_| !level.is_empty()) else {
        return EffortLaunch::default();
    };
    match &slot.carry {
        EffortCarry::Flag(flag) => EffortLaunch {
            args: vec![flag.clone(), level.to_string()],
            env: Vec::new(),
        },
        EffortCarry::Env(variable) => EffortLaunch {
            args: Vec::new(),
            env: vec![(variable.clone(), level.to_string())],
        },
    }
}

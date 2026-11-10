use crate::route::RoutedAgent;
use crate::route::{ProviderKind, RouteShape};
use crate::spec::admits_a_tddy_variable;
use crate::spec::CodingAgentSpec;
use serde::{Deserialize, Serialize};
use thiserror::Error;

impl CodingAgentSpec {
    /// `Ok` when the agent's `placements` include `placement`; otherwise the refusal, naming the
    /// agent and every placement it declares — a sandboxed `warp-cli` start is refused with it
    /// (`#agent-tools 8/12`).
    pub fn require_placement(&self, placement: Placement) -> Result<(), PlacementError> {
        if self.agent.placements.contains(&placement) {
            return Ok(());
        }
        Err(PlacementError::NotPlaced {
            tool: self.id.clone(),
            placement,
            placements: self.agent.placements.clone(),
        })
    }
}

/// `coding_agent.route`: what a provider route needs of the agent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteSlot {
    /// A jailed launch without a provider route is refused: the jail holds no credentials of the
    /// agent's own (OpenCode, Muse).
    #[serde(default)]
    pub required_in_jail: bool,
    /// The variable the route's configuration reaches the agent in: for `shape: inline_config` the
    /// configuration itself (`OPENCODE_CONFIG_CONTENT`), for `shape: json_config_file` the path of
    /// the route's first file. `None` for a route that is environment only. An `inline_config` route
    /// needs one, never a `TDDY_*` one, and takes no `files_dir` ([`route_slot_errors`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_env: Option<String>,
    /// The directory under the jail's scratch the route's files are written to owner-only
    /// (`provider-route`); `None` for a route with no files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files_dir: Option<String>,
    /// The variable the route's key is handed in (`OPENCODE_PROVIDER_KEY`, `META_API_KEY`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_env: Option<String>,
    /// The providers a route can point the agent at (`#agent-tools 12/12`); empty for an agent that
    /// always runs on its own vendor. Read through [`CodingAgentSpec::route_support`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<ProviderKind>,
    /// How the route reaches the agent (`#agent-tools 12/12`): environment (the default),
    /// `-c` TOML arguments (`toml_args`), or a JSON configuration file (`json_config_file`).
    #[serde(default)]
    pub shape: RouteShape,
}

/// `coding_agent.resume`: the launch adapter (`tddy_agent_backend::adapters::LaunchAdapters`) that
/// reads a resume token off an exited session's terminal — `warp-resume-token`. A name the registry
/// does not hold is refused at load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeSlot {
    pub adapter: String,
}

/// Where an agent may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    /// A PTY on the daemon's host, as the session's OS user.
    Host,
    /// Inside a session jail, through `tddy-sandbox-runner`.
    Jail,
}

impl Placement {
    /// The placements of a spec that declares none: both.
    pub fn anywhere() -> Vec<Placement> {
        vec![Placement::Host, Placement::Jail]
    }

    /// The word a spec writes it as.
    pub fn as_str(self) -> &'static str {
        match self {
            Placement::Host => "host",
            Placement::Jail => "jail",
        }
    }
}

impl std::fmt::Display for Placement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why an agent cannot run where a launch asked.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlacementError {
    /// The spec's `placements` exclude the asked one.
    #[error(
        "{tool} cannot run in the {placement}: its spec's placements are [{}]",
        placements.iter().map(|p| p.as_str()).collect::<Vec<_>>().join(", ")
    )]
    NotPlaced {
        tool: String,
        placement: Placement,
        placements: Vec<Placement>,
    },
}

/// A `coding_agent.route` slot refused at load because its shape cannot be rendered as declared:
/// `shape: inline_config` without a `config_env` to carry the configuration in, with a
/// `config_env` matching `TDDY_*` (an agent never sees a daemon variable), or with a `files_dir`
/// (an inline route writes no file). The message names the field.
pub(crate) fn route_slot_errors(route: &RouteSlot) -> Result<(), String> {
    if route.shape != RouteShape::InlineConfig {
        return Ok(());
    }
    let Some(config_env) = route.config_env.as_deref() else {
        return Err(
            "shape `inline_config` needs a `config_env` naming the variable that carries the \
             route's configuration inline"
                .to_string(),
        );
    };
    if admits_a_tddy_variable(config_env) {
        return Err(format!(
            "config_env '{config_env}' would hand the route's configuration over in a TDDY_* \
             variable, which no agent sees"
        ));
    }
    if route.files_dir.is_some() {
        return Err(
            "shape `inline_config` takes no `files_dir`: an inline route writes no file"
                .to_string(),
        );
    }
    Ok(())
}

/// What a route body knows of `spec` (`#agent-tools 13/19`): its id, its product, its route
/// support, its `route.key_env` and its `route.config_env`. A spec without a `route` section gives an agent with no
/// providers, which every route body refuses.
pub fn routed_agent(spec: &CodingAgentSpec) -> RoutedAgent {
    RoutedAgent {
        id: spec.id.clone(),
        product: spec.product().to_string(),
        support: spec.route_support(),
        key_env: spec
            .agent
            .route
            .as_ref()
            .and_then(|route| route.key_env.clone()),
        config_env: spec
            .agent
            .route
            .as_ref()
            .and_then(|route| route.config_env.clone()),
    }
}

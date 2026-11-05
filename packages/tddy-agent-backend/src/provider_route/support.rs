//! `coding_agent.route.{providers, shape}`: which model providers an agent can be routed to, and
//! how a route reaches it — and what a route body knows of one agent (`#agent-tools 13/19`).
//!
//! Feature doc: `docs/ft/daemon/provider-accounts.md`. The types live here, beside the route bodies
//! that read them, and `tddy-coding-agent-specs` re-exports them under the paths it published them
//! at (`#agent-tools 12/12`): the specs crate depends on this one, so the reverse edge would be a
//! cycle.
//!
//! A route body is chosen by the spec's (shape, provider served), never by the agent's name:
//! `(env, anthropic)` is Anthropic Messages in the environment, `(env, meta)` the key alone in the
//! spec's `route.key_env`, `(toml_args, openai_responses)` `-c key=value` TOML overrides, and
//! `(json_config_file, openai_responses | openai_chat)` a JSON configuration file. An agent that
//! declares no provider cannot be routed at all.

use serde::{Deserialize, Serialize};

/// A kind of model-provider endpoint a route can point an agent at. Written in YAML as its
/// snake_case word, and on the wire (`AgentInfo.route_providers`) the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// An endpoint serving Anthropic Messages.
    Anthropic,
    /// An endpoint serving OpenAI Responses.
    OpenaiResponses,
    /// An endpoint serving OpenAI Chat Completions.
    OpenaiChat,
    /// Meta's own endpoint (the `meta` preset), whatever protocol it serves.
    Meta,
}

impl ProviderKind {
    /// The word a spec, and the agent catalog, write it as.
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::OpenaiResponses => "openai_responses",
            ProviderKind::OpenaiChat => "openai_chat",
            ProviderKind::Meta => "meta",
        }
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a provider route reaches the agent process, named after the body format it is — never
/// after an agent. `Default` is a parse default only: a spec without a `route` section is not
/// routable, whatever this says.
///
/// A spec still writing a removed word (`args`, `config_file`) is refused at load naming its
/// replacement (`route.shape `args` was renamed to `toml_args``).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteShape {
    /// Environment variables only (Anthropic Messages' base URL and key, or a key alone).
    #[default]
    Env,
    /// `-c key=value` TOML overrides on the command line (was `args`).
    TomlArgs,
    /// A JSON configuration file named in the environment (was `config_file`).
    JsonConfigFile,
}

impl<'de> Deserialize<'de> for RouteShape {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let word = String::deserialize(deserializer)?;
        match word.as_str() {
            "env" => Ok(RouteShape::Env),
            "toml_args" => Ok(RouteShape::TomlArgs),
            "json_config_file" => Ok(RouteShape::JsonConfigFile),
            "args" => Err(serde::de::Error::custom(
                "route.shape `args` was renamed to `toml_args`",
            )),
            "config_file" => Err(serde::de::Error::custom(
                "route.shape `config_file` was renamed to `json_config_file`",
            )),
            other => Err(serde::de::Error::custom(format!(
                "unknown route.shape `{other}`; expected `env`, `toml_args` or `json_config_file`"
            ))),
        }
    }
}

/// What a route can do with one agent: the providers it can point the agent at, and the shape it
/// takes. No providers means the agent cannot be routed — it always runs on its own vendor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouteSupport {
    pub providers: Vec<ProviderKind>,
    pub shape: RouteShape,
}

impl RouteSupport {
    /// Whether any provider route can point the agent elsewhere.
    pub fn is_routable(&self) -> bool {
        !self.providers.is_empty()
    }
}

/// What a route body knows about one agent, built from its spec
/// (`tddy_coding_agent_specs::routed_agent`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedAgent {
    /// The agent id, named in every refusal.
    pub id: String,
    /// The operator-facing name refusals lead with: `coding_agent.product`, or the id.
    pub product: String,
    /// `coding_agent.route.{providers, shape}`; no providers for a spec without `route`.
    pub support: RouteSupport,
    /// `coding_agent.route.key_env`: the variable a key-only or file route hands the key in.
    pub key_env: Option<String>,
}

/// The providers a route body exists for in `shape` — what the drift test pins every built-in
/// spec's `route.providers` against.
pub fn bodies_accept(shape: RouteShape) -> &'static [ProviderKind] {
    match shape {
        RouteShape::Env => &[ProviderKind::Anthropic, ProviderKind::Meta],
        RouteShape::TomlArgs => &[ProviderKind::OpenaiResponses],
        RouteShape::JsonConfigFile => &[ProviderKind::OpenaiResponses, ProviderKind::OpenaiChat],
    }
}

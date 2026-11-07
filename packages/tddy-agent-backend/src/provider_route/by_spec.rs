//! Route bodies chosen by an agent's spec (`#agent-tools 13/19`): the (shape, provider served) of
//! a [`RoutedAgent`] picks the body, and every refusal names the agent's product — its spec's
//! `coding_agent.product`, or its id when the spec declares none.
//!
//! Feature doc: `docs/ft/daemon/provider-accounts.md`. Re-exported at `provider_route`.
//!
//! **No fallback**: a spec without `route` is [`RouteRefusal::AgentCannotBeRouted`], never routed
//! as `env` + Anthropic, and a declared pair no body serves is [`RouteRefusal::NotYetRoutable`],
//! never a run on the vendor.

use super::codex::toml_args_route;
use super::support::{bodies_accept, ProviderKind, RouteShape, RoutedAgent};
use super::{
    openai_wire, opencode_config, HarnessRoute, RouteEndpoints, RouteTarget,
    OPENCODE_ROUTE_PROVIDER_ID,
};

/// Why an agent cannot run on an account. Each carries the agent id, names the agent's product
/// (the same text a `Harness` refusal had), and says what the operator would have to change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteRefusal {
    /// The agent declares no provider: it always runs on its own vendor.
    AgentCannotBeRouted { agent: String, product: String },
    /// The account's endpoint does not serve the protocol the agent's route needs.
    ProtocolNotServed {
        agent: String,
        product: String,
        needs: &'static str,
    },
    /// The agent accepts only some providers, and this preset is not one of them (Meta only).
    PresetNotAccepted {
        agent: String,
        product: String,
        preset: String,
    },
    /// The spec declares a (shape, provider) pair no route body serves.
    NotYetRoutable { agent: String, product: String },
}

impl std::fmt::Display for RouteRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteRefusal::AgentCannotBeRouted { product, .. } => write!(
                f,
                "{product} cannot be pointed at another provider; it always runs on its \
                 own vendor"
            ),
            RouteRefusal::ProtocolNotServed { product, needs, .. } => write!(
                f,
                "{product} needs an account whose endpoint serves {needs}, and this one \
                 does not"
            ),
            RouteRefusal::PresetNotAccepted {
                product, preset, ..
            } => write!(
                f,
                "{product} does not accept a {preset} account; it runs on Meta only"
            ),
            RouteRefusal::NotYetRoutable { agent, product } => write!(
                f,
                "{product} ({agent}) cannot run on a provider account in this build yet"
            ),
        }
    }
}

impl std::error::Error for RouteRefusal {}

const ANTHROPIC_MESSAGES: &str = "anthropic-messages";
const OPENAI_RESPONSES: &str = "openai-responses";
const OPENAI_CHAT: &str = "openai-chat";
const META_PRESET: &str = "meta";

/// The wire protocol a provider is reached over; `None` for Meta, which is a preset.
fn protocol_of(provider: ProviderKind) -> Option<&'static str> {
    match provider {
        ProviderKind::Anthropic => Some(ANTHROPIC_MESSAGES),
        ProviderKind::OpenaiResponses => Some(OPENAI_RESPONSES),
        ProviderKind::OpenaiChat => Some(OPENAI_CHAT),
        ProviderKind::Meta => None,
    }
}

/// Whether an account of `preset` serving `protocols` is one `provider` points at.
fn account_serves(provider: ProviderKind, preset: &str, protocols: &[&str]) -> bool {
    match protocol_of(provider) {
        Some(protocol) => protocols.contains(&protocol),
        None => preset == META_PRESET,
    }
}

fn cannot_be_routed(agent: &RoutedAgent) -> RouteRefusal {
    RouteRefusal::AgentCannotBeRouted {
        agent: agent.id.clone(),
        product: agent.product.clone(),
    }
}

pub(super) fn not_yet_routable(agent: &RoutedAgent) -> RouteRefusal {
    RouteRefusal::NotYetRoutable {
        agent: agent.id.clone(),
        product: agent.product.clone(),
    }
}

/// Whether `agent` can run on an account of `preset` serving `protocols` (wire spellings:
/// `"anthropic-messages"`, `"openai-responses"`, `"openai-chat"`), as its declared providers say,
/// without building a route.
pub fn route_supported(
    agent: &RoutedAgent,
    preset: &str,
    protocols: &[&str],
) -> Result<(), RouteRefusal> {
    supported_among(agent, &agent.support.providers, preset, protocols)
}

/// [`route_supported`] over `providers`, a subset of the agent's declared ones.
fn supported_among(
    agent: &RoutedAgent,
    providers: &[ProviderKind],
    preset: &str,
    protocols: &[&str],
) -> Result<(), RouteRefusal> {
    if agent.support.providers.is_empty() {
        return Err(cannot_be_routed(agent));
    }
    if providers
        .iter()
        .any(|provider| account_serves(*provider, preset, protocols))
    {
        return Ok(());
    }
    let needs = protocols_needed(providers);
    if needs.is_empty() {
        return Err(RouteRefusal::PresetNotAccepted {
            agent: agent.id.clone(),
            product: agent.product.clone(),
            preset: preset.to_string(),
        });
    }
    Err(RouteRefusal::ProtocolNotServed {
        agent: agent.id.clone(),
        product: agent.product.clone(),
        needs: needs_display_name(&needs),
    })
}

/// The protocol providers among `providers`, in order — what an account has to serve one of.
fn protocols_needed(providers: &[ProviderKind]) -> Vec<ProviderKind> {
    providers
        .iter()
        .copied()
        .filter(|provider| protocol_of(*provider).is_some())
        .collect()
}

/// The protocols an account has to serve one of, spelled for an operator. Closed over the lists a
/// body table entry can produce.
fn needs_display_name(needs: &[ProviderKind]) -> &'static str {
    match needs {
        [ProviderKind::Anthropic] => "Anthropic Messages",
        [ProviderKind::OpenaiResponses] => "OpenAI Responses",
        [ProviderKind::OpenaiChat] => "OpenAI Chat Completions",
        _ => "OpenAI Responses or OpenAI Chat Completions",
    }
}

/// The declared providers a route body exists for and can run — a key-carrying body needs a
/// `route.key_env` to hand the key in.
fn served_providers(agent: &RoutedAgent) -> Vec<ProviderKind> {
    agent
        .support
        .providers
        .iter()
        .copied()
        .filter(|provider| bodies_accept(agent.support.shape).contains(provider))
        .filter(|provider| body_has_its_inputs(agent, *provider))
        .collect()
}

fn body_has_its_inputs(agent: &RoutedAgent, provider: ProviderKind) -> bool {
    match (agent.support.shape, provider) {
        (RouteShape::Env, ProviderKind::Meta)
        | (RouteShape::TomlArgs, _)
        | (RouteShape::JsonConfigFile, _) => agent.key_env.is_some(),
        // TODO(opencode-route-inline): implement — an inline body also needs `config_env`.
        (RouteShape::InlineConfig, _) => agent.key_env.is_some(),
        _ => true,
    }
}

/// Whether this build can run `agent` on any provider account at all: `Err` for an agent that
/// declares no provider, and for a declared pair no body serves.
pub fn agent_routable(agent: &RoutedAgent) -> Result<(), RouteRefusal> {
    if agent.support.providers.is_empty() {
        return Err(cannot_be_routed(agent));
    }
    if served_providers(agent).is_empty() {
        return Err(not_yet_routable(agent));
    }
    Ok(())
}

/// What `agent` needs to run on `target`: the body its (shape, provider served) picks. The first
/// declared provider the account serves, among those a body exists for, picks it.
pub fn resolve_route(
    agent: &RoutedAgent,
    target: &RouteTarget,
) -> Result<HarnessRoute, RouteRefusal> {
    agent_routable(agent)?;
    let served = served_providers(agent);
    let protocols = served_protocols(&target.endpoints);
    supported_among(agent, &served, &target.preset, &protocols)?;
    let provider = served
        .iter()
        .copied()
        .find(|provider| account_serves(*provider, &target.preset, &protocols))
        .ok_or_else(|| not_yet_routable(agent))?;
    match (agent.support.shape, provider) {
        (RouteShape::Env, ProviderKind::Anthropic) => anthropic_env_route(agent, target),
        (RouteShape::Env, ProviderKind::Meta) => key_env_route(agent, target),
        (RouteShape::TomlArgs, _) => toml_args_route(agent, target),
        (RouteShape::JsonConfigFile, _) => json_config_file_route(agent, target),
        (RouteShape::InlineConfig, _) => inline_config_route(agent, target),
        _ => Err(not_yet_routable(agent)),
    }
}

/// The wire spellings of the protocols `endpoints` serves, as [`route_supported`] takes them.
pub(super) fn served_protocols(endpoints: &RouteEndpoints) -> Vec<&'static str> {
    [
        endpoints
            .anthropic_messages
            .as_ref()
            .map(|_| ANTHROPIC_MESSAGES),
        endpoints
            .openai_responses
            .as_ref()
            .map(|_| OPENAI_RESPONSES),
        endpoints.openai_chat.as_ref().map(|_| OPENAI_CHAT),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// An agent that reads its endpoint, key and every model choice from the environment (Claude
/// Code): the base URL is the account's Anthropic Messages root, and each alias a model choice
/// could fall through to is pinned to `target.model`, so no request reaches Anthropic.
fn anthropic_env_route(
    agent: &RoutedAgent,
    target: &RouteTarget,
) -> Result<HarnessRoute, RouteRefusal> {
    let Some(root) = target.endpoints.anthropic_messages.as_ref() else {
        return Err(RouteRefusal::ProtocolNotServed {
            agent: agent.id.clone(),
            product: agent.product.clone(),
            needs: "Anthropic Messages",
        });
    };
    let mut env = vec![("ANTHROPIC_BASE_URL".to_string(), root.clone())];
    env.extend(
        [
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "CLAUDE_CODE_SUBAGENT_MODEL",
        ]
        .map(|alias| (alias.to_string(), target.model.clone())),
    );
    Ok(HarnessRoute {
        env,
        secret_env: vec![("ANTHROPIC_AUTH_TOKEN".to_string(), target.api_key.clone())],
        files: Vec::new(),
        args: vec!["--model".to_string(), target.model.clone()],
    })
}

/// An agent that talks only to Meta (Muse Code): a route on a Meta account needs no endpoint, file
/// or argument — the account's key alone, as a secret variable in the spec's `route.key_env`. The
/// route names no model (Meta's model ids need no provider prefix), so a routed launch keeps the
/// session's own model in its `--model`.
fn key_env_route(agent: &RoutedAgent, target: &RouteTarget) -> Result<HarnessRoute, RouteRefusal> {
    let Some(key_env) = agent.key_env.as_ref() else {
        return Err(not_yet_routable(agent));
    };
    Ok(HarnessRoute {
        env: Vec::new(),
        secret_env: vec![(key_env.clone(), target.api_key.clone())],
        files: Vec::new(),
        args: Vec::new(),
    })
}

/// An agent that reads its provider from a JSON file the route names in its environment (OpenCode),
/// the file reading the key from the spec's `route.key_env` — so the key travels as a secret
/// variable and never as an argument. The model is selected on the provider the file declares, by
/// the `--model` argument.
fn json_config_file_route(
    agent: &RoutedAgent,
    target: &RouteTarget,
) -> Result<HarnessRoute, RouteRefusal> {
    let Some(key_env) = agent.key_env.as_ref() else {
        return Err(not_yet_routable(agent));
    };
    let Some(wire) = openai_wire(&target.endpoints) else {
        return Err(RouteRefusal::ProtocolNotServed {
            agent: agent.id.clone(),
            product: agent.product.clone(),
            needs: "OpenAI Responses or OpenAI Chat Completions",
        });
    };
    Ok(HarnessRoute {
        env: Vec::new(),
        secret_env: vec![(key_env.clone(), target.api_key.clone())],
        files: vec![opencode_config(&target.model, wire, key_env)],
        args: vec![
            "--model".to_string(),
            format!("{OPENCODE_ROUTE_PROVIDER_ID}/{}", target.model),
        ],
    })
}

/// An agent that reads its provider from JSON configuration carried inline in the variable its
/// spec's `route.config_env` names (OpenCode's `OPENCODE_CONFIG_CONTENT`): the configuration
/// [`json_config_file_route`] would write as a file is the value of that variable in the route's
/// plain environment, and **no file** is written. The configuration reads the key from the spec's
/// `route.key_env`, so the key travels as a secret variable and is never in the configuration. The
/// model is selected on the provider the configuration declares, by the `--model` argument.
///
/// Inline configuration outranks a worktree's own config file, where a file named in the
/// environment ranks below it — so a repository cannot redirect the route's endpoint.
pub(super) fn inline_config_route(
    agent: &RoutedAgent,
    target: &RouteTarget,
) -> Result<HarnessRoute, RouteRefusal> {
    // TODO(opencode-route-inline): implement
    let _ = target;
    Err(not_yet_routable(agent))
}

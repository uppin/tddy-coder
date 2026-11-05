//! Route bodies chosen by an agent's spec (`#agent-tools 13/19`): the (shape, provider served) of
//! a [`RoutedAgent`] picks the body, and every refusal names the agent's product — its spec's
//! `coding_agent.product`, or its id when the spec declares none.
//!
//! Feature doc: `docs/ft/daemon/provider-accounts.md`. This is the replacement of the
//! [`super::Harness`]-keyed entry points beside it; once every caller has moved here those, and
//! `Harness`, are deleted and these take their names at `provider_route`.
//!
//! **No fallback**: a spec without `route` is [`RouteRefusal::AgentCannotBeRouted`], never routed
//! as `env` + Anthropic, and a declared pair no body serves is [`RouteRefusal::NotYetRoutable`],
//! never a run on the vendor.

use super::support::RoutedAgent;
use super::{HarnessRoute, RouteTarget};

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

/// Whether `agent` can run on an account of `preset` serving `protocols` (wire spellings:
/// `"anthropic-messages"`, `"openai-responses"`, `"openai-chat"`), as its declared providers say,
/// without building a route.
pub fn route_supported(
    agent: &RoutedAgent,
    preset: &str,
    protocols: &[&str],
) -> Result<(), RouteRefusal> {
    // TODO(route-support): implement — provider ↔ protocol (anthropic ↔ anthropic-messages,
    // openai_responses ↔ openai-responses, openai_chat ↔ openai-chat), meta ↔ preset `meta`.
    let _ = (agent, preset, protocols);
    todo!("route-support: by_spec::route_supported")
}

/// Whether this build can run `agent` on any provider account at all: `Err` for an agent that
/// declares no provider, and for a declared pair no body serves.
pub fn agent_routable(agent: &RoutedAgent) -> Result<(), RouteRefusal> {
    // TODO(route-support): implement.
    let _ = agent;
    todo!("route-support: by_spec::agent_routable")
}

/// What `agent` needs to run on `target`: the body its (shape, provider served) picks.
pub fn resolve_route(
    agent: &RoutedAgent,
    target: &RouteTarget,
) -> Result<HarnessRoute, RouteRefusal> {
    // TODO(route-support): implement — (env, anthropic) → anthropic_env_route, (env, meta) →
    // key_env_route writing `agent.key_env`, (toml_args, openai_responses) → toml_args_route,
    // (json_config_file, openai_responses | openai_chat) → json_config_file_route.
    let _ = (agent, target);
    todo!("route-support: by_spec::resolve_route")
}

//! Agents as their specs declare them, for the route tests that build no spec: a route body is
//! chosen by this data (shape, providers, key variable), never by the agent's name.

#![allow(dead_code)]

use tddy_agent_backend::provider_route::{ProviderKind, RouteShape, RouteSupport, RoutedAgent};

/// An agent as its spec declares it: the route bodies are chosen by this data, never by the name.
pub fn an_agent(
    id: &str,
    product: &str,
    shape: RouteShape,
    providers: &[ProviderKind],
    key_env: Option<&str>,
) -> RoutedAgent {
    RoutedAgent {
        id: id.to_string(),
        product: product.to_string(),
        support: RouteSupport {
            providers: providers.to_vec(),
            shape,
        },
        key_env: key_env.map(str::to_string),
    }
}

/// Claude Code under `id` (the CLI, the headless and the ACP spec declare the same route).
pub fn claude_named(id: &str) -> RoutedAgent {
    an_agent(
        id,
        "Claude Code",
        RouteShape::Env,
        &[ProviderKind::Anthropic],
        None,
    )
}

/// Cursor under `id`: no route section, so no providers.
pub fn cursor_named(id: &str) -> RoutedAgent {
    an_agent(id, "Cursor", RouteShape::Env, &[], None)
}

pub fn codex_named(id: &str) -> RoutedAgent {
    an_agent(
        id,
        "Codex",
        RouteShape::TomlArgs,
        &[ProviderKind::OpenaiResponses],
        None,
    )
}

pub fn opencode_named(id: &str) -> RoutedAgent {
    an_agent(
        id,
        "OpenCode",
        RouteShape::JsonConfigFile,
        &[ProviderKind::OpenaiResponses, ProviderKind::OpenaiChat],
        Some("OPENCODE_PROVIDER_KEY"),
    )
}

pub fn muse_named(id: &str) -> RoutedAgent {
    an_agent(
        id,
        "Muse Code",
        RouteShape::Env,
        &[ProviderKind::Meta],
        Some("META_API_KEY"),
    )
}

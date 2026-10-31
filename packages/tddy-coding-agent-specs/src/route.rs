//! `coding_agent.route.{providers, shape}`: which model providers an agent can be routed to, and
//! how a route reaches it (`#agent-tools 12/12`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`. This is the data that
//! replaces `tddy_agent_backend::provider_route::Harness`'s matrix: Claude Code takes Anthropic
//! Messages through its environment, Codex OpenAI Responses through its arguments, OpenCode
//! Responses or Chat Completions through a configuration file, Muse Code Meta only, and an agent
//! that declares no provider (Cursor, Warp) cannot be routed at all.

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

/// How a provider route reaches the agent process.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RouteShape {
    /// Environment variables only (Claude Code's base URL and key, Muse Code's key).
    #[default]
    Env,
    /// Extra arguments (Codex's `-c model_provider=…`).
    Args,
    /// A configuration file named in the environment (OpenCode's `OPENCODE_CONFIG`).
    ConfigFile,
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

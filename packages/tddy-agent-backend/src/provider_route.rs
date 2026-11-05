//! Provider routes: what a harness process needs to run on a model-provider account instead of its
//! vendor.
//!
//! A route is pure data — environment, secret environment, files and extra arguments — computed from
//! a harness, an account's endpoints and key, and a model. Where it is applied (a PTY's env, a jail's
//! secret files, a tool session's route file) is the caller's concern; this module only decides
//! **what** each harness is given, and **whether** a harness can run on an account at all.
//!
//! The route is chosen by the agent's spec, never by its name (`#agent-tools 13/19`): the spec's
//! (shape, provider served) picks the body, a spec that declares no provider cannot be routed, and
//! a declared pair no body serves answers [`RouteRefusal::NotYetRoutable`] — a refusal, never a
//! silent run on the vendor. See [`by_spec`].

mod by_spec;
mod codex;
mod support;
pub use by_spec::{agent_routable, resolve_route, route_supported, RouteRefusal};
pub use codex::{CODEX_ROUTE_KEY_ENV, CODEX_ROUTE_PROVIDER_ID};
pub use support::{bodies_accept, ProviderKind, RouteShape, RouteSupport, RoutedAgent};

/// A secret route value. Its `Debug` never shows the value, so a route can be logged.
#[derive(Clone, PartialEq, Eq)]
pub struct RouteSecret(String);

impl RouteSecret {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value, for the one place that hands it to a child process.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for RouteSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RouteSecret(<redacted>)")
    }
}

/// The roots an account's endpoint serves, per wire protocol.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouteEndpoints {
    /// Anthropic Messages root; the client appends `/v1/messages`.
    pub anthropic_messages: Option<String>,
    /// OpenAI root `/responses` is appended to.
    pub openai_responses: Option<String>,
    /// OpenAI root `/chat/completions` is appended to.
    pub openai_chat: Option<String>,
}

/// One account and model a harness is to run on.
#[derive(Debug, Clone)]
pub struct RouteTarget {
    /// The account's preset id — `"meta"`, `"fireworks"`, …
    pub preset: String,
    pub endpoints: RouteEndpoints,
    pub api_key: RouteSecret,
    /// The model every model choice the harness makes is pinned to.
    pub model: String,
}

/// A file a route needs beside the session, written owner-only in the session directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteFile {
    /// File name within the session directory.
    pub name: String,
    pub contents: String,
}

/// What a harness process is given to run on an account.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HarnessRoute {
    /// Non-secret environment.
    pub env: Vec<(String, String)>,
    /// Environment that carries a credential: never logged, never on argv, delivered to the agent
    /// child only.
    pub secret_env: Vec<(String, RouteSecret)>,
    /// Files the harness reads, written in the session directory.
    pub files: Vec<RouteFile>,
    /// Extra command-line arguments.
    pub args: Vec<String>,
}

/// One level's choice of where a session runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteChoice {
    /// Explicitly the harness's own vendor ("Anthropic (built in)") — overrides lower levels.
    BuiltIn,
    /// The account with this id.
    Account(String),
}

/// The levels a session's route can be chosen at, highest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouteSources {
    /// The operator's choice for this one session.
    pub session: Option<RouteChoice>,
    /// The agent definition (harness profile) the session was started with.
    pub agent_definition: Option<RouteChoice>,
    /// The person's default account for this harness.
    pub user_default: Option<String>,
}

/// The account a session runs on — session over agent definition over the person's default — or
/// `None` for the harness's own vendor.
#[must_use]
pub fn choose_route(sources: &RouteSources) -> Option<String> {
    match sources
        .session
        .as_ref()
        .or(sources.agent_definition.as_ref())
    {
        Some(RouteChoice::Account(id)) => Some(id.clone()),
        Some(RouteChoice::BuiltIn) => None,
        None => sources.user_default.clone(),
    }
}

/// The provider id an OpenCode route's generated configuration declares the account under, so the
/// person's own `opencode.json` providers are never shadowed.
pub const OPENCODE_ROUTE_PROVIDER_ID: &str = "tddy-route";

/// The OpenAI protocol an OpenCode route speaks, with the account's root for it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum OpenAiWire<'a> {
    /// Responses, preferred: reasoning is carried across turns.
    Responses(&'a str),
    /// Chat Completions, for an account that serves no Responses.
    Chat(&'a str),
}

/// The wire an account's endpoints give OpenCode, Responses over Chat Completions; `None` when it
/// serves neither.
pub(crate) fn openai_wire(endpoints: &RouteEndpoints) -> Option<OpenAiWire<'_>> {
    match (&endpoints.openai_responses, &endpoints.openai_chat) {
        (Some(root), _) => Some(OpenAiWire::Responses(root)),
        (None, Some(root)) => Some(OpenAiWire::Chat(root)),
        (None, None) => None,
    }
}

/// The `opencode.json` that declares `target`'s account as an OpenCode provider and selects
/// `target.model` on it: `@ai-sdk/openai` over Responses (reasoning carried across turns) when the
/// account serves Responses, `@ai-sdk/openai-compatible` over Chat Completions otherwise. The key is
/// referenced as `{env:<key_env>}` (the spec's `route.key_env`) and never written into the file.
///
/// `None` when the account serves neither OpenAI protocol — there is no configuration OpenCode
/// could run on it; [`resolve_route`] refuses such an account as
/// [`RouteRefusal::ProtocolNotServed`].
#[must_use]
pub fn opencode_config_for(target: &RouteTarget, key_env: &str) -> Option<RouteFile> {
    openai_wire(&target.endpoints).map(|wire| opencode_config(&target.model, wire, key_env))
}

pub(crate) fn opencode_config(model: &str, wire: OpenAiWire<'_>, key_env: &str) -> RouteFile {
    let (npm, root, model_entry) = match wire {
        OpenAiWire::Responses(root) => (
            "@ai-sdk/openai",
            root,
            serde_json::json!({ "options": { "include": ["reasoning.encrypted_content"] } }),
        ),
        OpenAiWire::Chat(root) => ("@ai-sdk/openai-compatible", root, serde_json::json!({})),
    };
    let config = serde_json::json!({
        "model": format!("{OPENCODE_ROUTE_PROVIDER_ID}/{model}"),
        "provider": {
            (OPENCODE_ROUTE_PROVIDER_ID): {
                "npm": npm,
                "options": {
                    "baseURL": root,
                    "apiKey": format!("{{env:{key_env}}}"),
                },
                "models": { (model): model_entry },
            },
        },
    });
    RouteFile {
        name: "opencode.json".to_string(),
        contents: serde_json::to_string_pretty(&config)
            .expect("a serde_json::Value always serializes"),
    }
}

//! Provider routes: what a harness process needs to run on a model-provider account instead of its
//! vendor.
//!
//! A route is pure data — environment, secret environment, files and extra arguments — computed from
//! a harness, an account's endpoints and key, and a model. Where it is applied (a PTY's env, a jail's
//! secret files, a tool session's route file) is the caller's concern; this module only decides
//! **what** each harness is given, and **whether** a harness can run on an account at all.
//!
//! The capability matrix is here, once, for every harness: Claude Code needs Anthropic Messages,
//! Codex needs OpenAI Responses, OpenCode takes Responses or Chat Completions, Muse Code takes only
//! Meta, and Cursor's agent cannot be redirected. A harness whose route body has not been built yet
//! answers [`RouteRefusal::NotYetRoutable`] — a refusal, never a silent run on the vendor.

/// Every harness a session can run, by the id a session type or tool-session agent names it with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Harness {
    /// Headless `claude -p` (tool-session agent `claude`).
    Claude,
    /// `claude-agent-acp` (tool-session agent `claude-acp`).
    ClaudeAcp,
    /// Claude Code's TUI in a PTY (session type `claude-cli`).
    ClaudeCli,
    /// `codex exec` (tool-session agent `codex`).
    Codex,
    /// `codex-acp` (tool-session agent `codex-acp`).
    CodexAcp,
    /// `opencode acp` (tool-session agent `opencode`).
    OpenCode,
    /// OpenCode's TUI in a PTY (session type `opencode-cli`).
    OpenCodeCli,
    /// Muse Code's TUI in a PTY (session type `muse-cli`).
    MuseCli,
    /// Cursor's headless agent (tool-session agent `cursor`).
    Cursor,
    /// Cursor's agent in a PTY (session type `cursor-cli`).
    CursorCli,
}

impl Harness {
    /// The harness a tool-session agent id or a CLI session type names; `None` for anything else
    /// (`stub`, a specialized agent definition, `tool`).
    #[must_use]
    pub fn from_agent_or_session_type(id: &str) -> Option<Harness> {
        let _ = id;
        // TODO(claude-route): implement
        todo!()
    }

    /// The id this harness is named by — the inverse of [`Harness::from_agent_or_session_type`].
    #[must_use]
    pub fn id(self) -> &'static str {
        // TODO(claude-route): implement
        todo!()
    }
}

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

/// Why a harness cannot run on an account. Each says what the operator would have to change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteRefusal {
    /// The harness cannot be pointed at another provider at all (Cursor).
    HarnessCannotBeRouted { harness: Harness },
    /// The account's endpoint does not serve the protocol the harness speaks.
    ProtocolNotServed {
        harness: Harness,
        needs: &'static str,
    },
    /// The harness accepts only some providers, and this is not one of them (Muse Code: Meta only).
    PresetNotAccepted { harness: Harness, preset: String },
    /// The harness will be routable; this build does not route it yet.
    NotYetRoutable(Harness),
}

impl std::fmt::Display for RouteRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = f;
        // TODO(claude-route): implement
        todo!()
    }
}

/// Whether `harness` can run on an account of `preset` serving `protocols` (wire spellings:
/// `"anthropic-messages"`, `"openai-responses"`, `"openai-chat"`). Answers what the matrix says,
/// without building a route.
pub fn route_supported(
    harness: Harness,
    preset: &str,
    protocols: &[&str],
) -> Result<(), RouteRefusal> {
    let _ = (harness, preset, protocols);
    // TODO(claude-route): implement
    todo!()
}

/// What `harness` needs to run on `target`.
pub fn resolve_route(harness: Harness, target: &RouteTarget) -> Result<HarnessRoute, RouteRefusal> {
    let _ = (harness, target);
    // TODO(claude-route): implement
    todo!()
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
    let _ = sources;
    // TODO(claude-route): implement
    todo!()
}

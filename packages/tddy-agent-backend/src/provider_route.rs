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
        Harness::ALL.into_iter().find(|harness| harness.id() == id)
    }

    /// The id this harness is named by — the inverse of [`Harness::from_agent_or_session_type`].
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::ClaudeAcp => "claude-acp",
            Harness::ClaudeCli => "claude-cli",
            Harness::Codex => "codex",
            Harness::CodexAcp => "codex-acp",
            Harness::OpenCode => "opencode",
            Harness::OpenCodeCli => "opencode-cli",
            Harness::MuseCli => "muse-cli",
            Harness::Cursor => "cursor",
            Harness::CursorCli => "cursor-cli",
        }
    }

    const ALL: [Harness; 10] = [
        Harness::Claude,
        Harness::ClaudeAcp,
        Harness::ClaudeCli,
        Harness::Codex,
        Harness::CodexAcp,
        Harness::OpenCode,
        Harness::OpenCodeCli,
        Harness::MuseCli,
        Harness::Cursor,
        Harness::CursorCli,
    ];

    /// The harness's product name, for messages an operator reads.
    fn product(self) -> &'static str {
        match self {
            Harness::Claude | Harness::ClaudeAcp | Harness::ClaudeCli => "Claude Code",
            Harness::Codex | Harness::CodexAcp => "Codex",
            Harness::OpenCode | Harness::OpenCodeCli => "OpenCode",
            Harness::MuseCli => "Muse Code",
            Harness::Cursor | Harness::CursorCli => "Cursor",
        }
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
        match self {
            RouteRefusal::HarnessCannotBeRouted { harness } => write!(
                f,
                "{} cannot be pointed at another provider; it always runs on its own vendor",
                harness.product()
            ),
            RouteRefusal::ProtocolNotServed { harness, needs } => write!(
                f,
                "{} needs an account whose endpoint serves {needs}, and this one does not",
                harness.product()
            ),
            RouteRefusal::PresetNotAccepted { harness, preset } => write!(
                f,
                "{} does not accept a {preset} account; it runs on Meta only",
                harness.product()
            ),
            RouteRefusal::NotYetRoutable(harness) => write!(
                f,
                "{} ({}) cannot run on a provider account in this build yet",
                harness.product(),
                harness.id()
            ),
        }
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
    match harness {
        Harness::Claude | Harness::ClaudeAcp | Harness::ClaudeCli => {
            require_protocol(harness, protocols, &[ANTHROPIC_MESSAGES])
        }
        Harness::Codex | Harness::CodexAcp => {
            require_protocol(harness, protocols, &[OPENAI_RESPONSES])
        }
        Harness::OpenCode | Harness::OpenCodeCli => {
            require_protocol(harness, protocols, &[OPENAI_RESPONSES, OPENAI_CHAT])
        }
        Harness::MuseCli if preset == META_PRESET => Ok(()),
        Harness::MuseCli => Err(RouteRefusal::PresetNotAccepted {
            harness,
            preset: preset.to_string(),
        }),
        Harness::Cursor | Harness::CursorCli => {
            Err(RouteRefusal::HarnessCannotBeRouted { harness })
        }
    }
}

const ANTHROPIC_MESSAGES: &str = "anthropic-messages";
const OPENAI_RESPONSES: &str = "openai-responses";
const OPENAI_CHAT: &str = "openai-chat";
const META_PRESET: &str = "meta";

/// Ok when any of `accepted` is among `served`; otherwise names the first accepted protocol.
fn require_protocol(
    harness: Harness,
    served: &[&str],
    accepted: &[&'static str],
) -> Result<(), RouteRefusal> {
    if accepted.iter().any(|protocol| served.contains(protocol)) {
        return Ok(());
    }
    Err(RouteRefusal::ProtocolNotServed {
        harness,
        needs: protocol_display_name(accepted),
    })
}

fn protocol_display_name(accepted: &[&'static str]) -> &'static str {
    match accepted {
        [ANTHROPIC_MESSAGES] => "Anthropic Messages",
        [OPENAI_RESPONSES] => "OpenAI Responses",
        _ => "OpenAI Responses or OpenAI Chat Completions",
    }
}

/// What `harness` needs to run on `target`.
pub fn resolve_route(harness: Harness, target: &RouteTarget) -> Result<HarnessRoute, RouteRefusal> {
    match harness {
        Harness::Claude | Harness::ClaudeAcp | Harness::ClaudeCli => {
            claude_code_route(harness, target)
        }
        Harness::Cursor | Harness::CursorCli => {
            Err(RouteRefusal::HarnessCannotBeRouted { harness })
        }
        Harness::Codex
        | Harness::CodexAcp
        | Harness::OpenCode
        | Harness::OpenCodeCli
        | Harness::MuseCli => Err(RouteRefusal::NotYetRoutable(harness)),
    }
}

/// Claude Code reads its endpoint, key and every model choice from the environment: the base URL is
/// the account's Anthropic Messages root, and each alias a model choice could fall through to is
/// pinned to `target.model`, so no request reaches Anthropic.
fn claude_code_route(harness: Harness, target: &RouteTarget) -> Result<HarnessRoute, RouteRefusal> {
    let Some(root) = target.endpoints.anthropic_messages.as_ref() else {
        return Err(RouteRefusal::ProtocolNotServed {
            harness,
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

//! CLI harnesses: the coding agents a session runs interactively in a terminal pane.
//!
//! One descriptor per session type — `claude-cli`, `cursor-cli`, `opencode-cli` — says everything
//! the daemon needs that differs between them: which binary, the argument list for a start or a
//! resume, which harness a provider route is resolved for, whose context files the session is
//! served, where its model list comes from, and which environment variable names a generated
//! configuration file. Code that used to match on `"claude-cli" | "cursor-cli"` asks the registry
//! instead, so a further harness is one more descriptor rather than another arm in every match.

use crate::backend::BackendModels;
use crate::provider_route::Harness;

/// What one start or resume of a CLI harness is launched with.
#[derive(Debug, Clone, Default)]
pub struct CliLaunch<'a> {
    /// The binary to run.
    pub binary: &'a str,
    /// The model the operator chose; empty when none was (a routed launch passes the model in its
    /// route's arguments instead).
    pub model: &'a str,
    /// The session's id, for a harness that is told its session id (Claude Code).
    pub session_id: &'a str,
    /// The harness's own conversation id to continue (Cursor's chat id), when it has one.
    pub conversation_id: Option<&'a str>,
    /// The first turn, when the session starts with one.
    pub initial_prompt: Option<&'a str>,
    /// Claude Code's permission mode; ignored by harnesses without one.
    pub permission_mode: Option<&'a str>,
    /// Claude Code's `--dangerously-skip-permissions`; ignored by harnesses without one.
    pub dangerously_skip_permissions: bool,
    /// A resume of an existing session rather than its first start.
    pub resume: bool,
}

/// Where a CLI harness's model list comes from when no provider account is chosen.
#[derive(Debug, Clone, Copy)]
pub enum CliModelSource {
    /// A curated catalogue (Claude Code's aliases, Cursor's list).
    Curated(fn() -> BackendModels),
    /// Asked of the binary itself (`opencode models`), one `provider/model` per line.
    ListedByBinary(&'static [&'static str]),
}

/// One CLI harness.
#[derive(Debug)]
pub struct CliHarness {
    /// The session type that selects it.
    pub session_type: &'static str,
    /// What the session-type toggle calls it.
    pub label: &'static str,
    /// The harness a provider route is resolved for.
    pub route_harness: Harness,
    /// The agent whose context-file allow-list the session is served.
    pub context_agent: &'static str,
    /// The binary run when the daemon config and the environment name none.
    pub default_binary: &'static str,
    /// The environment variable that overrides the binary (`TDDY_CLAUDE_BINARY`, …).
    pub binary_env: &'static str,
    /// The environment variable a routed launch points at its generated configuration file
    /// (`OPENCODE_CONFIG`); `None` for a harness whose route is environment only.
    pub route_config_env: Option<&'static str>,
    /// The argument list, binary first.
    pub argv: fn(&CliLaunch<'_>) -> Vec<String>,
    pub models: CliModelSource,
}

/// Every CLI harness, in the order the session-type toggle offers them.
pub fn cli_harnesses() -> &'static [CliHarness] {
    // TODO(opencode-cli): implement
    todo!()
}

/// The CLI harness `session_type` selects; `None` for a tool or workspace session.
#[must_use]
pub fn cli_harness(session_type: &str) -> Option<&'static CliHarness> {
    let _ = session_type;
    // TODO(opencode-cli): implement
    todo!()
}

/// The catalogue a harness that lists its own models printed — one `provider/model` per line, as
/// `opencode models` does. Blank lines are skipped; the first model is the default.
#[must_use]
pub fn models_from_listing(stdout: &str) -> BackendModels {
    let _ = stdout;
    // TODO(opencode-cli): implement
    todo!()
}

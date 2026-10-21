//! CLI harnesses: the coding agents a session runs interactively in a terminal pane.
//!
//! One descriptor per session type — `claude-cli`, `cursor-cli`, `opencode-cli` — says everything
//! the daemon needs that differs between them: which binary, the argument list for a start or a
//! resume, which harness a provider route is resolved for, whose context files the session is
//! served, where its model list comes from, and which environment variable names a generated
//! configuration file. Code that used to match on `"claude-cli" | "cursor-cli"` asks the registry
//! instead, so a further harness is one more descriptor rather than another arm in every match.

use crate::backend::{claude_cli_models, cursor_cli_models, BackendModel, BackendModels};
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

/// The registry, in the order the session-type toggle offers the harnesses.
static CLI_HARNESSES: [CliHarness; 3] = [
    CliHarness {
        session_type: "claude-cli",
        label: "Claude CLI",
        route_harness: Harness::ClaudeCli,
        context_agent: "claude",
        default_binary: "claude",
        binary_env: "TDDY_CLAUDE_BINARY",
        route_config_env: None,
        argv: claude_cli_argv,
        models: CliModelSource::Curated(claude_cli_models),
    },
    CliHarness {
        session_type: "cursor-cli",
        label: "Cursor CLI",
        route_harness: Harness::CursorCli,
        context_agent: "cursor",
        default_binary: "agent",
        binary_env: "TDDY_CURSOR_AGENT",
        route_config_env: None,
        argv: cursor_cli_argv,
        models: CliModelSource::Curated(cursor_cli_models),
    },
    CliHarness {
        session_type: "opencode-cli",
        label: "OpenCode",
        route_harness: Harness::OpenCodeCli,
        context_agent: "opencode",
        default_binary: "opencode",
        binary_env: "TDDY_OPENCODE_CLI",
        route_config_env: Some("OPENCODE_CONFIG"),
        argv: opencode_cli_argv,
        models: CliModelSource::ListedByBinary(&["models"]),
    },
];

/// Every CLI harness, in the order the session-type toggle offers them.
pub fn cli_harnesses() -> &'static [CliHarness] {
    &CLI_HARNESSES
}

/// The CLI harness `session_type` selects; `None` for a tool or workspace session.
#[must_use]
pub fn cli_harness(session_type: &str) -> Option<&'static CliHarness> {
    cli_harnesses()
        .iter()
        .find(|harness| harness.session_type == session_type)
}

/// The catalogue a harness that lists its own models printed — one `provider/model` per line, as
/// `opencode models` does. Blank lines are skipped; the first model is the default.
#[must_use]
pub fn models_from_listing(stdout: &str) -> BackendModels {
    let models: Vec<BackendModel> = stdout
        .lines()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(|id| BackendModel::new(id, id))
        .collect();
    let default_model = models
        .first()
        .map(|model| model.id.clone())
        .unwrap_or_default();
    BackendModels {
        models,
        default_model,
    }
}

/// A value with its surrounding whitespace removed, or `None` when nothing but whitespace is left.
fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// `claude`: the argv the Claude Code CLI has always been started with — see
/// [`crate::claude_argv::build_claude_base_argv`]. A blank permission mode is `auto`, and a blank
/// first prompt is no prompt.
fn claude_cli_argv(launch: &CliLaunch<'_>) -> Vec<String> {
    let mode = non_blank(launch.permission_mode).unwrap_or("auto");
    let mut argv = crate::claude_argv::build_claude_base_argv(
        launch.binary,
        launch.model,
        launch.session_id,
        mode,
        launch.dangerously_skip_permissions,
        launch.resume,
    );
    if let Some(prompt) = non_blank(launch.initial_prompt) {
        argv.push(prompt.to_string());
    }
    argv
}

/// `agent`: `--resume <chat>` only for a non-blank chat id — `--resume` takes an optional argument,
/// so a bare flag would open an interactive picker — then `--model` when a model is named, then the
/// first prompt when it is not blank.
fn cursor_cli_argv(launch: &CliLaunch<'_>) -> Vec<String> {
    let mut argv = vec![launch.binary.to_string()];
    if let Some(chat_id) = non_blank(launch.conversation_id) {
        argv.push("--resume".to_string());
        argv.push(chat_id.to_string());
    }
    if !launch.model.is_empty() {
        argv.push("--model".to_string());
        argv.push(launch.model.to_string());
    }
    if let Some(prompt) = non_blank(launch.initial_prompt) {
        argv.push(prompt.to_string());
    }
    argv
}

/// `opencode`: a fresh start names its model when it has one and opens with its first prompt;
/// a resume continues the worktree's last session, which is this one (one session per worktree).
fn opencode_cli_argv(launch: &CliLaunch<'_>) -> Vec<String> {
    let mut argv = vec![launch.binary.to_string()];
    if launch.resume {
        argv.push("--continue".to_string());
        return argv;
    }
    if !launch.model.is_empty() {
        argv.push("--model".to_string());
        argv.push(launch.model.to_string());
    }
    if let Some(prompt) = non_blank(launch.initial_prompt) {
        argv.push("--prompt".to_string());
        argv.push(prompt.to_string());
    }
    argv
}

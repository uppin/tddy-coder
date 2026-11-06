//! The port a spec-driven headless launch is rendered through.
//!
//! `ClaudeCodeBackend` and `HeadlessDriver` live in this crate; the agent specs live in
//! `tddy-coding-agent-specs`, which depends on this crate. So the consumers here name a
//! [`HeadlessRenderer`] — what one headless turn of an agent is spawned with — and the specs crate
//! implements it from `coding_agent.headless` (`SpecHeadlessRenderer`). Nothing here names an agent.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::backend::SessionMode;
use crate::error::BackendError;

use super::StreamAdapter;

/// The MCP server one turn registers with its agent (`tddy-tools --mcp`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessMcpServer {
    /// The server's name in the registration (`tddy-tools`).
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    /// The server's `env` block; omitted from the registration when empty.
    pub env: BTreeMap<String, String>,
}

/// What one headless turn is launched with: the template's parameters and the slots' inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessTurn {
    pub prompt: String,
    pub model: Option<String>,
    /// The agent's own session: `Fresh` names a new one, `Resume` continues one; `None` names none
    /// (a conversation's first turn).
    pub session: Option<SessionMode>,
    /// The template's `permission_mode` (`plan`, `acceptEdits`); `None` leaves the agent's default.
    pub permission_mode: Option<String>,
    /// Passed with the spec's `append_system_prompt` flag; `None` adds no flag.
    pub append_system_prompt_file: Option<PathBuf>,
    /// Pre-approved tools, in order.
    pub allowed_tools: Vec<String>,
    /// Withdrawn tools, in order.
    pub denied_tools: Vec<String>,
    /// The MCP tool the agent asks before anything not pre-approved.
    pub permission_prompt_tool: Option<String>,
    /// The tool server to register; `None` registers none.
    pub mcp_server: Option<HeadlessMcpServer>,
    /// The agent's working directory.
    pub workdir: PathBuf,
    /// Where the turn's registration files (the MCP config) are written.
    pub scratch_dir: PathBuf,
    /// Agent-specific template parameters, bound as `{name}` (and testable by `When`): what one
    /// agent's argv needs and the generic fields above do not name — Cursor's `force`, Codex's
    /// `sandbox_mode`. Added by the 2026-10-31 amendment (#798's red phase).
    pub params: BTreeMap<String, String>,
    /// A provider route's argv (`-c model_provider=…`, `-m …`), bound as the list `route_args` for
    /// a `{ splice: route_args }` item. Empty when the turn runs on no route.
    pub route_args: Vec<String>,
}

/// One rendered headless turn: what to spawn, what to write first, and how to read it.
#[derive(Clone)]
pub struct HeadlessInvocation {
    /// The resolved binary (`{binary}`).
    pub program: PathBuf,
    /// The rendered argv after the program.
    pub argv: Vec<String>,
    /// The spec's `headless.env`, set on the child on top of what the consumer passes.
    pub env: Vec<(String, String)>,
    /// Files the slots render (the MCP config), written before the spawn.
    pub files: Vec<(PathBuf, Vec<u8>)>,
    /// The adapter the spec names, which reads the child's stdout.
    pub adapter: Arc<dyn StreamAdapter>,
    /// The spec's `headless.exit_ok_if_structured`, handed to [`StreamAdapter::exit_ok`].
    pub exit_ok_if_structured: bool,
}

impl fmt::Debug for HeadlessInvocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeadlessInvocation")
            .field("program", &self.program)
            .field("argv", &self.argv)
            .field("env", &self.env)
            .field(
                "files",
                &self.files.iter().map(|(path, _)| path).collect::<Vec<_>>(),
            )
            .field("adapter", &self.adapter.name())
            .field("exit_ok_if_structured", &self.exit_ok_if_structured)
            .finish()
    }
}

/// Renders an agent's headless turns from its declaration.
pub trait HeadlessRenderer: Send + Sync + fmt::Debug {
    /// The agent's id (`claude`).
    fn agent_id(&self) -> &str;

    /// The tool server this agent registers when its consumer names no other: its `tddy-tools` dep
    /// with `--mcp`. `None` when the agent has no such dep.
    fn tool_server(&self) -> Option<HeadlessMcpServer>;

    /// `turn` rendered: the agent's `coding_agent.headless.argv` with `turn`'s parameters and its
    /// slots, the files those slots write, and the adapter its output is read through.
    ///
    /// Binds `prompt`, `model`, `session_id`, `resume`, `permission_mode`, `workdir`, every entry of
    /// `params`, and the lists `slots` and `route_args`. The MCP slot is `headless.mcp` when the
    /// spec sets one, else the agent's `coding_agent.mcp`.
    fn render(&self, turn: &HeadlessTurn) -> Result<HeadlessInvocation, BackendError>;
}

/// The mode a rendered file is written with: an MCP registration can carry the agent's tool
/// credential, so only its owner may read it.
const OWNER_ONLY_FILE: u32 = 0o600;

/// Writes a turn's rendered `files` owner-only, creating their directories, before the agent that
/// reads them is spawned.
pub(crate) fn write_rendered_files(files: &[(PathBuf, Vec<u8>)]) -> Result<(), BackendError> {
    for (path, contents) in files {
        crate::atomic_file::write_atomic_with_mode(path, contents, OWNER_ONLY_FILE).map_err(
            |e| BackendError::InvocationFailed(format!("could not write {}: {e}", path.display())),
        )?;
    }
    Ok(())
}

/// A turn's working directory: `cwd` when the request names one, else the directory this process
/// runs in, which the agent inherits.
pub(crate) fn turn_workdir(cwd: Option<&Path>) -> Result<PathBuf, BackendError> {
    match cwd {
        Some(cwd) => Ok(cwd.to_path_buf()),
        None => std::env::current_dir().map_err(|e| {
            BackendError::InvocationFailed(format!("the working directory cannot be read: {e}"))
        }),
    }
}

// --- The protocol a workflow turn runs with (`#agent-tools 15/19`) -----------------------------
//
// What one headless agent's workflow turn needs beyond its argv, declared as the spec's
// `coding_agent.headless` fields and read by `backend::SpecHeadlessBackend`. The types live here,
// beside the port, and `tddy-coding-agent-specs` re-exports them for its `HeadlessSection`: the specs
// crate depends on this one, so the reverse edge would be a cycle. Each is named after what it
// does, never after an agent.

/// How a workflow turn hands the agent its system prompt (`headless.system_prompt`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPromptDelivery {
    /// As a file, through the spec's `append_system_prompt` slot (Claude's
    /// `--append-system-prompt-file`); an inline prompt is written to a temp file the turn removes.
    Flag,
    /// In front of the prompt, separated by a blank line (Cursor, Codex, Oz).
    #[default]
    Prepend,
}

/// The template params a workflow turn binds by its goal's hints (`headless.goal_params`). Bound in
/// this order, a later map overriding an earlier one's name: `always`, then `plan_mode` for a goal
/// in the agent's plan mode, then `read_only_plan` for one that is also read-only, then
/// `accept_edits` for a goal that accepts edits. A param named `permission_mode` fills the
/// template's `{permission_mode}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalParams {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub always: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plan_mode: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub read_only_plan: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub accept_edits: BTreeMap<String, String>,
}

impl GoalParams {
    /// The params a goal with `hints` binds.
    pub fn bind(&self, hints: &tddy_workflow::GoalHints) -> BTreeMap<String, String> {
        use tddy_workflow::PermissionHint;
        let read_only = hints.permission == PermissionHint::ReadOnly;
        let accepts_edits = hints.permission == PermissionHint::AcceptEdits;
        let layers = [
            (true, &self.always),
            (hints.agent_cli_plan_mode, &self.plan_mode),
            (hints.agent_cli_plan_mode && read_only, &self.read_only_plan),
            (accepts_edits, &self.accept_edits),
        ];
        let mut bound = BTreeMap::new();
        for (applies, params) in layers {
            if applies {
                bound.extend(params.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
        }
        bound
    }
}

/// Whether a provider route's argv replaces the request's model (`headless.route_model`), written
/// as `{ kind: kept }`, `{ kind: replaced }` or `{ kind: replaced_if_route_has, flag: --model }`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RouteModel {
    /// The turn keeps its model, routed or not.
    #[default]
    Kept,
    /// A routed turn names no model: the route's argv carries it (Codex's `-m`).
    Replaced,
    /// A routed turn names no model when the route's argv carries `flag` (Claude's `--model`).
    ReplacedIfRouteHas { flag: String },
}

impl RouteModel {
    /// The model a turn requesting `model` runs with, on a route whose argv is `route_args`
    /// (empty when the turn is not routed).
    pub fn model_for(&self, model: Option<String>, route_args: &[String]) -> Option<String> {
        match self {
            Self::Kept => model,
            Self::Replaced if route_args.is_empty() => model,
            Self::Replaced => None,
            Self::ReplacedIfRouteHas { flag } if route_args.iter().any(|arg| arg == flag) => None,
            Self::ReplacedIfRouteHas { .. } => model,
        }
    }
}

/// Who names a fresh session's id (`headless.fresh_session`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshSession {
    /// The caller's id is passed (`--session-id <id>`).
    Caller,
    /// The agent mints its own, which the turn reads off its stream.
    #[default]
    Agent,
}

/// Where a workflow turn's registration files are written (`headless.scratch`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScratchDir {
    /// The session directory, never the worktree: the registration can carry a credential.
    #[default]
    SessionDir,
    /// The working directory, for a slot that writes under it (Cursor's `.cursor/mcp.json`).
    Workdir,
}

/// Everything a workflow turn of one headless agent is run with beyond its argv.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeadlessProtocol {
    pub system_prompt: SystemPromptDelivery,
    pub goal_params: GoalParams,
    /// The approval tool (`mcp__tddy-tools__approval_prompt`) is bound when the tool server is.
    pub approval_tool: bool,
    /// The workflow registers the agent's tool server (`HeadlessRenderer::tool_server`).
    pub workflow_mcp: bool,
    pub route_model: RouteModel,
    pub fresh_session: FreshSession,
    pub scratch: ScratchDir,
}

/// The values a spec's `headless.env` may name: `{session_dir}` and `{self_exe}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvTemplate {
    /// The request's session directory; `None` refuses a value naming `{session_dir}`.
    pub session_dir: Option<PathBuf>,
    /// This process's own executable (Codex's `BROWSER` re-exec).
    pub self_exe: PathBuf,
}

/// `env` with `{session_dir}` and `{self_exe}` in its values replaced by `template`'s. Refused,
/// naming the variable, when a value names `{session_dir}` and the request has none — never set
/// with the placeholder left in.
pub fn render_env(
    env: &[(String, String)],
    template: &EnvTemplate,
) -> Result<Vec<(String, String)>, BackendError> {
    env.iter()
        .map(|(name, value)| {
            let value = if value.contains("{session_dir}") {
                let session_dir = template.session_dir.as_ref().ok_or_else(|| {
                    BackendError::InvocationFailed(format!(
                        "headless env {name} names {{session_dir}}, and the request has no session directory"
                    ))
                })?;
                value.replace("{session_dir}", &session_dir.to_string_lossy())
            } else {
                value.clone()
            };
            Ok((
                name.clone(),
                value.replace("{self_exe}", &template.self_exe.to_string_lossy()),
            ))
        })
        .collect()
}

/// How a spec lists its agent's models (`coding_agent.models.list_command` + `adapter`), rendered
/// with the spec-resolved binary.
#[derive(Clone)]
pub struct HeadlessModelListing {
    pub program: PathBuf,
    pub argv: Vec<String>,
    pub adapter: Arc<dyn super::ModelsAdapter>,
}

impl fmt::Debug for HeadlessModelListing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeadlessModelListing")
            .field("program", &self.program)
            .field("argv", &self.argv)
            .field("adapter", &self.adapter.name())
            .finish()
    }
}

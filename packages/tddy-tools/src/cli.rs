//! CLI subcommands: `submit`, `ask`, `transition`, `get-schema`, `list-schemas`,
//! `set-session-context`, `persist-changeset-workflow`.
//!
//! Workflow goal names and schema filenames are defined in `packages/tddy-workflow-recipes/goals.json`
//! (see [`tddy_tools::schema`] and [`tddy_tools::schema_manifest`]).

use anyhow::{Context, Result};
use clap::Parser;
use log::info;
use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::PathBuf;

use tddy_core::{read_changeset, write_changeset, ChangesetWorkflow};
use tddy_tools::review_persist;
use tddy_tools::schema;
use tddy_tools::schema_manifest;
use tddy_tools::session_actions_cli;
use tddy_tools::session_context;

/// Maximum bytes read from stdin or accepted inline `--data` for `submit` / `ask` (DoS guard).
const MAX_CLI_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// Submit structured output. Validates against schema, relays to tddy-coder via TDDY_SOCKET.
#[derive(Parser)]
#[command(name = "submit")]
pub struct SubmitArgs {
    /// Goal name for validation (uses embedded schema). Required for validation.
    #[arg(long)]
    pub goal: Option<String>,

    /// JSON data (alternative to stdin).
    #[arg(long)]
    pub data: Option<String>,

    /// Read JSON from stdin. Use with pipe or heredoc to avoid shell escaping issues.
    #[arg(long)]
    pub data_stdin: bool,
}

/// Ask clarification questions. Blocks until user answers in TUI.
#[derive(Parser)]
#[command(name = "ask")]
pub struct AskArgs {
    /// Questions JSON (alternative to stdin). Format: {"questions":[{"header":"...","question":"...","options":[...],"multiSelect":false}]}
    #[arg(long)]
    pub data: Option<String>,
}

/// Start a new implementation conversation on a fresh worktree (grill-me handoff).
#[derive(Parser)]
#[command(name = "spawn_conversation")]
pub struct SpawnConversationArgs {
    /// JSON payload: `{"prompt":"...","branch":"optional-slug","base_ref":null}`
    #[arg(long)]
    pub data: Option<String>,
}

/// Print every tool available to the session as a JSON array (name, description,
/// input_schema_json) — the workflow MCP tools + exec catalog + subagent tools + the Bash CLI
/// subcommands. Consumed by the web Inspector → Tools panel (via the coder's `ListExecTools`).
#[derive(Parser)]
#[command(name = "list-tools")]
pub struct ListToolsArgs {}

/// Transition the workflow state machine to another goal (agent-driven orchestration).
///
/// The orchestrator agent calls this (without `--provisional`) to commit a transition and receive
/// the next goal's instructions. Subagents call it **with** `--provisional`: the transition is
/// recorded but not committed until the orchestrator verifies their work and commits.
#[derive(Parser)]
#[command(name = "transition")]
pub struct TransitionArgs {
    /// Target goal id to transition into (e.g. `plan`, `red`, `green`).
    #[arg(long)]
    pub to: String,

    /// Mark this transition provisional (subagents pass this). Provisional transitions are recorded
    /// but not committed until the orchestrator commits.
    #[arg(long, default_value_t = false)]
    pub provisional: bool,
}

/// List registered workflow goals / JSON Schemas (machine-readable JSON).
#[derive(Parser)]
#[command(name = "list-schemas")]
pub struct ListSchemasArgs {}

/// Merge JSON key/value pairs into the active workflow session context.
#[derive(Parser)]
#[command(name = "set-session-context")]
pub struct SetSessionContextArgs {
    /// JSON object to merge into session context (same size limits as submit).
    #[arg(long)]
    pub data: Option<String>,
}

/// Merge validated workflow/demo JSON into `changeset.yaml` (`workflow` block).
#[derive(Parser)]
#[command(name = "persist-changeset-workflow")]
pub struct PersistChangesetWorkflowArgs {
    /// Directory containing `changeset.yaml` (session / plan dir).
    #[arg(long)]
    pub session_dir: PathBuf,

    /// JSON body for the `workflow` block (validated against `changeset-workflow` schema).
    #[arg(long)]
    pub data: Option<String>,
}

/// List session action manifests (JSON printed to stdout).
///
/// When `TDDY_SOCKET` is set the request is relayed to the session-owning process (which may be a
/// remote host); `--session-dir` is then ignored. Without `TDDY_SOCKET` the local `--session-dir`
/// is used for backward-compatible direct discovery.
#[derive(Parser)]
#[command(name = "list-actions")]
pub struct ListActionsArgs {
    /// Directory containing session artifacts (`actions/` subtree). Used only in local
    /// (non-relay) mode when `TDDY_SOCKET` is not set.
    #[arg(long)]
    pub session_dir: Option<PathBuf>,

    /// Filter by relative-path prefix (e.g. `packages/foo`).
    #[arg(long)]
    pub path: Option<String>,

    /// Case-insensitive substring filter on action id, summary, or path.
    #[arg(long)]
    pub query: Option<String>,

    /// Maximum actions to return.
    #[arg(long)]
    pub limit: Option<usize>,

    /// Zero-based offset into the result set (for pagination).
    #[arg(long, default_value_t = 0)]
    pub offset: usize,
}

/// Invoke one session action with validated JSON (`--data`).
///
/// When `TDDY_SOCKET` is set the request is relayed to the session-owning process; `--session-dir`
/// is then ignored. Without `TDDY_SOCKET`, `--session-dir` is required.
#[derive(Parser)]
#[command(name = "invoke-action")]
pub struct InvokeActionArgs {
    /// Directory containing session artifacts. Used only in local (non-relay) mode.
    #[arg(long)]
    pub session_dir: Option<PathBuf>,

    /// Relative path identifier of the action (e.g. `packages/foo/build` or `run-tests`).
    #[arg(long)]
    pub action: String,

    /// JSON object passed to the manifest input schema mapper.
    #[arg(long)]
    pub data: String,
}

/// Get JSON schema for a goal.
#[derive(Parser)]
#[command(name = "get-schema")]
pub struct GetSchemaArgs {
    /// Goal name (plan, red, green, acceptance-tests, evaluate-changes, validate, refactor, update-docs, demo).
    pub goal: String,

    /// Write schema to file (creates common/ subdirs as needed).
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

/// Wire format for submit request (sent to socket).
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitRequest {
    pub r#type: String,
    pub goal: String,
    pub data: serde_json::Value,
}

/// Wire format for submit response (from socket).
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitResponse {
    pub status: String,
    pub goal: Option<String>,
    pub errors: Option<Vec<String>>,
    /// Transport / relay failures from tddy-coder (`ToolCallResponse::Error`).
    #[serde(default)]
    pub message: Option<String>,
}

/// Wire format for ask request (matches ClarificationQuestion).
#[derive(Debug, Serialize, Deserialize)]
pub struct AskRequest {
    pub r#type: String,
    pub questions: Vec<AskQuestionItem>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AskQuestionItem {
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    #[serde(default, rename = "multiSelect")]
    pub multi_select: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default)]
    pub description: String,
}

/// Wire format for ask response.
#[derive(Debug, Serialize, Deserialize)]
pub struct AskResponse {
    pub status: String,
    pub answers: Option<String>,
    pub error: Option<String>,
}

/// Wire format for transition request (sent to socket).
#[derive(Debug, Serialize, Deserialize)]
pub struct TransitionRequest {
    pub r#type: String,
    pub to: String,
    pub provisional: bool,
}

/// Exit codes: 0=success, 1=general failure, 2=usage error, 3=validation error
pub async fn run_submit(args: SubmitArgs) -> Result<()> {
    let json_str = read_input(&args.data, args.data_stdin)?;

    let data: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
        output_error(&format!("invalid JSON: {}", e), 1);
        e
    })?;

    let goal = data
        .get("goal")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let validation_goal = args.goal.as_deref().unwrap_or(&goal);
    if schema::get_schema(validation_goal).is_some() {
        if let Err(errors) = schema::validate_output(validation_goal, &json_str) {
            let tip = schema::validation_error_tip(validation_goal);
            output_validation_error_with_tip(&errors, &tip);
            std::process::exit(3);
        }
    }

    if let Some(socket_path) = std::env::var_os("TDDY_SOCKET") {
        relay_submit(std::path::Path::new(&socket_path), &goal, &data).await?;
    } else {
        if goal == "branch-review" {
            if let Ok(session_dir) = std::env::var("TDDY_SESSION_DIR") {
                if let Err(e) = review_persist::persist_review_md_from_branch_review_json(
                    std::path::Path::new(&session_dir),
                    &json_str,
                ) {
                    output_error(&e, 1);
                }
            }
        }
        output_success(&goal);
    }

    Ok(())
}

fn read_input(data_arg: &Option<String>, data_stdin: bool) -> Result<String> {
    let buf = if data_stdin {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        buf
    } else if let Some(ref s) = data_arg {
        s.clone()
    } else {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        buf
    };
    if buf.len() > MAX_CLI_INPUT_BYTES {
        anyhow::bail!(
            "input exceeds {} bytes (CLI limit for submit/ask)",
            MAX_CLI_INPUT_BYTES
        );
    }
    Ok(buf)
}

fn output_success(goal: &str) {
    let out = serde_json::json!({
        "status": "ok",
        "goal": goal
    });
    println!("{}", serde_json::to_string(&out).unwrap());
}

fn output_error(msg: &str, code: i32) {
    let out = serde_json::json!({
        "status": "error",
        "message": msg
    });
    eprintln!("{}", msg);
    println!("{}", serde_json::to_string(&out).unwrap());
    std::process::exit(code);
}

fn output_validation_error(errors: &[String]) {
    let out = serde_json::json!({
        "status": "error",
        "errors": errors
    });
    println!("{}", serde_json::to_string(&out).unwrap());
    std::process::exit(3);
}

fn output_validation_error_with_tip(errors: &[schema::SchemaError], tip: &str) {
    let error_strings: Vec<String> = errors
        .iter()
        .map(|e| {
            if e.instance_path.is_empty() {
                e.message.clone()
            } else {
                format!("{}: {}", e.instance_path, e.message)
            }
        })
        .collect();
    let out = serde_json::json!({
        "status": "error",
        "errors": error_strings,
        "tip": tip
    });
    eprintln!("{}", tip);
    println!("{}", serde_json::to_string(&out).unwrap());
    std::process::exit(3);
}

#[cfg(unix)]
async fn relay_submit(
    socket_path: &std::path::Path,
    goal: &str,
    data: &serde_json::Value,
) -> Result<()> {
    let req = serde_json::to_value(SubmitRequest {
        r#type: "submit".to_string(),
        goal: goal.to_string(),
        data: data.clone(),
    })?;
    let response_json = tddy_tools::toolcall_client::dispatch_toolcall(socket_path, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let response: SubmitResponse = serde_json::from_value(response_json)
        .with_context(|| "invalid response from tddy-coder")?;

    if response.status == "ok" {
        output_success(response.goal.as_deref().unwrap_or(goal));
    } else if let Some(ref errs) = response.errors {
        output_validation_error(errs);
    } else if let Some(ref msg) = response.message {
        output_error(msg, 1);
    } else {
        output_error("relay failed", 1);
    }

    Ok(())
}

#[cfg(not(unix))]
async fn relay_submit(
    _socket_path: &std::path::Path,
    goal: &str,
    _data: &serde_json::Value,
) -> Result<()> {
    output_success(goal);
    Ok(())
}

pub async fn run_ask(args: AskArgs) -> Result<()> {
    let json_str = read_input(&args.data, false)?;

    let parsed: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
        output_error(&format!("invalid JSON: {}", e), 1);
        e
    })?;

    let questions = parsed
        .get("questions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            output_error("missing or invalid 'questions' array", 2);
            anyhow::anyhow!("invalid questions format")
        })?;

    let questions: Vec<AskQuestionItem> =
        serde_json::from_value(serde_json::Value::Array(questions.clone())).map_err(|e| {
            output_error(&format!("invalid questions format: {}", e), 2);
            e
        })?;

    if let Some(socket_path) = std::env::var_os("TDDY_SOCKET") {
        relay_ask(std::path::Path::new(&socket_path), &questions).await?;
    } else {
        let out = serde_json::json!({
            "status": "ok",
            "message": "TDDY_SOCKET not set; questions not relayed"
        });
        println!("{}", serde_json::to_string(&out).unwrap());
    }

    Ok(())
}

#[cfg(unix)]
async fn relay_ask(socket_path: &std::path::Path, questions: &[AskQuestionItem]) -> Result<()> {
    let req = serde_json::to_value(AskRequest {
        r#type: "ask".to_string(),
        questions: questions.to_vec(),
    })?;
    let response_json = tddy_tools::toolcall_client::dispatch_toolcall(socket_path, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let response: AskResponse = serde_json::from_value(response_json)
        .with_context(|| "invalid response from tddy-coder")?;

    if response.status == "ok" {
        let out = serde_json::json!({
            "status": "ok",
            "answers": response.answers
        });
        println!("{}", serde_json::to_string(&out).unwrap());
    } else {
        output_error(response.error.as_deref().unwrap_or("ask failed"), 1);
    }

    Ok(())
}

pub async fn run_spawn_conversation(args: SpawnConversationArgs) -> Result<()> {
    let json_str = read_input(&args.data, false)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
        output_error(&format!("invalid JSON: {}", e), 1);
        e
    })?;
    let prompt = parsed
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            output_error("missing or invalid 'prompt' string", 2);
            anyhow::anyhow!("invalid spawn_conversation format")
        })?;
    let branch = parsed.get("branch").and_then(|v| v.as_str());
    let base_ref = parsed.get("base_ref").and_then(|v| v.as_str());
    let request = serde_json::json!({
        "type": "spawn-conversation",
        "prompt": prompt,
        "branch": branch,
        "base_ref": base_ref,
    });
    let Some(socket_path) = std::env::var_os("TDDY_SOCKET") else {
        output_error("TDDY_SOCKET not set; spawn_conversation not relayed", 1);
        return Ok(());
    };
    let response_json =
        tddy_tools::toolcall_client::dispatch_toolcall(std::path::Path::new(&socket_path), request)
            .await
            .map_err(|e| {
                output_error(&e, 1);
                anyhow::anyhow!(e)
            })?;
    println!("{}", response_json);
    Ok(())
}

/// The Bash CLI subcommands the agent invokes via `Bash(tddy-tools <cmd> …)` — not MCP tools, but
/// part of the session's real toolset, so the Inspector lists them alongside the MCP/exec tools.
const CLI_SUBCOMMAND_TOOLS: &[(&str, &str)] = &[
    (
        "submit",
        "Submit a PRD/goal document to the workflow (relayed over TDDY_SOCKET).",
    ),
    (
        "ask",
        "Ask the user clarification question(s); blocks until answered in the session UI.",
    ),
    (
        "transition",
        "Transition the workflow state machine to another goal.",
    ),
    (
        "get-schema",
        "Print the JSON schema for a workflow document type.",
    ),
    (
        "build",
        "Run a build for the session's codebase and stream results.",
    ),
];

/// One `list-tools` entry — matches the coder-side `ToolDef` shape (name/description/schema).
#[derive(serde::Serialize)]
struct ListToolsEntry {
    name: String,
    description: String,
    input_schema_json: String,
}

/// The full session toolset: MCP workflow tools + exec catalog + subagent tools (when enabled) +
/// the Bash CLI subcommands. Pure — safe to call without a live session/socket.
fn all_session_tools() -> Vec<ListToolsEntry> {
    let mut out: Vec<ListToolsEntry> = tddy_tools::server::PermissionServer::advertised_tool_defs()
        .into_iter()
        .map(|t| ListToolsEntry {
            name: t.name,
            description: t.description,
            input_schema_json: t.input_schema_json,
        })
        .collect();
    for (name, description) in CLI_SUBCOMMAND_TOOLS {
        out.push(ListToolsEntry {
            name: (*name).to_string(),
            description: (*description).to_string(),
            input_schema_json: "{}".to_string(),
        });
    }
    out
}

/// `list-tools`: print the full session toolset as a JSON array (see [`ListToolsArgs`]).
pub fn run_list_tools(_args: ListToolsArgs) -> Result<()> {
    println!("{}", serde_json::to_string(&all_session_tools())?);
    Ok(())
}

/// Invoke a single session tool by name with JSON `--data` — the execution counterpart to
/// `list-tools`, used by the web Inspector → Tools "invoke" button (routed here by the coder's
/// `ExecuteTool` for any non-engine tool). MCP workflow tools run in-process against the
/// `PermissionServer`; the Bash CLI subcommands (submit/ask/transition/get-schema/build) are
/// re-dispatched to their own handlers so their exact `TDDY_SOCKET` relay runs.
#[derive(Parser)]
#[command(name = "call-tool")]
pub struct CallToolArgs {
    /// Tool name, exactly as reported by `list-tools`.
    pub name: String,
    /// JSON arguments object for the tool (defaults to `{}`).
    #[arg(long)]
    pub data: Option<String>,
}

pub async fn run_call_tool(args: CallToolArgs) -> Result<()> {
    let raw = args.data.clone().unwrap_or_default();
    let value: serde_json::Value = if raw.trim().is_empty() {
        serde_json::Value::Object(Default::default())
    } else {
        serde_json::from_str(&raw).context("invalid --data JSON")?
    };
    // Helper: a JSON field as a CLI string (string values verbatim; objects/arrays re-serialized).
    let as_str = |v: &serde_json::Value| -> String {
        match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    };
    // Bash CLI subcommands: re-invoke this binary with the mapped flags so the real handler (and
    // its TDDY_SOCKET relay) runs, inheriting our env. args_json maps to each tool's primary flags.
    let cli_argv: Option<Vec<String>> = match args.name.as_str() {
        "submit" => {
            let mut v = vec!["submit".to_string()];
            if let Some(g) = value.get("goal").and_then(|x| x.as_str()) {
                v.push("--goal".into());
                v.push(g.to_string());
            }
            if let Some(d) = value.get("data") {
                v.push("--data".into());
                v.push(as_str(d));
            }
            Some(v)
        }
        "ask" => {
            let d = value.get("data").cloned().unwrap_or_else(|| value.clone());
            Some(vec!["ask".to_string(), "--data".into(), as_str(&d)])
        }
        "transition" => {
            let to = value.get("to").and_then(|x| x.as_str()).unwrap_or_default();
            let mut v = vec!["transition".to_string(), "--to".into(), to.to_string()];
            if value
                .get("provisional")
                .and_then(|x| x.as_bool())
                .unwrap_or(false)
            {
                v.push("--provisional".into());
            }
            Some(v)
        }
        "get-schema" => {
            // `get-schema` takes the goal as a positional argument (not a flag).
            let g = value
                .get("goal")
                .and_then(|x| x.as_str())
                .unwrap_or_default();
            Some(vec!["get-schema".to_string(), g.to_string()])
        }
        "build" => {
            let mut v = vec!["build".to_string()];
            if let Some(t) = value.get("target").and_then(|x| x.as_str()) {
                v.push("--target".into());
                v.push(t.to_string());
            }
            if let Some(r) = value.get("repo_dir").and_then(|x| x.as_str()) {
                v.push("--repo-dir".into());
                v.push(r.to_string());
            }
            Some(v)
        }
        _ => None,
    };
    if let Some(argv) = cli_argv {
        let exe =
            std::env::current_exe().context("resolve current exe for call-tool re-dispatch")?;
        let status = std::process::Command::new(exe)
            .args(&argv)
            .status()
            .context("run cli subcommand")?;
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }
        return Ok(());
    }
    // MCP workflow tool: invoke it in-process against the PermissionServer, exactly as the agent
    // would over MCP (relay tools still relay over TDDY_SOCKET from inside their methods).
    let server = tddy_tools::server::PermissionServer::new();
    match server.call_tool_by_name(&args.name, value).await {
        Ok(out) => {
            println!("{out}");
            Ok(())
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod list_tools_tests {
    use super::all_session_tools;

    #[test]
    fn list_tools_includes_mcp_exec_and_cli_tools() {
        let names: Vec<String> = all_session_tools().into_iter().map(|t| t.name).collect();
        for expected in ["spawn_conversation", "Read", "Shell", "submit"] {
            assert!(
                names.iter().any(|n| n == expected),
                "list-tools must advertise {expected}; got {names:?}"
            );
        }
    }
}

pub async fn run_transition(args: TransitionArgs) -> Result<()> {
    if let Some(socket_path) = std::env::var_os("TDDY_SOCKET") {
        relay_transition(
            std::path::Path::new(&socket_path),
            &args.to,
            args.provisional,
        )
        .await?;
    } else {
        let out = serde_json::json!({
            "status": "error",
            "message": "TDDY_SOCKET not set; transition not relayed"
        });
        println!("{}", serde_json::to_string(&out).unwrap());
    }
    Ok(())
}

#[cfg(unix)]
async fn relay_transition(
    socket_path: &std::path::Path,
    to: &str,
    provisional: bool,
) -> Result<()> {
    let req = serde_json::to_value(TransitionRequest {
        r#type: "transition".to_string(),
        to: to.to_string(),
        provisional,
    })?;
    let response_json = tddy_tools::toolcall_client::dispatch_toolcall(socket_path, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    // Print the relay's JSON verbatim so the agent reads `instructions` (committed),
    // the provisional acknowledgement, or `reason` (rejected).
    println!("{}", serde_json::to_string(&response_json)?);
    Ok(())
}

#[cfg(not(unix))]
async fn relay_transition(
    _socket_path: &std::path::Path,
    _to: &str,
    _provisional: bool,
) -> Result<()> {
    let out = serde_json::json!({
        "status": "error",
        "message": "Unix socket not available on this platform"
    });
    println!("{}", serde_json::to_string(&out).unwrap());
    Ok(())
}

pub fn run_list_schemas(_args: ListSchemasArgs) -> Result<()> {
    let goals = schema_manifest::list_registered_goals().context("load schema manifest")?;
    info!(
        target: "tddy_tools::cli",
        "list-schemas ({} goals)",
        goals.len()
    );
    let out = serde_json::json!({ "goals": goals });
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}

pub fn run_persist_changeset_workflow(args: PersistChangesetWorkflowArgs) -> Result<()> {
    info!(
        target: "tddy_tools::cli",
        "persist-changeset-workflow: session_dir={}",
        args.session_dir.display()
    );
    let json_str = read_input(&args.data, false)?;
    if let Err(errors) = schema::validate_output("changeset-workflow", &json_str) {
        let tip = schema::validation_error_tip("changeset-workflow");
        output_validation_error_with_tip(&errors, &tip);
        std::process::exit(3);
    }
    let workflow: ChangesetWorkflow = match serde_json::from_str(&json_str) {
        Ok(w) => w,
        Err(e) => {
            output_error(&format!("invalid workflow JSON: {}", e), 1);
            unreachable!()
        }
    };
    let mut cs = read_changeset(&args.session_dir)
        .map_err(|e| anyhow::anyhow!("read changeset {}: {}", args.session_dir.display(), e))?;
    cs.workflow = Some(workflow);
    write_changeset(&args.session_dir, &cs)
        .map_err(|e| anyhow::anyhow!("write changeset {}: {}", args.session_dir.display(), e))?;
    Ok(())
}

pub fn run_set_session_context(args: SetSessionContextArgs) -> Result<()> {
    info!(
        target: "tddy_tools::cli",
        "set-session-context: merging payload into workflow session"
    );
    let json_str = read_input(&args.data, false)?;
    let patch: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid JSON: {}", e);
            std::process::exit(1);
        }
    };
    if !patch.is_object() {
        eprintln!(
            "invalid payload: expected a JSON object at the top level (non-array, non-scalar)"
        );
        std::process::exit(1);
    }
    let session_dir = std::env::var("TDDY_SESSION_DIR")
        .map_err(|_| anyhow::anyhow!("TDDY_SESSION_DIR is required for set-session-context"))?;
    let session_id = std::env::var("TDDY_WORKFLOW_SESSION_ID").map_err(|_| {
        anyhow::anyhow!("TDDY_WORKFLOW_SESSION_ID is required for set-session-context")
    })?;
    let workflow_dir = PathBuf::from(session_dir).join(".workflow");
    session_context::apply_session_context_merge(&workflow_dir, &session_id, &patch)
}

/// Wire format for `list-actions` relay request (sent to TDDY_SOCKET).
#[derive(Debug, Serialize)]
struct ListActionsRelayRequest {
    r#type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    path_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<usize>,
}

/// Wire format for `list-actions` relay response.
#[derive(Debug, Deserialize)]
struct ListActionsRelayResponse {
    status: String,
    #[serde(default)]
    actions: Option<serde_json::Value>,
    #[serde(default)]
    total: Option<usize>,
    #[serde(default)]
    message: Option<String>,
}

/// Wire format for `invoke-action` relay request.
#[derive(Debug, Serialize)]
struct InvokeActionRelayRequest {
    r#type: &'static str,
    action: String,
    data: String,
}

/// Wire format for `invoke-action` relay response.
#[derive(Debug, Deserialize)]
struct InvokeActionRelayResponse {
    status: String,
    #[serde(default)]
    record: Option<serde_json::Value>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    exit_code: Option<i32>,
}

pub async fn run_list_actions(args: ListActionsArgs) -> Result<()> {
    if let Some(socket_path) = std::env::var_os("TDDY_SOCKET") {
        relay_list_actions(std::path::Path::new(&socket_path), &args).await?;
    } else {
        let session_dir = args.session_dir.as_deref().ok_or_else(|| {
            anyhow::anyhow!("--session-dir is required when TDDY_SOCKET is not set")
        })?;
        session_actions_cli::run_list_actions(
            session_dir,
            args.path.as_deref(),
            args.query.as_deref(),
            args.limit,
            args.offset,
        )?;
    }
    Ok(())
}

pub async fn run_invoke_action(args: InvokeActionArgs) -> Result<()> {
    if let Some(socket_path) = std::env::var_os("TDDY_SOCKET") {
        relay_invoke_action(std::path::Path::new(&socket_path), &args).await?;
    } else {
        let session_dir = args.session_dir.as_deref().ok_or_else(|| {
            anyhow::anyhow!("--session-dir is required when TDDY_SOCKET is not set")
        })?;
        session_actions_cli::run_invoke_action(session_dir, &args.action, &args.data)?;
    }
    Ok(())
}

#[cfg(unix)]
async fn relay_list_actions(socket_path: &std::path::Path, args: &ListActionsArgs) -> Result<()> {
    let req = serde_json::to_value(ListActionsRelayRequest {
        r#type: "list-actions",
        path_prefix: args.path.clone(),
        query: args.query.clone(),
        limit: args.limit,
        offset: if args.offset > 0 {
            Some(args.offset)
        } else {
            None
        },
    })?;
    let response_json = tddy_tools::toolcall_client::dispatch_toolcall(socket_path, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let response: ListActionsRelayResponse =
        serde_json::from_value(response_json).with_context(|| "invalid response from relay")?;

    if response.status == "ok" {
        let out = serde_json::json!({
            "actions": response.actions.unwrap_or(serde_json::Value::Array(vec![])),
            "total": response.total.unwrap_or(0),
            "offset": args.offset,
            "limit": args.limit,
        });
        println!("{}", serde_json::to_string(&out)?);
    } else {
        let msg = response
            .message
            .as_deref()
            .unwrap_or("list-actions relay failed");
        output_error(msg, 1);
    }

    Ok(())
}

#[cfg(not(unix))]
async fn relay_list_actions(_socket_path: &std::path::Path, args: &ListActionsArgs) -> Result<()> {
    // No relay available; fall back to local path (will re-check session_dir).
    if let Some(ref session_dir) = args.session_dir {
        session_actions_cli::run_list_actions(
            session_dir,
            args.path.as_deref(),
            args.query.as_deref(),
            args.limit,
            args.offset,
        )?;
    } else {
        output_error("--session-dir required on this platform", 1);
    }
    Ok(())
}

#[cfg(unix)]
async fn relay_invoke_action(socket_path: &std::path::Path, args: &InvokeActionArgs) -> Result<()> {
    let req = serde_json::to_value(InvokeActionRelayRequest {
        r#type: "invoke-action",
        action: args.action.clone(),
        data: args.data.clone(),
    })?;
    let response_json = tddy_tools::toolcall_client::dispatch_toolcall(socket_path, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let response: InvokeActionRelayResponse =
        serde_json::from_value(response_json).with_context(|| "invalid response from relay")?;

    if response.status == "ok" {
        if let Some(record) = response.record {
            println!("{}", serde_json::to_string(&record)?);
        } else {
            println!("{{}}");
        }
    } else {
        let msg = response
            .message
            .as_deref()
            .unwrap_or("invoke-action relay failed");
        let exit_code = response.exit_code.unwrap_or(1);
        eprintln!("{}", msg);
        std::process::exit(exit_code);
    }

    Ok(())
}

#[cfg(not(unix))]
async fn relay_invoke_action(
    _socket_path: &std::path::Path,
    args: &InvokeActionArgs,
) -> Result<()> {
    if let Some(ref session_dir) = args.session_dir {
        session_actions_cli::run_invoke_action(session_dir, &args.action, &args.data)?;
    } else {
        output_error("--session-dir required on this platform", 1);
    }
    Ok(())
}

pub fn run_get_schema(args: GetSchemaArgs) -> Result<()> {
    let content = match schema::get_schema(&args.goal) {
        Some(c) => c,
        None => {
            output_error(&format!("unknown goal: {}", args.goal), 2);
            unreachable!("output_error exits")
        }
    };
    if let Some(ref out_path) = args.output {
        if let Err(e) = schema::write_schema_to_path(&args.goal, out_path) {
            output_error(&format!("failed to write schema: {}", e), 1);
        }
    } else {
        println!("{}", content);
    }
    Ok(())
}

#[cfg(not(unix))]
async fn relay_ask(_socket_path: &std::path::Path, _questions: &[AskQuestionItem]) -> Result<()> {
    let out = serde_json::json!({
        "status": "ok",
        "message": "Unix socket not available on this platform"
    });
    println!("{}", serde_json::to_string(&out).unwrap());
    Ok(())
}

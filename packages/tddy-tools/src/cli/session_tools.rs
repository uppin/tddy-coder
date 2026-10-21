use crate::cli::ListToolsArgs;
use anyhow::{Context, Result};
use clap::Parser;

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
// TODO(restructure-engine): `move_item` widened `ListToolsEntry::name` but not the struct itself, which
// `all_session_tools` (widened) returns; widened by hand to keep the tree compiling.
#[derive(serde::Serialize)]
pub(super) struct ListToolsEntry {
    pub(super) name: String,
    description: String,
    input_schema_json: String,
}

/// The full session toolset: MCP workflow tools + exec catalog + subagent tools (when enabled) +
/// the Bash CLI subcommands. Pure — safe to call without a live session/socket.
pub(super) fn all_session_tools() -> Vec<ListToolsEntry> {
    let mut out: Vec<ListToolsEntry> = crate::server::PermissionServer::advertised_tool_defs()
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
    let server = crate::server::PermissionServer::new();
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

//! CLI subcommands: `submit`, `ask`, `get-schema`, `list-schemas`, `set-session-context`,
//! `persist-changeset-workflow`.
//!
//! Workflow goal names and schema filenames are defined in `packages/tddy-workflow-recipes/goals.json`
//! (see [`tddy_tools::schema`] and [`tddy_tools::schema_manifest`]).

use anyhow::{Context, Result};
use clap::Parser;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::PathBuf;
use tddy_core::changeset::{read_changeset, write_changeset_atomic, ChangesetWorkflow};

use tddy_tools::review_persist;
use tddy_tools::schema;
use tddy_tools::schema_manifest;
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

/// Merge workflow/demo JSON into `changeset.yaml` for the given plan session directory.
#[derive(Parser)]
#[command(name = "persist-changeset-workflow")]
pub struct PersistChangesetWorkflowArgs {
    /// Directory containing `changeset.yaml` (plan / session tree).
    #[arg(long)]
    pub session_dir: PathBuf,
    /// JSON object: `run_optional_step_x`, `demo_options`, `tool_schema_id` (schema: changeset-workflow).
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

/// Exit codes: 0=success, 1=general failure, 2=usage error, 3=validation error
pub fn run_submit(args: SubmitArgs) -> Result<()> {
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

    if let Err(e) = maybe_persist_branch_review_artifact(validation_goal, &json_str) {
        output_error(&e.to_string(), 1);
    }

    if let Some(socket_path) = std::env::var_os("TDDY_SOCKET") {
        relay_submit(std::path::Path::new(&socket_path), &goal, &data)?;
    } else {
        output_success(&goal);
    }

    Ok(())
}

/// After schema validation, write `review.md` under [`TDDY_SESSION_DIR`] for **`branch-review`** submits.
///
/// Backends set `TDDY_SESSION_DIR` for agent subprocesses; when unset (e.g. local CLI-only use), persistence is skipped.
fn maybe_persist_branch_review_artifact(validation_goal: &str, json_str: &str) -> Result<()> {
    if validation_goal != "branch-review" {
        return Ok(());
    }
    let Some(dir) = std::env::var_os("TDDY_SESSION_DIR") else {
        debug!(
            target: "tddy_tools::cli",
            "branch-review submit: TDDY_SESSION_DIR unset; skip review.md persist"
        );
        return Ok(());
    };
    let session_dir = std::path::Path::new(&dir);
    review_persist::persist_review_md_from_branch_review_json(session_dir, json_str).map_err(|e| {
        anyhow::anyhow!(
            "failed to persist review.md under {}: {}",
            session_dir.display(),
            e
        )
    })
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
fn relay_submit(socket_path: &std::path::Path, goal: &str, data: &serde_json::Value) -> Result<()> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket_path).with_context(|| {
        format!(
            "failed to connect to TDDY_SOCKET: {}",
            socket_path.display()
        )
    })?;

    let req = SubmitRequest {
        r#type: "submit".to_string(),
        goal: goal.to_string(),
        data: data.clone(),
    };
    let line = serde_json::to_string(&req)?;
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut reader = BufReader::new(&mut stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;
    let response_line = response_line.trim();

    let response: SubmitResponse = serde_json::from_str(response_line)
        .with_context(|| format!("invalid response from tddy-coder: {}", response_line))?;

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
fn relay_submit(
    _socket_path: &std::path::Path,
    goal: &str,
    _data: &serde_json::Value,
) -> Result<()> {
    output_success(goal);
    Ok(())
}

pub fn run_ask(args: AskArgs) -> Result<()> {
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
        relay_ask(std::path::Path::new(&socket_path), &questions)?;
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
fn relay_ask(socket_path: &std::path::Path, questions: &[AskQuestionItem]) -> Result<()> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket_path).with_context(|| {
        format!(
            "failed to connect to TDDY_SOCKET: {}",
            socket_path.display()
        )
    })?;

    let req = AskRequest {
        r#type: "ask".to_string(),
        questions: questions.to_vec(),
    };
    let line = serde_json::to_string(&req)?;
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut reader = BufReader::new(&mut stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;
    let response_line = response_line.trim();

    let response: AskResponse = serde_json::from_str(response_line)
        .with_context(|| format!("invalid response from tddy-coder: {}", response_line))?;

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

/// Read `changeset.yaml`, merge validated workflow JSON, atomically replace the file.
pub fn run_persist_changeset_workflow(args: PersistChangesetWorkflowArgs) -> Result<()> {
    info!(
        target: "tddy_tools::cli",
        "persist-changeset-workflow: session_dir={}",
        args.session_dir.display()
    );
    if let Err(errors) = schema::validate_output("changeset-workflow", &args.data) {
        let tip = schema::validation_error_tip("changeset-workflow");
        output_validation_error_with_tip(&errors, &tip);
        std::process::exit(3);
    }
    let workflow: ChangesetWorkflow = serde_json::from_str(&args.data).unwrap_or_else(|e| {
        output_error(&format!("invalid JSON after validation: {e}"), 1);
        unreachable!()
    });
    debug!(
        target: "tddy_tools::cli",
        "persist-changeset-workflow: parsed workflow run_optional_step_x={:?} demo_options_len={}",
        workflow.run_optional_step_x,
        workflow.demo_options.len()
    );

    let mut cs = read_changeset(&args.session_dir).unwrap_or_else(|e| {
        output_error(&e.to_string(), 1);
        unreachable!()
    });
    cs.workflow = Some(workflow);
    write_changeset_atomic(&args.session_dir, &cs).unwrap_or_else(|e| {
        output_error(&e.to_string(), 1);
        unreachable!()
    });
    info!(
        target: "tddy_tools::cli",
        "persist-changeset-workflow: wrote changeset.yaml atomically"
    );
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
fn relay_ask(_socket_path: &std::path::Path, _questions: &[AskQuestionItem]) -> Result<()> {
    let out = serde_json::json!({
        "status": "ok",
        "message": "Unix socket not available on this platform"
    });
    println!("{}", serde_json::to_string(&out).unwrap());
    Ok(())
}

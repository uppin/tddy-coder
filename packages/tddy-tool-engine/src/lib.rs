//! Tool dispatch engine for workspace sessions.
//!
//! Implements `execute_tool` which routes tool calls to their respective implementations.
//! All file paths are validated against the worktree root to prevent path traversal.
//!
//! Shared by `tddy-daemon` (remote-codebase workspace sessions) and `tddy-coder` (session
//! participant `ExecuteTool` / `ListExecTools`).

pub mod catalog;

pub use catalog::{tool_catalog, ToolDef};

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use tddy_task::{ChannelKind, TaskBody, TaskChannel, TaskContext, TaskRegistry, TaskStatus};

/// Outcome of a tool execution.
pub struct ToolOutcome {
    pub result_json: String,
    pub is_error: bool,
    pub error_message: String,
    /// The task ID for this invocation (equals `job_id` when `job_running`, otherwise the
    /// task_id of the completed task).
    pub job_id: String,
    /// True immediately after a background shell job is launched.
    pub job_running: bool,
}

impl ToolOutcome {
    fn ok(result_json: impl Into<String>) -> Self {
        Self {
            result_json: result_json.into(),
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        }
    }

    fn err(msg: impl Into<String>) -> Self {
        let m = msg.into();
        Self {
            result_json: serde_json::json!({ "error": m }).to_string(),
            is_error: true,
            error_message: m,
            job_id: String::new(),
            job_running: false,
        }
    }
}

/// Validate and resolve a path argument, ensuring it stays within `worktree_root`.
///
/// Returns an `Err` string if the path escapes the worktree root.
fn contain_path(worktree_root: &Path, arg_path: &str) -> Result<PathBuf, String> {
    let root = worktree_root
        .canonicalize()
        .map_err(|e| format!("cannot canonicalize worktree root: {e}"))?;

    let candidate = if Path::new(arg_path).is_absolute() {
        PathBuf::from(arg_path)
    } else {
        root.join(arg_path)
    };

    // For existing paths, canonicalize and check containment.
    if candidate.exists() {
        let canon = candidate
            .canonicalize()
            .map_err(|e| format!("cannot canonicalize path: {e}"))?;
        if !canon.starts_with(&root) {
            return Err(format!(
                "resolved path escapes worktree: {}",
                canon.display()
            ));
        }
        return Ok(canon);
    }

    // For new (non-existent) paths: reject any `..` components immediately before the ancestor
    // walk, so that `root/foo/../../../etc/passwd` is caught here rather than by the walk.
    for component in candidate.components() {
        if component == Component::ParentDir {
            return Err(format!(
                "path contains '..' component: {}",
                candidate.display()
            ));
        }
    }

    // Walk up to the nearest existing ancestor and verify it is inside the worktree.
    let mut check = candidate.clone();
    loop {
        if check.exists() {
            let canon = check
                .canonicalize()
                .map_err(|e| format!("cannot canonicalize ancestor: {e}"))?;
            if !canon.starts_with(&root) {
                return Err(format!("path escapes worktree: {}", candidate.display()));
            }
            break;
        }
        if !check.pop() {
            return Err(format!("path escapes worktree: {}", candidate.display()));
        }
    }

    Ok(candidate)
}

/// Background shell job body — runs a shell command and writes combined output to channel "0".
struct ShellTaskBody {
    command: String,
    root: PathBuf,
    /// Extra environment variables set on the spawned shell (in addition to the inherited env).
    env: Vec<(String, String)>,
}

#[async_trait]
impl TaskBody for ShellTaskBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        let result = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&self.command)
            .current_dir(&self.root)
            .envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .output()
            .await;

        match result {
            Ok(out) => {
                if let Some(ch) = ctx.channel("0") {
                    ch.write(bytes::Bytes::from(out.stdout));
                    ch.write(bytes::Bytes::from(out.stderr));
                }
                let code = out.status.code().unwrap_or(-1);
                ctx.set_result(serde_json::json!({ "exit_code": code }).to_string());
                TaskStatus::Completed {
                    exit_code: Some(code),
                }
            }
            Err(e) => TaskStatus::Failed {
                message: format!("Shell: spawn failed: {e}"),
            },
        }
    }
}

/// Wrap an already-completed inline tool invocation as a terminal task for observability.
///
/// Returns the new `TaskId` string so callers can populate `ToolOutcome.job_id`.
async fn register_sync_task(
    registry: &TaskRegistry,
    session_id: &str,
    kind: &str,
    outcome: &ToolOutcome,
) -> String {
    let terminal = if outcome.is_error {
        TaskStatus::Failed {
            message: outcome.error_message.clone(),
        }
    } else {
        TaskStatus::Completed { exit_code: Some(0) }
    };
    let task_id = registry
        .create_terminal_task(
            kind,
            session_id,
            Some(outcome.result_json.clone()),
            terminal,
        )
        .await;
    task_id.0
}

/// Dispatch a tool call within the given `worktree_root`.
///
/// Returns a `ToolOutcome` — never panics or returns a gRPC-level error.
pub async fn execute_tool(
    worktree_root: &Path,
    tool_name: &str,
    args_json: &str,
    registry: &TaskRegistry,
    session_id: &str,
) -> ToolOutcome {
    execute_tool_with_env(
        worktree_root,
        tool_name,
        args_json,
        registry,
        session_id,
        &[],
    )
    .await
}

/// Like [`execute_tool`], but with extra environment variables applied to spawned shell commands
/// (e.g. a managed session's per-session `TDDY_SOCKET` + `PATH` so host-side `tddy-tools transition`
/// reaches the session's `WorkflowController`).
pub async fn execute_tool_with_env(
    worktree_root: &Path,
    tool_name: &str,
    args_json: &str,
    registry: &TaskRegistry,
    session_id: &str,
    extra_env: &[(String, String)],
) -> ToolOutcome {
    let args: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(e) => return ToolOutcome::err(format!("invalid args_json: {e}")),
    };

    let kind = format!("execute_tool:{tool_name}");

    match tool_name {
        "Shell" => tool_shell(worktree_root, &args, registry, session_id, &kind, extra_env).await,
        "Await" => tool_await(&args, registry).await,
        "LspDiagnostics" | "LspDefinition" | "LspReferences" | "LspHover" | "LspSymbols" => {
            tool_lsp(worktree_root, tool_name, &args).await
        }
        _ => {
            // Sync tool — run inline, then register as a terminal task for observability.
            let mut outcome = match tool_name {
                "Read" => tool_read(worktree_root, &args),
                "Write" => tool_write(worktree_root, &args),
                "StrReplace" => tool_str_replace(worktree_root, &args),
                "Delete" => tool_delete(worktree_root, &args),
                "Grep" => tool_grep(worktree_root, &args).await,
                "Glob" => tool_glob(worktree_root, &args),
                "ReadLints" => tool_read_lints(),
                "SemanticSearch" => tool_semantic_search(worktree_root, &args).await,
                other => ToolOutcome::err(format!("unknown tool: {other}")),
            };
            let task_id = register_sync_task(registry, session_id, &kind, &outcome).await;
            outcome.job_id = task_id;
            outcome
        }
    }
}

/// Dispatch one of the five language-agnostic LSP tools to the registered
/// [`tddy_core::toolcall::lsp::LspExecutor`]. The executor's methods are synchronous and
/// block on an internal runtime, so they are run on a blocking thread via `spawn_blocking`.
async fn tool_lsp(worktree_root: &Path, tool_name: &str, args: &serde_json::Value) -> ToolOutcome {
    let Some(executor) = tddy_core::toolcall::lsp::lsp_executor() else {
        return ToolOutcome::err(format!("{tool_name}: no language server available"));
    };
    let query = match parse_lsp_query(args) {
        Ok(q) => q,
        Err(e) => return ToolOutcome::err(format!("{tool_name}: {e}")),
    };
    let root = worktree_root.to_path_buf();
    let name = tool_name.to_string();
    let result = tokio::task::spawn_blocking(move || match name.as_str() {
        "LspDiagnostics" => executor.diagnostics(&root, &query),
        "LspDefinition" => executor.definition(&root, &query),
        "LspReferences" => executor.references(&root, &query),
        "LspHover" => executor.hover(&root, &query),
        "LspSymbols" => executor.symbols(&root, &query),
        other => Err(format!("unknown lsp tool: {other}")),
    })
    .await;
    match result {
        Ok(Ok(value)) => ToolOutcome::ok(value.to_string()),
        Ok(Err(e)) => ToolOutcome::err(format!("{tool_name}: {e}")),
        Err(e) => ToolOutcome::err(format!("{tool_name}: {e}")),
    }
}

fn parse_lsp_query(args: &serde_json::Value) -> Result<tddy_core::toolcall::lsp::LspQuery, String> {
    let target = args
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or("missing 'target' argument")?
        .to_string();
    let file = args
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let line = args.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let character = args.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let symbol_query = args
        .get("query")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Ok(tddy_core::toolcall::lsp::LspQuery {
        target,
        file,
        line,
        character,
        symbol_query,
    })
}

fn tool_read(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Read: missing 'path' argument"),
    };

    let resolved = match contain_path(root, path_str) {
        Ok(p) => p,
        Err(e) => return ToolOutcome::err(format!("Read: {e}")),
    };

    match std::fs::read_to_string(&resolved) {
        Ok(content) => ToolOutcome::ok(serde_json::json!({ "content": content }).to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ToolOutcome {
            result_json: serde_json::json!({ "error": "file not found" }).to_string(),
            is_error: true,
            error_message: "file not found".to_string(),
            job_id: String::new(),
            job_running: false,
        },
        Err(e) => ToolOutcome::err(format!("Read: {e}")),
    }
}

fn tool_write(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Write: missing 'path' argument"),
    };
    let contents = match args.get("contents").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return ToolOutcome::err("Write: missing 'contents' argument"),
    };

    let resolved = match contain_path(root, path_str) {
        Ok(p) => p,
        Err(e) => return ToolOutcome::err(format!("Write: {e}")),
    };

    if let Some(parent) = resolved.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return ToolOutcome::err(format!("Write: create_dir_all failed: {e}"));
        }
    }

    match std::fs::write(&resolved, contents) {
        Ok(()) => {
            let bytes = contents.len();
            ToolOutcome::ok(serde_json::json!({ "bytes_written": bytes }).to_string())
        }
        Err(e) => ToolOutcome::err(format!("Write: {e}")),
    }
}

fn tool_str_replace(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("StrReplace: missing 'path' argument"),
    };
    let old_string = match args.get("old_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolOutcome::err("StrReplace: missing 'old_string' argument"),
    };
    let new_string = match args.get("new_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolOutcome::err("StrReplace: missing 'new_string' argument"),
    };

    let resolved = match contain_path(root, path_str) {
        Ok(p) => p,
        Err(e) => return ToolOutcome::err(format!("StrReplace: {e}")),
    };

    let content = match std::fs::read_to_string(&resolved) {
        Ok(c) => c,
        Err(e) => return ToolOutcome::err(format!("StrReplace: read failed: {e}")),
    };

    let count = content.matches(old_string).count();
    if count == 0 {
        return ToolOutcome::err("StrReplace: old_string not found in file");
    }
    if count > 1 {
        return ToolOutcome::err(format!(
            "StrReplace: old_string matches {count} times (must be unique)"
        ));
    }

    let new_content = content.replacen(old_string, new_string, 1);
    match std::fs::write(&resolved, &new_content) {
        Ok(()) => ToolOutcome::ok(
            serde_json::json!({ "replaced": true, "bytes_written": new_content.len() }).to_string(),
        ),
        Err(e) => ToolOutcome::err(format!("StrReplace: write failed: {e}")),
    }
}

fn tool_delete(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let path_str = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Delete: missing 'path' argument"),
    };

    let resolved = match contain_path(root, path_str) {
        Ok(p) => p,
        Err(e) => return ToolOutcome::err(format!("Delete: {e}")),
    };

    match std::fs::remove_file(&resolved) {
        Ok(()) => ToolOutcome::ok(serde_json::json!({ "deleted": true }).to_string()),
        Err(e) => ToolOutcome::err(format!("Delete: {e}")),
    }
}

async fn tool_grep(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let pattern = match args.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Grep: missing 'pattern' argument"),
    };

    let output = tokio::process::Command::new("rg")
        .args(["--json", "-e", pattern, "."])
        .current_dir(root)
        .output()
        .await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut matches = vec![];
            for line in stdout.lines() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    if v.get("type").and_then(|t| t.as_str()) == Some("match") {
                        matches.push(v);
                    }
                }
            }
            ToolOutcome::ok(serde_json::json!({ "matches": matches }).to_string())
        }
        Err(e) => ToolOutcome::err(format!("Grep: rg execution failed: {e}")),
    }
}

fn tool_glob(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let pattern = match args.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolOutcome::err("Glob: missing 'pattern' argument"),
    };

    let full_pattern = root.join(pattern);
    let pattern_str = full_pattern.to_string_lossy();

    match glob::glob(&pattern_str) {
        Ok(entries) => {
            let mut paths = vec![];
            for entry in entries.flatten() {
                // Strip the root prefix for relative output.
                let display = entry
                    .strip_prefix(root)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| entry.to_string_lossy().into_owned());
                paths.push(display);
            }
            ToolOutcome::ok(serde_json::json!({ "paths": paths }).to_string())
        }
        Err(e) => ToolOutcome::err(format!("Glob: invalid pattern: {e}")),
    }
}

async fn tool_shell(
    root: &Path,
    args: &serde_json::Value,
    registry: &TaskRegistry,
    session_id: &str,
    kind: &str,
    extra_env: &[(String, String)],
) -> ToolOutcome {
    // Shell runs arbitrary commands with the daemon's OS user privileges.
    // Access is controlled by session authentication and worktree containment for file paths.
    // The command itself is not further restricted — callers are assumed authenticated and trusted.
    let command = match args.get("command").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return ToolOutcome::err("Shell: missing 'command' argument"),
    };
    let block_until_ms = args
        .get("block_until_ms")
        .and_then(|v| v.as_i64())
        .unwrap_or(30_000); // default: block 30s

    let root_owned = root.to_path_buf();

    if block_until_ms == 0 {
        // Detached background job — spawn a task with a combined output channel.
        let ch = TaskChannel::output_only("0", "combined", ChannelKind::Combined);
        let handle = registry
            .spawn(
                ShellTaskBody {
                    command,
                    root: root_owned,
                    env: extra_env.to_vec(),
                },
                kind,
                session_id,
                vec![ch],
            )
            .await;
        let task_id = handle.id.0.clone();
        return ToolOutcome {
            result_json: serde_json::json!({ "job_id": &task_id }).to_string(),
            is_error: false,
            error_message: String::new(),
            job_id: task_id,
            job_running: true,
        };
    }

    // Blocking execution with timeout.
    let timeout = Duration::from_millis(block_until_ms as u64);
    let fut = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(root)
        .envs(extra_env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .output();

    let outcome = match tokio::time::timeout(timeout, fut).await {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            let exit_code = out.status.code().unwrap_or(-1);
            ToolOutcome::ok(
                serde_json::json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": exit_code,
                })
                .to_string(),
            )
        }
        Ok(Err(e)) => ToolOutcome::err(format!("Shell: spawn failed: {e}")),
        Err(_) => ToolOutcome::err(format!("Shell: timed out after {}ms", block_until_ms)),
    };

    let task_id = register_sync_task(registry, session_id, kind, &outcome).await;
    let mut o = outcome;
    o.job_id = task_id;
    o
}

async fn tool_await(args: &serde_json::Value, registry: &TaskRegistry) -> ToolOutcome {
    // Accept both "job_id" (canonical) and "task_id" (alias used by some callers).
    let job_id = args
        .get("job_id")
        .or_else(|| args.get("task_id"))
        .and_then(|v| v.as_str());

    let job_id = match job_id {
        Some(j) => j,
        None => return ToolOutcome::err("Await: missing 'job_id' argument"),
    };

    let timeout_ms = args
        .get("timeout_ms")
        .or_else(|| args.get("block_until_ms"))
        .and_then(|v| v.as_i64())
        .unwrap_or(30_000);

    let handle = match registry.get_by_str(job_id).await {
        Some(h) => h,
        None => {
            return ToolOutcome {
                result_json: serde_json::json!({ "error": format!("job '{}' not found", job_id) })
                    .to_string(),
                is_error: true,
                error_message: format!("job '{}' not found", job_id),
                job_id: String::new(),
                job_running: false,
            }
        }
    };

    let mut status_rx = handle.status_watch();
    let timeout = Duration::from_millis(timeout_ms as u64);

    let completed = tokio::time::timeout(timeout, async {
        loop {
            if status_rx.borrow().is_terminal() {
                return true;
            }
            if status_rx.changed().await.is_err() {
                return true;
            }
        }
    })
    .await
    .unwrap_or(false);

    // Read combined output from channel "0" (set by background Shell body).
    let stdout = handle
        .channel("0")
        .map(|ch| String::from_utf8_lossy(&ch.replay_capture()).into_owned())
        .unwrap_or_default();

    let exit_code = match handle.status() {
        TaskStatus::Completed { exit_code } => exit_code,
        _ => None,
    };

    ToolOutcome {
        result_json: serde_json::json!({
            "stdout": stdout,
            "exit_code": exit_code,
            "completed": completed,
        })
        .to_string(),
        is_error: false,
        error_message: String::new(),
        job_id: job_id.to_string(),
        job_running: !completed,
    }
}

fn tool_read_lints() -> ToolOutcome {
    ToolOutcome::ok(
        serde_json::json!({
            "lints": [],
            "note": "ReadLints: no linter configured in this environment"
        })
        .to_string(),
    )
}

async fn tool_semantic_search(root: &Path, args: &serde_json::Value) -> ToolOutcome {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return ToolOutcome::err("SemanticSearch: missing 'query' argument"),
    };

    let output = tokio::process::Command::new("rg")
        .args(["--json", "-e", query, "."])
        .current_dir(root)
        .output()
        .await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut results: Vec<serde_json::Value> = vec![];
            for line in stdout.lines() {
                if results.len() >= 10 {
                    break;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    if v.get("type").and_then(|t| t.as_str()) == Some("match") {
                        results.push(v);
                    }
                }
            }
            ToolOutcome::ok(
                serde_json::json!({
                    "results": results,
                    "note": "SemanticSearch: ripgrep-backed fallback"
                })
                .to_string(),
            )
        }
        Err(e) => ToolOutcome::err(format!("SemanticSearch: rg execution failed: {e}")),
    }
}

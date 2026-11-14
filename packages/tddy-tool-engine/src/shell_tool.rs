use crate::contained_shell;
use crate::shell::Shell;
use crate::ToolOutcome;
use async_trait::async_trait;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tddy_task::ChannelKind;
use tddy_task::TaskChannel;
use tddy_task::{TaskBody, TaskContext, TaskRegistry, TaskStatus};

/// Background shell job body — runs a shell command and writes combined output to channel "0".
pub(crate) struct ShellTaskBody {
    pub(crate) command: String,
    pub(crate) root: PathBuf,
    /// Extra environment variables set on the spawned shell (in addition to the inherited env).
    pub(crate) env: Vec<(String, String)>,
}

#[async_trait]
impl TaskBody for ShellTaskBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        // No budget: a background job is awaited through the registry, not timed out here.
        let result =
            contained_shell::run_contained(&self.command, &self.root, &self.env, None).await;

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
                message: format!("Shell: {e}"),
            },
        }
    }
}

/// Wrap an already-completed inline tool invocation as a terminal task for observability.
///
/// Returns the new `TaskId` string so callers can populate `ToolOutcome.job_id`.
pub(crate) async fn register_sync_task(
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

pub(crate) async fn tool_shell(
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
        // default: block 30s. Mirrored by `tddy_tools::session_tool_client::
        // REMOTE_ENGINE_DEFAULT_BLOCK_MS`, whose remote ceiling is pinned *below* it by a
        // compile-time assert — lowering this without lowering that ceiling breaks the build there.
        .unwrap_or(30_000);

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

    // Blocking execution: the budget bounds the command itself, not merely the wait for it.
    let budget = Duration::from_millis(block_until_ms as u64);
    let outcome =
        match contained_shell::run_contained(&command, root, extra_env, Some(budget)).await {
            Ok(out) => {
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
            Err(e) => ToolOutcome::err(format!("Shell: {e}")),
        };

    let task_id = register_sync_task(registry, session_id, kind, &outcome).await;
    let mut o = outcome;
    o.job_id = task_id;
    o
}

pub(crate) async fn tool_await(args: &serde_json::Value, registry: &TaskRegistry) -> ToolOutcome {
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
        // Mirrored by `tddy_tools::session_tool_client::REMOTE_ENGINE_DEFAULT_BLOCK_MS` (the crates
        // cannot import each other), whose remote ceiling is pinned *below* it by a compile-time
        // assert — lowering this without lowering that ceiling breaks the build there.
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

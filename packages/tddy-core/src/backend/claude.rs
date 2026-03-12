//! Claude Code CLI backend implementation.

use super::{Goal, InvokeRequest, InvokeResponse};
use crate::error::BackendError;
use crate::permission;
use crate::stream;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// Resolve binary path for logging (which-like). Returns path as string for display.
pub(crate) fn which_binary(binary: &Path) -> String {
    let name = binary.to_string_lossy();
    if name.contains('/') || name.contains('\\') {
        if let Ok(canon) = std::fs::canonicalize(binary) {
            return canon.display().to_string();
        }
        return name.to_string();
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(&*name);
            if candidate.is_file() {
                if let Ok(canon) = std::fs::canonicalize(&candidate) {
                    return canon.display().to_string();
                }
                return candidate.display().to_string();
            }
        }
    }
    format!("{} (not found in PATH)", name)
}

/// Type for progress callback (tool activity, task events).
type ProgressCallback = Option<Arc<Mutex<Box<dyn FnMut(&stream::ProgressEvent) + Send>>>>;

/// Claude-specific permission mode (maps from Goal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Plan,
    Default,
    AcceptEdits,
}

/// Claude-specific config derived from InvokeRequest (permission_mode, allowlist, etc.).
#[derive(Debug, Clone)]
pub struct ClaudeInvokeConfig {
    pub permission_mode: PermissionMode,
    pub allowed_tools: Vec<String>,
    pub permission_prompt_tool: Option<String>,
    pub mcp_config_path: Option<PathBuf>,
}

/// Build the argument list for the Claude Code CLI (excluding the binary path).
/// Exposed for testing to verify correct command construction.
///
/// When `system_prompt_path` is `Some`, uses `--append-system-prompt-file` with that path
/// (avoids argument length limits and parsing issues). When `None` and `request.system_prompt`
/// is `Some`, uses `--append-system-prompt` with inline content.
///
/// Always adds `--output-format stream-json` for NDJSON stream processing.
/// When `session_id` is set: `--session-id <id>` (first call) or `--resume <id>` (followup).
pub fn build_claude_args(
    request: &InvokeRequest,
    config: &ClaudeInvokeConfig,
    system_prompt_path: Option<&std::path::Path>,
) -> Vec<String> {
    // Prompt must come immediately after -p per CLI docs: claude -p "query"
    let mut args = vec![
        "-p".to_string(),
        request.prompt.clone(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
    ];

    match config.permission_mode {
        PermissionMode::Plan => {
            args.push("--permission-mode".to_string());
            args.push("plan".to_string());
        }
        PermissionMode::AcceptEdits => {
            args.push("--permission-mode".to_string());
            args.push("acceptEdits".to_string());
        }
        PermissionMode::Default => {}
    }

    if let Some(ref model) = request.model {
        args.push("--model".to_string());
        args.push(model.clone());
    }

    if let Some(ref session) = request.session {
        match session {
            super::SessionMode::Fresh(id) => {
                args.push("--session-id".to_string());
                args.push(id.clone());
            }
            super::SessionMode::Resume(id) => {
                args.push("--resume".to_string());
                args.push(id.clone());
            }
        }
    }

    if let Some(path) = system_prompt_path {
        args.push("--append-system-prompt-file".to_string());
        args.push(path.to_string_lossy().to_string());
    } else if let Some(ref sys_prompt) = request.system_prompt {
        args.push("--append-system-prompt".to_string());
        args.push(sys_prompt.clone());
    }

    if !config.allowed_tools.is_empty() {
        for tool in &config.allowed_tools {
            args.push("--allowedTools".to_string());
            args.push(tool.clone());
        }
    }

    if let Some(ref tool_name) = config.permission_prompt_tool {
        args.push("--permission-prompt-tool".to_string());
        args.push(tool_name.clone());
    }

    if let Some(ref mcp_path) = config.mcp_config_path {
        args.push("--mcp-config".to_string());
        args.push(mcp_path.to_string_lossy().to_string());
    }

    args
}

/// Resolve tddy-tools binary path (next to current executable, or parent dir for test binaries in deps/).
fn tddy_tools_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    #[cfg(windows)]
    let name = "tddy-tools.exe";
    #[cfg(not(windows))]
    let name = "tddy-tools";
    // Try same dir first (tddy-coder and tddy-tools in target/debug/)
    let path = dir.join(name);
    if path.is_file() {
        return path.canonicalize().ok().or(Some(path));
    }
    // Fallback: parent dir (test binary in target/debug/deps/)
    if let Some(parent) = dir.parent() {
        let path = parent.join(name);
        if path.is_file() {
            return path.canonicalize().ok().or(Some(path));
        }
        return Some(parent.join(name));
    }
    Some(dir.join(name))
}

/// Create a temporary MCP config file registering tddy-tools. Returns path on success.
fn create_mcp_config_temp_file() -> Option<PathBuf> {
    let tddy_tools = tddy_tools_path()?;
    let tddy_tools_str = tddy_tools.to_string_lossy();
    let config = serde_json::json!({
        "mcpServers": {
            "tddy-tools": {
                "command": tddy_tools_str,
                "args": ["--mcp"]
            }
        }
    });
    let tmp = std::env::temp_dir().join(format!(
        "tddy-mcp-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&tmp, config.to_string()).ok()?;
    Some(tmp)
}

fn goal_to_claude_config(request: &InvokeRequest) -> ClaudeInvokeConfig {
    let (permission_mode, mut allowed_tools) = match request.goal {
        Goal::Plan => (PermissionMode::Plan, permission::plan_allowlist()),
        Goal::AcceptanceTests | Goal::Red | Goal::Green => (
            PermissionMode::AcceptEdits,
            permission::acceptance_tests_allowlist(),
        ),
        Goal::Demo => (PermissionMode::AcceptEdits, permission::demo_allowlist()),
        Goal::Evaluate => (PermissionMode::Plan, permission::evaluate_allowlist()),
        Goal::Validate => (
            PermissionMode::Plan,
            permission::validate_subagents_allowlist(),
        ),
        Goal::Refactor => (
            PermissionMode::AcceptEdits,
            permission::refactor_allowlist(),
        ),
        Goal::UpdateDocs => (
            PermissionMode::AcceptEdits,
            permission::update_docs_allowlist(),
        ),
    };
    if let Some(ref extras) = request.extra_allowed_tools {
        allowed_tools.extend(extras.iter().cloned());
    }
    ClaudeInvokeConfig {
        permission_mode,
        allowed_tools,
        permission_prompt_tool: None,
        mcp_config_path: None,
    }
}

/// Backend that invokes the Claude Code CLI binary.
///
/// Uses `--output-format stream-json` for NDJSON stream processing.
/// Supports session continuity via `--session-id` / `--resume`.
pub struct ClaudeCodeBackend {
    binary_path: PathBuf,
    progress_callback: ProgressCallback,
}

impl std::fmt::Debug for ClaudeCodeBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeCodeBackend")
            .field("binary_path", &self.binary_path)
            .field(
                "progress_callback",
                &if self.progress_callback.is_some() {
                    "Some(..)"
                } else {
                    "None"
                },
            )
            .finish()
    }
}

impl Clone for ClaudeCodeBackend {
    fn clone(&self) -> Self {
        Self {
            binary_path: self.binary_path.clone(),
            progress_callback: self.progress_callback.clone(),
        }
    }
}

impl Default for ClaudeCodeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeCodeBackend {
    /// Create a new backend using the default `claude` binary from PATH.
    pub fn new() -> Self {
        Self {
            binary_path: PathBuf::from("claude"),
            progress_callback: None,
        }
    }

    /// Create a backend with a custom binary path.
    #[must_use]
    pub fn with_path(path: PathBuf) -> Self {
        Self {
            binary_path: path,
            progress_callback: None,
        }
    }

    /// Set a callback invoked for each progress event (tool use, task started, task progress).
    #[must_use]
    pub fn with_progress<F>(mut self, f: F) -> Self
    where
        F: FnMut(&stream::ProgressEvent) + Send + 'static,
    {
        self.progress_callback = Some(Arc::new(Mutex::new(Box::new(f))));
        self
    }
}

#[async_trait::async_trait]
impl super::CodingBackend for ClaudeCodeBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let self_clone = self.clone();
        tokio::task::spawn_blocking(move || self_clone.invoke_sync(request))
            .await
            .map_err(|e| BackendError::InvocationFailed(e.to_string()))?
    }

    fn name(&self) -> &str {
        "claude"
    }
}

impl ClaudeCodeBackend {
    fn invoke_sync(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let (system_prompt_path, cleanup_temp) = if let Some(ref path) = request.system_prompt_path
        {
            (Some(path.clone()), false)
        } else if let Some(ref sys_prompt) = request.system_prompt {
            let tmp = std::env::temp_dir().join(format!(
                "tddy-sys-{}-{}.txt",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::write(&tmp, sys_prompt).map_err(|e| {
                BackendError::InvocationFailed(format!("failed to write system prompt file: {}", e))
            })?;
            (Some(tmp), true)
        } else {
            (None, false)
        };

        struct CleanupGuard(PathBuf);
        impl Drop for CleanupGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = if cleanup_temp {
            system_prompt_path.as_ref().map(|p| CleanupGuard(p.clone()))
        } else {
            None
        };

        let mut config = goal_to_claude_config(&request);

        // Plan goal: create MCP config so Claude Code routes permission requests to tddy-tools.
        let _mcp_cleanup: Option<CleanupGuard> = if request.goal == Goal::Plan {
            if let Some(mcp_path) = create_mcp_config_temp_file() {
                config.permission_prompt_tool =
                    Some("mcp__tddy-tools__approval_prompt".to_string());
                config.mcp_config_path = Some(mcp_path.clone());
                Some(CleanupGuard(mcp_path))
            } else {
                None
            }
        } else {
            None
        };

        let args = build_claude_args(&request, &config, system_prompt_path.as_deref());
        let mut cmd = Command::new(&self.binary_path);
        if let Some(ref wd) = request.working_dir {
            cmd.current_dir(wd);
        }
        for arg in &args {
            cmd.arg(arg);
        }

        let resolved = which_binary(&self.binary_path);
        let cwd_str = request
            .working_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "(unknown)".into())
            });
        let cmd_str = super::format_command_for_log(&self.binary_path, &args, 200);
        log::debug!("[tddy-coder] Claude backend command: {}", cmd_str);
        log::debug!(
            "[tddy-coder] Claude backend spawning: {} (resolved: {})",
            self.binary_path.display(),
            resolved
        );
        log::debug!("[tddy-coder] cwd: {}", cwd_str);
        log::debug!(
            "[tddy-coder] goal: {:?}, model: {:?}, session: {:?}",
            request.goal,
            request.model,
            request.session
        );
        log::debug!(
            "[tddy-coder] prompt ({} bytes): {}",
            request.prompt.len(),
            &request.prompt[..request.prompt.len().min(500)]
        );
        if let Some(ref sp) = request.system_prompt {
            log::debug!(
                "[tddy-coder] system_prompt ({} bytes): {}",
                sp.len(),
                &sp[..sp.len().min(500)]
            );
        }
        if let Some(ref sp_path) = request.system_prompt_path {
            log::debug!("[tddy-coder] system_prompt_path: {}", sp_path.display());
        }

        cmd.env("PATH", super::path_with_exe_dir());
        if let Some(ref p) = request.socket_path {
            cmd.env("TDDY_SOCKET", p);
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(if request.inherit_stdin {
            Stdio::inherit()
        } else {
            Stdio::null()
        });

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                BackendError::BinaryNotFound(self.binary_path.to_string_lossy().to_string())
            } else {
                BackendError::InvocationFailed(e.to_string())
            }
        })?;
        super::set_child_pid(child.id());

        let stdout_handle = child
            .stdout
            .take()
            .ok_or_else(|| BackendError::InvocationFailed("failed to capture stdout".into()))?;

        let stderr_handle = child.stderr.take();
        let stderr_thread = stderr_handle.map(|h| {
            std::thread::spawn(move || {
                let mut buf = String::new();
                let _ = std::io::Read::read_to_string(&mut BufReader::new(h), &mut buf);
                buf
            })
        });

        let progress_sink = request.progress_sink.clone();
        let instance_cb = self.progress_callback.clone();
        let mut on_progress = move |ev: &stream::ProgressEvent| {
            if let Some(ref sink) = progress_sink {
                sink.emit(ev);
            } else if let Some(ref cb) = instance_cb {
                if let Ok(mut f) = cb.lock() {
                    f(ev);
                }
            }
        };

        let skip_until_line = if request.session.as_ref().is_some_and(|s| s.is_resume()) {
            request
                .conversation_output_path
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|c| {
                    c.lines()
                        .filter(|l| {
                            let t = l.trim();
                            !t.is_empty() && !t.contains("\"type\":\"tddy-request\"")
                        })
                        .count()
                })
                .unwrap_or(0)
        } else {
            0
        };

        let agent_output = request.agent_output;
        let agent_output_sink = request.agent_output_sink.clone();
        let mut on_raw_output = move |s: &str| {
            if agent_output {
                if let Some(ref sink) = agent_output_sink {
                    sink.emit(s);
                } else if std::env::var("TDDY_QUIET").is_err() {
                    eprint!("{}", s);
                }
            }
        };

        let mut on_debug_line = |line: &str| {
            if request.debug {
                log::debug!("[tddy-coder debug] {}", line);
            }
        };

        let mut conv_file = if let Some(ref path) = request.conversation_output_path {
            Some(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| {
                        BackendError::InvocationFailed(format!(
                            "failed to open conversation output {}: {}",
                            path.display(),
                            e
                        ))
                    })?,
            )
        } else {
            None
        };

        if let Some(ref mut f) = conv_file {
            let sys_prompt_content = system_prompt_path
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .or_else(|| request.system_prompt.clone());
            let (session_id, is_resume) = request
                .session
                .as_ref()
                .map(|s| (s.session_id().to_string(), s.is_resume()))
                .unwrap_or((String::new(), false));
            let request_entry = serde_json::json!({
                "type": "tddy-request",
                "goal": format!("{:?}", request.goal),
                "prompt": request.prompt,
                "system_prompt": sys_prompt_content,
                "model": request.model,
                "session_id": session_id,
                "is_resume": is_resume,
            });
            let _ = writeln!(f, "{}", request_entry);
            let _ = f.flush();
        }

        let mut first_line_logged = false;
        let mut on_conversation_line = |line: &str| {
            if !first_line_logged {
                first_line_logged = true;
                let preview = if line.len() > 150 {
                    format!("{}...", &line[..150])
                } else {
                    line.to_string()
                };
                log::debug!("[tddy-coder] first stream line (format hint): {}", preview);
            }
            if let Some(ref mut f) = conv_file {
                let _ = writeln!(f, "{}", line);
                let _ = f.flush();
            }
        };

        let reader = BufReader::new(stdout_handle);
        let stream_result = stream::process_ndjson_stream(
            reader,
            &mut on_progress,
            &mut on_raw_output,
            if request.debug {
                Some(&mut on_debug_line)
            } else {
                None
            },
            if request.conversation_output_path.is_some() {
                Some(&mut on_conversation_line)
            } else {
                None
            },
            skip_until_line,
        )
        .map_err(|e| BackendError::InvocationFailed(format!("stream parse error: {}", e)))?;

        let stderr_buf = stderr_thread
            .and_then(|j| j.join().ok())
            .unwrap_or_default();

        let status = child
            .wait()
            .map_err(|e| BackendError::InvocationFailed(e.to_string()))?;
        super::clear_child_pid();
        let exit_code = status.code().unwrap_or(-1);
        log::debug!(
            "[tddy-coder] Claude process exited with code {} (goal: {:?}, session_id: {:?})",
            exit_code,
            request.goal,
            request.session
        );

        if exit_code != 0 {
            // When plan goal produced valid structured output, treat exit 1 as non-fatal.
            // CLI may exit 1 after session/ExitPlanMode issues despite successful output.
            let has_plan_output = request.goal == Goal::Plan
                && stream_result.result_text.contains("<structured-response");
            if has_plan_output {
                log::debug!(
                    "[tddy-coder] CLI exited with code {} but plan output present; treating as success",
                    exit_code
                );
            } else {
                let detail = if !stream_result.stream_errors.is_empty() {
                    stream_result.stream_errors.join("; ")
                } else if !stderr_buf.trim().is_empty() {
                    stderr_buf.trim().to_string()
                } else {
                    String::new()
                };
                let msg = if detail.is_empty() {
                    format!("Claude Code CLI exited with code {}", exit_code)
                } else {
                    format!("Claude Code CLI exited with code {}: {}", exit_code, detail)
                };
                return Err(BackendError::InvocationFailed(msg));
            }
        }

        let raw_stream = if stream_result.raw_lines.is_empty() {
            None
        } else {
            Some(stream_result.raw_lines.join("\n"))
        };

        let stderr = if stream_result.raw_lines.is_empty() && !stderr_buf.trim().is_empty() {
            Some(stderr_buf)
        } else {
            None
        };

        Ok(InvokeResponse {
            output: stream_result.result_text,
            exit_code,
            session_id: if stream_result.session_id.is_empty() {
                None
            } else {
                Some(stream_result.session_id)
            },
            questions: stream_result.questions,
            raw_stream,
            stderr,
        })
    }
}

//! Claude Code CLI backend implementation.

use super::{Goal, InvokeRequest, InvokeResponse};
use crate::error::BackendError;
use crate::permission;
use crate::stream;
use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

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

    if let Some(ref sid) = request.session_id {
        if request.is_resume {
            args.push("--resume".to_string());
        } else {
            args.push("--session-id".to_string());
        }
        args.push(sid.clone());
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

fn goal_to_claude_config(request: &InvokeRequest) -> ClaudeInvokeConfig {
    let (permission_mode, mut allowed_tools) = match request.goal {
        Goal::Plan => (PermissionMode::Plan, permission::plan_allowlist()),
        Goal::AcceptanceTests | Goal::Red | Goal::Green => (
            PermissionMode::AcceptEdits,
            permission::acceptance_tests_allowlist(),
        ),
        Goal::Validate => (PermissionMode::Plan, permission::validate_allowlist()),
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

impl super::CodingBackend for ClaudeCodeBackend {
    fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
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

        let config = goal_to_claude_config(&request);
        let args = build_claude_args(&request, &config, system_prompt_path.as_deref());
        let mut cmd = Command::new(&self.binary_path);
        if let Some(ref wd) = request.working_dir {
            cmd.current_dir(wd);
        }
        for arg in &args {
            cmd.arg(arg);
        }

        if request.debug {
            let cwd = request
                .working_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| {
                    std::env::current_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| "(unknown)".into())
                });
            eprintln!("[tddy-coder debug] cwd: {}", cwd);
            eprintln!(
                "[tddy-coder debug] cmd: {} {}",
                self.binary_path.display(),
                args.join(" ")
            );
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

        let mut on_progress = |ev: &stream::ProgressEvent| {
            if let Some(ref cb) = self.progress_callback {
                if let Ok(mut f) = cb.lock() {
                    f(ev);
                }
            }
        };

        let mut on_raw_output = |s: &str| {
            if request.agent_output {
                eprint!("{}", s);
            }
        };

        let mut on_debug_line = |line: &str| {
            if request.debug {
                eprintln!("[tddy-coder debug] {}", line);
            }
        };

        let mut conv_file = if let Some(ref path) = request.conversation_output_path {
            Some(
                std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
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

        let mut on_conversation_line = |line: &str| {
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
        )
        .map_err(|e| BackendError::InvocationFailed(format!("stream parse error: {}", e)))?;

        let stderr_buf = stderr_thread
            .and_then(|j| j.join().ok())
            .unwrap_or_default();

        let status = child
            .wait()
            .map_err(|e| BackendError::InvocationFailed(e.to_string()))?;
        let exit_code = status.code().unwrap_or(-1);

        if exit_code != 0 {
            let msg = if stderr_buf.trim().is_empty() {
                format!("Claude Code CLI exited with code {}", exit_code)
            } else {
                format!(
                    "Claude Code CLI exited with code {}: {}",
                    exit_code,
                    stderr_buf.trim()
                )
            };
            return Err(BackendError::InvocationFailed(msg));
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

        // Fallback: when agent outputs questions in text (no AskUserQuestion tool events), parse from output
        let questions = if stream_result.questions.is_empty() {
            stream::parse_clarification_questions_from_text(&stream_result.result_text)
        } else {
            stream_result.questions
        };

        Ok(InvokeResponse {
            output: stream_result.result_text,
            exit_code,
            session_id: if stream_result.session_id.is_empty() {
                None
            } else {
                Some(stream_result.session_id)
            },
            questions,
            raw_stream,
            stderr,
        })
    }

    fn name(&self) -> &str {
        "claude"
    }
}

//! Claude Code CLI backend implementation.

use super::{InvokeRequest, InvokeResponse, PermissionMode};
use crate::error::BackendError;
use crate::stream;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// Type for progress callback (tool activity, task events).
type ProgressCallback = Option<Arc<Mutex<Box<dyn FnMut(&stream::ProgressEvent) + Send>>>>;

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
    system_prompt_path: Option<&std::path::Path>,
) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
    ];

    match request.permission_mode {
        PermissionMode::Plan => {
            args.push("--permission-mode".to_string());
            args.push("plan".to_string());
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

    args.push(request.prompt.clone());
    args
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
        let system_prompt_path = if let Some(ref sys_prompt) = request.system_prompt {
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
            Some(tmp)
        } else {
            None
        };

        struct CleanupGuard(PathBuf);
        impl Drop for CleanupGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _cleanup = system_prompt_path.as_ref().map(|p| CleanupGuard(p.clone()));

        let args = build_claude_args(&request, system_prompt_path.as_deref());
        let mut cmd = Command::new(&self.binary_path);
        for arg in &args {
            cmd.arg(arg);
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::null());

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

        let reader = BufReader::new(stdout_handle);
        let stream_result =
            stream::process_ndjson_stream(reader, &mut on_progress, &mut on_raw_output).map_err(
                |e| BackendError::InvocationFailed(format!("stream parse error: {}", e)),
            )?;

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

        Ok(InvokeResponse {
            output: stream_result.result_text,
            exit_code,
            session_id: stream_result.session_id,
            questions: stream_result.questions,
        })
    }
}

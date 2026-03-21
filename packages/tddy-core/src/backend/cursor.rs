//! Cursor agent CLI backend implementation.
//!
//! Invokes the `agent` executable on `PATH` with stream-json output format (`--output-format stream-json`, etc.).
//! Some environments also expose the same flow as `cursor agent`; this backend targets the `agent` binary name.
//! Based on Baker CLI's executeWithCursor.

use super::{Goal, InvokeRequest, InvokeResponse};
use crate::error::BackendError;
use crate::stream::cursor;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// Type for progress callback.
type ProgressCallback = Option<Arc<Mutex<Box<dyn FnMut(&crate::stream::ProgressEvent) + Send>>>>;

/// Backend that invokes the Cursor agent CLI binary.
pub struct CursorBackend {
    binary_path: PathBuf,
    progress_callback: ProgressCallback,
}

impl std::fmt::Debug for CursorBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CursorBackend")
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

impl Clone for CursorBackend {
    fn clone(&self) -> Self {
        Self {
            binary_path: self.binary_path.clone(),
            progress_callback: self.progress_callback.clone(),
        }
    }
}

impl Default for CursorBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorBackend {
    /// Default executable name on `PATH` for [`CursorBackend::new`] (also used in logs / `BinaryNotFound`).
    pub const DEFAULT_CLI_BINARY: &'static str = "agent";

    /// Create a new backend using the default `agent` binary from PATH.
    pub fn new() -> Self {
        Self {
            binary_path: PathBuf::from(Self::DEFAULT_CLI_BINARY),
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

    /// Set a callback invoked for each progress event.
    #[must_use]
    pub fn with_progress<F>(mut self, f: F) -> Self
    where
        F: FnMut(&crate::stream::ProgressEvent) + Send + 'static,
    {
        self.progress_callback = Some(Arc::new(Mutex::new(Box::new(f))));
        self
    }
}

#[async_trait::async_trait]
impl super::CodingBackend for CursorBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let self_clone = self.clone();
        tokio::task::spawn_blocking(move || self_clone.invoke_sync(request))
            .await
            .map_err(|e| BackendError::InvocationFailed(e.to_string()))?
    }

    fn name(&self) -> &str {
        "cursor"
    }
}

/// Builds argv for `cursor agent` (excluding the binary path). Used by [`CursorBackend::invoke_sync`] and tests.
pub(crate) fn build_cursor_cli_args(request: &InvokeRequest, prompt: &str) -> Vec<String> {
    let mut args = Vec::new();
    if request.goal == Goal::Plan {
        args.push("--plan".to_string());
    }
    if let Some(ref session) = request.session {
        match session {
            super::SessionMode::Fresh(_) => {
                // Cursor `agent` does not document `--session-id`; new chats omit session flags.
            }
            super::SessionMode::Resume(id) => {
                args.push("--resume".to_string());
                args.push(id.clone());
            }
        }
    }
    args.push("-p".to_string());
    args.push(prompt.to_string());
    if let Some(ref m) = request.model {
        args.push("--model".to_string());
        args.push(m.clone());
    }
    args.push("--output-format".to_string());
    args.push("stream-json".to_string());
    args.push("--stream-partial-output".to_string());
    args.push("--force".to_string());
    args.push("--trust".to_string());
    args
}

impl CursorBackend {
    fn invoke_sync(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        // Cursor CLI has no --system-prompt; prepend system content to user prompt.
        let system_content: Option<String> = if let Some(ref path) = request.system_prompt_path {
            Some(std::fs::read_to_string(path).map_err(|e| {
                BackendError::InvocationFailed(format!(
                    "failed to read system_prompt_path {}: {}",
                    path.display(),
                    e
                ))
            })?)
        } else {
            request.system_prompt.clone()
        };

        let prompt = match system_content {
            Some(ref sys) => format!("{}\n\n{}", sys, request.prompt),
            None => request.prompt.clone(),
        };

        let args = build_cursor_cli_args(&request, &prompt);

        let mut cmd = Command::new(&self.binary_path);
        if let Some(ref wd) = request.working_dir {
            cmd.current_dir(wd);
        }
        for arg in &args {
            cmd.arg(arg);
        }

        let resolved = super::claude::which_binary(&self.binary_path);
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
        log::info!("[tddy-coder] Cursor backend command: {}", cmd_str);
        log::debug!(
            "[tddy-coder] Cursor backend spawning: {} (resolved: {})",
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
        if let Some(ref sys) = system_content {
            log::debug!(
                "[tddy-coder] system_prompt ({} bytes): {}",
                sys.len(),
                &sys[..sys.len().min(500)]
            );
        }

        cmd.env("PATH", super::path_with_exe_dir());
        if let Some(ref p) = request.socket_path {
            cmd.env("TDDY_SOCKET", p);
        }
        if let Some(ref p) = request.working_dir {
            cmd.env("TDDY_REPO_DIR", p);
        }
        if let Some(ref p) = request.plan_dir {
            cmd.env("TDDY_PLAN_DIR", p);
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
                let _ = std::io::Read::read_to_string(&mut std::io::BufReader::new(h), &mut buf);
                buf
            })
        });

        let progress_sink = request.progress_sink.clone();
        let instance_cb = self.progress_callback.clone();
        let mut on_progress = move |ev: &crate::stream::ProgressEvent| {
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
            let (session_id, is_resume) = request
                .session
                .as_ref()
                .map(|s| (s.session_id().to_string(), s.is_resume()))
                .unwrap_or((String::new(), false));
            let request_entry = serde_json::json!({
                "type": "tddy-request",
                "goal": format!("{:?}", request.goal),
                "prompt": request.prompt,
                "system_prompt": system_content,
                "model": request.model,
                "session_id": session_id,
                "is_resume": is_resume,
            });
            let _ = writeln!(f, "{}", request_entry);
            let _ = f.flush();
        }

        let mut on_conversation_line = |line: &str| {
            if let Some(ref mut f) = conv_file {
                let _ = writeln!(f, "{}", line);
                let _ = f.flush();
            }
        };

        let reader = std::io::BufReader::new(stdout_handle);
        let stream_result = cursor::process_cursor_stream(
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
            "[tddy-coder] Cursor process exited with code {} (goal: {:?}, session_id: {:?})",
            exit_code,
            request.goal,
            request.session
        );

        if exit_code != 0 {
            log::warn!(
                "[tddy-coder] Cursor backend command failed (exit {}): {}",
                exit_code,
                cmd_str
            );
            if !stderr_buf.trim().is_empty() {
                log::warn!("[tddy-coder] Cursor backend stderr: {}", stderr_buf.trim());
            }
            let msg = if stderr_buf.trim().is_empty() {
                format!(
                    "Cursor agent exited with code {} (no stderr from CLI). Invoked: {}",
                    exit_code, cmd_str
                )
            } else {
                format!(
                    "Cursor agent exited with code {}: {}\nInvoked: {}",
                    exit_code,
                    stderr_buf.trim(),
                    cmd_str
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

        if let Some(ref sink) = request.progress_sink {
            sink.emit(&crate::stream::ProgressEvent::AgentExited {
                exit_code,
                goal: request.goal.submit_key().to_string(),
            });
        }

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

#[cfg(test)]
mod tests {
    use super::build_cursor_cli_args;
    use super::CursorBackend;
    use crate::backend::{Goal, InvokeRequest, SessionMode};

    fn minimal_request(goal: Goal, model: Option<&str>, prompt: &str) -> InvokeRequest {
        InvokeRequest {
            prompt: prompt.to_string(),
            system_prompt: None,
            system_prompt_path: None,
            goal,
            model: model.map(std::string::ToString::to_string),
            session: None,
            working_dir: None,
            debug: false,
            agent_output: false,
            agent_output_sink: None,
            progress_sink: None,
            conversation_output_path: None,
            inherit_stdin: false,
            extra_allowed_tools: None,
            socket_path: None,
            plan_dir: None,
        }
    }

    #[test]
    fn build_args_includes_model_when_set() {
        let request = minimal_request(Goal::Plan, Some("composer-2"), "test");
        let args = build_cursor_cli_args(&request, "test prompt");
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"composer-2".to_string()));
    }

    #[test]
    fn build_args_omits_model_when_none() {
        let request = minimal_request(Goal::Plan, None, "test");
        let args = build_cursor_cli_args(&request, "test prompt");
        assert!(!args.contains(&"--model".to_string()));
    }

    #[test]
    fn build_args_plan_includes_plan_flag() {
        let request = minimal_request(Goal::Plan, None, "x");
        let args = build_cursor_cli_args(&request, "p");
        assert!(args.contains(&"--plan".to_string()));
    }

    #[test]
    fn build_args_session_fresh_omits_session_flags() {
        let mut request = minimal_request(Goal::Red, None, "x");
        request.session = Some(SessionMode::Fresh("sid-1".to_string()));
        let args = build_cursor_cli_args(&request, "p");
        assert!(!args.iter().any(|a| a == "--session-id"));
        assert!(!args.iter().any(|a| a == "--resume"));
    }

    #[test]
    fn build_args_session_resume_includes_resume() {
        let mut request = minimal_request(Goal::Red, None, "x");
        request.session = Some(SessionMode::Resume("resume-id".to_string()));
        let args = build_cursor_cli_args(&request, "p");
        assert!(args.iter().any(|a| a == "--resume"));
        assert!(args.contains(&"resume-id".to_string()));
    }

    #[test]
    fn default_cursor_cli_binary_on_path_is_agent() {
        let backend = CursorBackend::new();
        assert_eq!(
            backend.binary_path,
            std::path::PathBuf::from("agent"),
            "invokes the Cursor CLI as `agent` on PATH; BinaryNotFound must report agent, not cursor"
        );
    }
}

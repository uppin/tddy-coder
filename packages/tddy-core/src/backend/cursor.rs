//! Cursor agent CLI backend implementation.
//!
//! Spawns `cursor agent` with stream-json output format.
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

impl Default for CursorBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorBackend {
    /// Create a new backend using the default `cursor` binary from PATH.
    pub fn new() -> Self {
        Self {
            binary_path: PathBuf::from("cursor"),
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

impl super::CodingBackend for CursorBackend {
    fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        // validate-refactor spawns subagents via the Agent tool which Cursor does not support.
        // Reject early before any spawn attempt so tests can distinguish this from BinaryNotFound.
        if request.goal == Goal::ValidateRefactor {
            crate::debug_eprintln!(
                "[tddy-coder] CursorBackend: rejecting Goal::ValidateRefactor — not supported on Cursor"
            );
            return Err(BackendError::InvocationFailed(
                "validate-refactor is not supported on the Cursor backend".to_string(),
            ));
        }

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

        let mut args = vec!["agent".to_string()];
        if request.goal == Goal::Plan {
            args.push("--plan".to_string());
        }
        if let (Some(ref sid), true) = (&request.session_id, request.is_resume) {
            args.push("--resume".to_string());
            args.push(sid.clone());
        }
        args.push("-p".to_string());
        args.push(prompt);
        args.push("--output-format".to_string());
        args.push("stream-json".to_string());
        args.push("--stream-partial-output".to_string());
        args.push("--force".to_string());
        args.push("--trust".to_string());

        let mut cmd = Command::new(&self.binary_path);
        if let Some(ref wd) = request.working_dir {
            cmd.current_dir(wd);
        }
        for arg in &args {
            cmd.arg(arg);
        }

        // Always log spawn for debugging backend/binary confusion
        let resolved = super::claude::which_binary(&self.binary_path);
        crate::debug_eprintln!(
            "[tddy-coder] Cursor backend spawning: {} (resolved: {})",
            self.binary_path.display(),
            resolved
        );
        crate::debug_eprintln!("[tddy-coder] cmd: cursor {}", args.join(" "));

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
            crate::debug_eprintln!("[tddy-coder debug] cwd: {}", cwd);
            crate::debug_eprintln!(
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

        let mut on_progress = |ev: &crate::stream::ProgressEvent| {
            if let Some(ref cb) = self.progress_callback {
                if let Ok(mut f) = cb.lock() {
                    f(ev);
                }
            }
        };

        let skip_until_line = if request.is_resume {
            request
                .conversation_output_path
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|c| c.lines().filter(|l| !l.trim().is_empty()).count())
                .unwrap_or(0)
        } else {
            0
        };

        let mut on_raw_output = |s: &str| {
            eprint!("{}", s);
        };

        let mut on_debug_line = |line: &str| {
            if request.debug {
                crate::debug_eprintln!("[tddy-coder debug] {}", line);
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

        if exit_code != 0 {
            let msg = if stderr_buf.trim().is_empty() {
                format!("Cursor agent exited with code {}", exit_code)
            } else {
                format!(
                    "Cursor agent exited with code {}: {}",
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

    fn name(&self) -> &str {
        "cursor"
    }
}

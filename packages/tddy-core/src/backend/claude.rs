//! Claude Code CLI backend implementation.

use super::{InvokeRequest, InvokeResponse, PermissionMode};
use crate::error::BackendError;
use std::io::{self, BufReader, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Build the argument list for the Claude Code CLI (excluding the binary path).
/// Exposed for testing to verify correct command construction.
///
/// When `system_prompt_path` is `Some`, uses `--append-system-prompt-file` with that path
/// (avoids argument length limits and parsing issues). When `None` and `request.system_prompt`
/// is `Some`, uses `--append-system-prompt` with inline content.
pub fn build_claude_args(
    request: &InvokeRequest,
    system_prompt_path: Option<&std::path::Path>,
) -> Vec<String> {
    let mut args = vec!["-p".to_string()];

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

/// ANSI dim and reset for styling streamed output.
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_RESET: &str = "\x1b[0m";

/// Backend that invokes the Claude Code CLI binary.
#[derive(Debug)]
pub struct ClaudeCodeBackend {
    binary_path: PathBuf,
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
        }
    }

    /// Create a backend with a custom binary path.
    pub fn with_path(path: PathBuf) -> Self {
        Self { binary_path: path }
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
        let _cleanup = system_prompt_path
            .as_ref()
            .map(|p| CleanupGuard(p.clone()));

        let stream_to_tty = io::stdout().is_terminal();

        let (mut cmd, script_log_path) = if stream_to_tty {
            let claude_args: Vec<String> =
                build_claude_args(&request, system_prompt_path.as_deref());
            let script_log = std::env::temp_dir().join(format!(
                "tddy-script-{}-{}.log",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let mut script_cmd = Command::new("script");
            script_cmd
                .arg("-q")
                .arg(&script_log)
                .arg(&self.binary_path);
            for a in &claude_args {
                script_cmd.arg(a);
            }
            script_cmd.stdout(Stdio::piped());
            script_cmd.stderr(Stdio::piped());
            script_cmd.stdin(Stdio::null());
            (script_cmd, Some(script_log))
        } else {
            let mut c = Command::new(&self.binary_path);
            for arg in build_claude_args(&request, system_prompt_path.as_deref()) {
                c.arg(arg);
            }
            c.stdout(Stdio::piped());
            c.stderr(Stdio::piped());
            (c, None)
        };

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                BackendError::BinaryNotFound(self.binary_path.to_string_lossy().to_string())
            } else {
                BackendError::InvocationFailed(e.to_string())
            }
        })?;

        let (stdout, exit_code) = if let Some(ref log_path) = script_log_path {
            let status = child.wait().map_err(|e| {
                BackendError::InvocationFailed(e.to_string())
            })?;
            let code = status.code().unwrap_or(-1);
            let captured = std::fs::read(log_path).unwrap_or_default();
            let stdout_str = String::from_utf8_lossy(&captured).into_owned();
            let _ = std::fs::remove_file(log_path);
            if stream_to_tty {
                let mut out = io::stdout().lock();
                let _ = write!(out, "{ANSI_DIM}");
                for line in stdout_str.lines() {
                    let _ = write!(out, "  {}\n", line);
                }
                let _ = write!(out, "{ANSI_RESET}");
                let _ = out.flush();
            }
            (stdout_str, code)
        } else if stream_to_tty {
            let stdout_handle = child.stdout.take().ok_or_else(|| {
                BackendError::InvocationFailed("failed to capture stdout".into())
            })?;
            let stderr_handle = child.stderr.take();

            let stderr_thread = stderr_handle.map(|h| {
                std::thread::spawn(move || {
                    let mut buf = String::new();
                    let _ = io::Read::read_to_string(&mut BufReader::new(h), &mut buf);
                    buf
                })
            });

            let mut captured = Vec::new();
            let mut reader = BufReader::new(stdout_handle);
            let mut out = io::stdout().lock();
            let mut chunk = [0u8; 256];
            let mut at_line_start = true;

            let _ = write!(out, "{ANSI_DIM}");
            loop {
                let n = reader.read(&mut chunk).map_err(|e| {
                    BackendError::InvocationFailed(e.to_string())
                })?;
                if n == 0 {
                    break;
                }
                captured.extend_from_slice(&chunk[..n]);

                for &b in &chunk[..n] {
                    if at_line_start {
                        let _ = write!(out, "  ");
                        at_line_start = false;
                    }
                    if b == b'\n' {
                        at_line_start = true;
                    }
                    let _ = out.write_all(&[b]);
                }
                let _ = out.flush();
            }
            let _ = write!(out, "{ANSI_RESET}");

            if let Some(join) = stderr_thread {
                if let Ok(err_buf) = join.join() {
                    if !err_buf.is_empty() {
                        for line in err_buf.lines() {
                            let _ = write!(out, "{ANSI_DIM}  [stderr] {}{ANSI_RESET}\n", line);
                        }
                        let _ = out.flush();
                    }
                }
            }

            let status = child.wait().map_err(|e| {
                BackendError::InvocationFailed(e.to_string())
            })?;
            let code = status.code().unwrap_or(-1);

            let stdout = String::from_utf8_lossy(&captured).into_owned();
            (stdout, code)
        } else {
            let output = child.wait_with_output().map_err(|e| {
                BackendError::InvocationFailed(e.to_string())
            })?;
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);

            if !output.status.success() {
                return Err(BackendError::InvocationFailed(format!(
                    "exit code {}: {}",
                    exit_code, stderr
                )));
            }
            (stdout, exit_code)
        };

        if !stream_to_tty {
            if exit_code != 0 {
                return Err(BackendError::InvocationFailed(format!(
                    "exit code {}",
                    exit_code
                )));
            }
        } else if exit_code != 0 {
            return Err(BackendError::InvocationFailed(format!(
                "Claude Code CLI exited with code {}",
                exit_code
            )));
        }

        Ok(InvokeResponse {
            output: stdout,
            exit_code,
        })
    }
}

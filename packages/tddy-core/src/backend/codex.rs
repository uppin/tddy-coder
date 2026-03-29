//! OpenAI Codex CLI backend (`codex exec`, `codex exec resume <id>`, `--json`).
//!
//! Spawns non-interactive `codex exec` with JSONL on stdout. Maps
//! [`GoalHints::agent_cli_plan_mode`] + [`PermissionHint`] to explicit Codex flags
//! (see [`build_codex_exec_argv`]).

use super::{InvokeRequest, InvokeResponse, PermissionHint};
use crate::error::BackendError;
use crate::stream::codex::parse_codex_jsonl_output;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Backend that invokes the Codex CLI (`codex` on PATH by default).
#[derive(Debug, Clone)]
pub struct CodexBackend {
    binary_path: PathBuf,
}

impl Default for CodexBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexBackend {
    /// Default executable name on `PATH` for [`CodexBackend::new`].
    pub const DEFAULT_CLI_BINARY: &'static str = "codex";

    pub fn new() -> Self {
        Self {
            binary_path: PathBuf::from(Self::DEFAULT_CLI_BINARY),
        }
    }

    #[must_use]
    pub fn with_path(path: PathBuf) -> Self {
        Self { binary_path: path }
    }

    fn invoke_sync(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let merged_prompt = merge_codex_prompt(&request)?;
        let args = build_codex_exec_argv(&request, &merged_prompt);

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
        log::info!("[tddy-codex] command: {}", cmd_str);
        log::debug!(
            "[tddy-codex] spawning binary={} resolved={} cwd={}",
            self.binary_path.display(),
            resolved,
            cwd_str
        );
        log::debug!(
            "[tddy-codex] goal={:?} model={:?} session={:?}",
            request.goal_id,
            request.model,
            request.session
        );

        cmd.env("PATH", super::path_with_exe_dir());
        if let Some(ref p) = request.socket_path {
            cmd.env("TDDY_SOCKET", p);
        }
        if let Some(ref p) = request.working_dir {
            cmd.env("TDDY_REPO_DIR", p);
        }
        if let Some(ref p) = request.session_dir {
            cmd.env("TDDY_SESSION_DIR", p);
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
                "goal": request.hints.display_name,
                "prompt": request.prompt,
                "system_prompt": request.system_prompt,
                "model": request.model,
                "session_id": session_id,
                "is_resume": is_resume,
            });
            let _ = writeln!(f, "{}", request_entry);
            let _ = f.flush();
        }

        let reader = std::io::BufReader::new(stdout_handle);
        let mut raw_lines: Vec<String> = Vec::new();
        for line in reader.lines().filter_map(Result::ok) {
            log::debug!("[tddy-codex] stdout line ({} bytes)", line.len());
            if let Some(ref mut f) = conv_file {
                let _ = writeln!(f, "{}", line);
                let _ = f.flush();
            }
            raw_lines.push(line);
        }

        let stderr_buf = stderr_thread
            .and_then(|j| j.join().ok())
            .unwrap_or_default();

        let status = child
            .wait()
            .map_err(|e| BackendError::InvocationFailed(e.to_string()))?;
        super::clear_child_pid();
        let exit_code = status.code().unwrap_or(-1);

        log::info!(
            "[tddy-codex] process exited code={} (goal={:?})",
            exit_code,
            request.goal_id
        );
        if !stderr_buf.trim().is_empty() {
            log::debug!("[tddy-codex] stderr: {}", stderr_buf.trim());
        }

        let parsed = parse_codex_jsonl_output(&raw_lines);
        let raw_stream = if raw_lines.is_empty() {
            None
        } else {
            Some(raw_lines.join("\n"))
        };
        let stderr_opt = if stderr_buf.is_empty() {
            None
        } else {
            Some(stderr_buf)
        };

        Ok(InvokeResponse {
            output: parsed.result_text,
            exit_code,
            session_id: parsed.session_id,
            questions: vec![],
            raw_stream,
            stderr: stderr_opt,
        })
    }
}

/// Merge system prompt / path with user prompt using the same precedence as [`super::cursor::CursorBackend`].
pub(crate) fn merge_codex_prompt(request: &InvokeRequest) -> Result<String, BackendError> {
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

    let merged = match system_content {
        Some(ref sys) => format!("{}\n\n{}", sys, request.prompt),
        None => request.prompt.clone(),
    };
    log::debug!(
        "[tddy-codex] merged prompt length: {} (user prompt {} bytes)",
        merged.len(),
        request.prompt.len()
    );
    Ok(merged)
}

/// Arguments after the `codex` binary: `codex <these...>`.
///
/// Shape:
/// - Fresh: `exec`, `--json`, optional `-C` cwd, optional `-m` model, sandbox/approval flags, prompt.
/// - Resume: `exec`, `resume`, `<session_id>`, then the same optionals, then prompt.
///
/// **Permission / plan mapping (Codex CLI):**
/// - Plan-style goals (`agent_cli_plan_mode` + read-only): `--sandbox read-only` and
///   `--ask-for-approval never` so non-interactive runs do not block on approvals.
/// - Editing goals (default): `--sandbox workspace-write` and `--ask-for-approval never`.
pub(crate) fn build_codex_exec_argv(request: &InvokeRequest, merged_prompt: &str) -> Vec<String> {
    let mut args = vec!["exec".to_string()];

    match &request.session {
        Some(super::SessionMode::Resume(id)) => {
            args.push("resume".to_string());
            args.push(id.clone());
        }
        Some(super::SessionMode::Fresh(_)) | None => {}
    }

    args.push("--json".to_string());

    if let Some(ref wd) = request.working_dir {
        args.push("-C".to_string());
        args.push(wd.display().to_string());
    }

    if let Some(ref m) = request.model {
        args.push("-m".to_string());
        args.push(m.clone());
    }

    if request.hints.agent_cli_plan_mode && request.hints.permission == PermissionHint::ReadOnly {
        args.push("--sandbox".to_string());
        args.push("read-only".to_string());
    } else {
        args.push("--sandbox".to_string());
        args.push("workspace-write".to_string());
    }
    args.push("--ask-for-approval".to_string());
    args.push("never".to_string());

    args.push(merged_prompt.to_string());
    log::debug!("[tddy-codex] argv len={} (prompt trailing)", args.len());
    args
}

#[async_trait::async_trait]
impl super::CodingBackend for CodexBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.invoke_sync(request))
            .await
            .map_err(|e| BackendError::InvocationFailed(e.to_string()))?
    }

    fn name(&self) -> &str {
        "codex"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{GoalHints, GoalId, InvokeRequest, PermissionHint, SessionMode};

    fn hints_tdd_plan_goal() -> GoalHints {
        GoalHints {
            display_name: "Plan".to_string(),
            permission: PermissionHint::ReadOnly,
            allowed_tools: vec![],
            default_model: None,
            agent_output: false,
            agent_cli_plan_mode: true,
            claude_nonzero_exit_ok_if_structured_response: true,
        }
    }

    fn hints_tdd_red_goal() -> GoalHints {
        GoalHints {
            display_name: "Red".to_string(),
            permission: PermissionHint::AcceptEdits,
            allowed_tools: vec![],
            default_model: None,
            agent_output: true,
            agent_cli_plan_mode: false,
            claude_nonzero_exit_ok_if_structured_response: false,
        }
    }

    fn stub_request(prompt: &str, goal_id: &str, hints: GoalHints) -> InvokeRequest {
        let gid = GoalId::new(goal_id);
        let sk = GoalId::new(goal_id);
        InvokeRequest {
            prompt: prompt.to_string(),
            system_prompt: None,
            system_prompt_path: None,
            goal_id: gid,
            submit_key: sk,
            hints,
            model: None,
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
            session_dir: None,
        }
    }

    #[test]
    fn codex_exec_argv_fresh_includes_exec_json_and_prompt() {
        let req = stub_request("do the plan", "plan", hints_tdd_plan_goal());
        let merged = merge_codex_prompt(&req).expect("merge");
        let args = build_codex_exec_argv(&req, &merged);
        assert_eq!(
            args.first().map(String::as_str),
            Some("exec"),
            "argv should start with exec subcommand, got {:?}",
            args
        );
        assert!(
            args.iter().any(|a| a == "--json"),
            "argv should include --json, got {:?}",
            args
        );
        assert!(
            args.last()
                .map(|s| s.contains("do the plan"))
                .unwrap_or(false),
            "prompt should be present in argv, got {:?}",
            args
        );
    }

    #[test]
    fn codex_exec_argv_resume_includes_session_id() {
        let mut req = stub_request("continue", "plan", hints_tdd_plan_goal());
        req.session = Some(SessionMode::Resume("sess-resume-99".to_string()));
        let merged = merge_codex_prompt(&req).expect("merge");
        let args = build_codex_exec_argv(&req, &merged);
        let pos_exec = args.iter().position(|a| a == "exec").expect("exec");
        assert_eq!(args.get(pos_exec + 1).map(String::as_str), Some("resume"));
        assert_eq!(
            args.get(pos_exec + 2).map(String::as_str),
            Some("sess-resume-99")
        );
    }

    #[test]
    fn codex_exec_argv_includes_model_when_set() {
        let mut req = stub_request("hi", "red", hints_tdd_red_goal());
        req.model = Some("gpt-5".to_string());
        let merged = merge_codex_prompt(&req).expect("merge");
        let args = build_codex_exec_argv(&req, &merged);
        let pos_m = args
            .iter()
            .position(|a| a == "-m")
            .expect("expected -m when model set");
        assert_eq!(args.get(pos_m + 1).map(String::as_str), Some("gpt-5"));
    }

    #[test]
    fn codex_merge_prompt_combines_system_like_cursor() {
        let mut req = stub_request("user line", "plan", hints_tdd_plan_goal());
        req.system_prompt = Some("system instruction".to_string());
        let merged = merge_codex_prompt(&req).expect("merge");
        assert!(
            merged.contains("system instruction") && merged.contains("user line"),
            "expected system then user like CursorBackend, got {:?}",
            merged
        );
    }

    /// `GoalHints::agent_cli_plan_mode` and permission must map to explicit Codex CLI flags (documented beside argv builder).
    #[test]
    fn codex_exec_argv_maps_plan_goal_hints_to_flags() {
        let req = stub_request("goal body", "plan", hints_tdd_plan_goal());
        let merged = merge_codex_prompt(&req).expect("merge");
        let args = build_codex_exec_argv(&req, &merged);
        assert!(
            args.iter().any(|a| {
                a.starts_with("--approval")
                    || a.contains("approval")
                    || a == "--sandbox"
                    || a.contains("sandbox")
            }),
            "plan-mode read-only hints should produce documented codex sandbox/approval argv, got {:?}",
            args
        );
    }
}

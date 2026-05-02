//! OpenAI Codex CLI backend (`codex exec`, `codex exec resume <id>`, `--json`).
//!
//! Spawns non-interactive `codex exec` with JSONL on stdout. Maps
//! [`GoalHints::agent_cli_plan_mode`] + [`PermissionHint`] to explicit Codex flags
//! (see [`build_codex_exec_argv`]).
//!
//! **Auth:** `codex exec` does **not** emit an OpenAI browser OAuth URL when credentials are
//! missing — you get JSONL / stderr API errors (e.g. 401) only. The sign-in link is produced by
//! **`codex login`** (stdout + `BROWSER`); see [`CodexBackend::spawn_oauth_login`] and the
//! `tddy-coder` `BROWSER` hook for capturing that URL into the session dir.

use super::{InvokeRequest, InvokeResponse, PermissionHint};
use crate::error::BackendError;
use crate::stream::codex::{
    codex_jsonl_last_error_message, codex_stderr_brief_for_user, parse_codex_jsonl_output,
};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Basename under the TDDY artifact session directory; stores Codex CLI `thread_id` for `codex exec resume`.
pub const CODEX_THREAD_ID_FILENAME: &str = "codex_thread_id";

/// Basename written when Codex invokes `BROWSER` with the OpenAI authorize URL (`tddy-coder` hook).
pub const CODEX_OAUTH_AUTHORIZE_URL_FILENAME: &str = "codex_oauth_authorize.url";

/// Extract an `https://…` URL from a line of `codex login` output (or any single-line text).
#[must_use]
pub fn scrape_codex_oauth_authorize_url_from_text(s: &str) -> Option<String> {
    let s = s.trim();
    let start = s.find("https://")?;
    let rest = &s[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let mut url = rest[..end].to_string();
    while url.ends_with(')') || url.ends_with('"') || url.ends_with('\'') || url.ends_with(',') {
        url.pop();
    }
    (url.len() > "https://".len()).then_some(url)
}

/// When Codex/OpenAI returns 401-style text, append a short fix hint for the TUI.
fn codex_openai_auth_remediation(detail: &str) -> Option<&'static str> {
    let d = detail.to_lowercase();
    if !d.contains("401") {
        return None;
    }
    if d.contains("unauthorized")
        || d.contains("missing bearer")
        || d.contains("authentication")
        || d.contains("invalid api key")
    {
        Some(
            "Fix: not signed in to OpenAI for Codex. `codex exec --json` does not print a login URL or start browser OAuth — only API errors.\n\
             Run `codex login` first (URL on stdout + BROWSER), or `tddy-coder --session-dir <artifact_dir> --codex-oauth-login` to capture the link for the web UI.\n\
             Or use an API key: `printenv OPENAI_API_KEY | codex login --with-api-key`.",
        )
    } else {
        None
    }
}

pub(crate) fn write_codex_thread_id_file(session_dir: &Path, thread_id: &str) {
    let path = session_dir.join(CODEX_THREAD_ID_FILENAME);
    match std::fs::write(&path, thread_id.trim()) {
        Ok(()) => log::debug!("[tddy-codex] persisted thread id to {}", path.display()),
        Err(e) => log::warn!("[tddy-codex] could not write {}: {}", path.display(), e),
    }
}

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

    /// Run `codex login` with **browser OAuth** (the default `codex login` flow, **not** `--device-auth`).
    ///
    /// On Unix, when [`std::env::current_exe`] succeeds, sets `BROWSER` to the current executable and
    /// `TDDY_CODEX_OAUTH_OUT` to `{session_dir}/{[`CODEX_OAUTH_AUTHORIZE_URL_FILENAME`]}`, matching
    /// [`invoke`](Self::invoke) so the `tddy-coder` pre-main hook records the authorize URL.
    ///
    /// Stdout is scanned in a background thread for an `https://` line as a fallback if the CLI only
    /// prints the link.
    ///
    /// You must [`std::process::Child::wait`] on the returned child: Codex keeps a localhost callback
    /// server until the user finishes signing in at `http://localhost:<port>/…`.
    pub fn spawn_oauth_login(
        &self,
        session_dir: &Path,
    ) -> Result<std::process::Child, BackendError> {
        std::fs::create_dir_all(session_dir).map_err(|e| {
            BackendError::InvocationFailed(format!(
                "codex login: could not create session_dir {}: {}",
                session_dir.display(),
                e
            ))
        })?;
        let oauth_out = session_dir.join(CODEX_OAUTH_AUTHORIZE_URL_FILENAME);
        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("login");
        cmd.env("PATH", super::path_with_exe_dir());
        #[cfg(unix)]
        if let Ok(exe) = std::env::current_exe() {
            log::info!(
                target: "tddy_core::backend::codex",
                "[tddy-codex] codex login OAuth capture: BROWSER={}, TDDY_CODEX_OAUTH_OUT={}",
                exe.display(),
                oauth_out.display()
            );
            cmd.env("TDDY_CODEX_OAUTH_OUT", oauth_out.as_os_str());
            cmd.env("BROWSER", exe.as_os_str());
        }
        cmd.env("TDDY_SESSION_DIR", session_dir.as_os_str());
        cmd.current_dir(session_dir);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                BackendError::BinaryNotFound(self.binary_path.to_string_lossy().to_string())
            } else {
                BackendError::InvocationFailed(format!("codex login spawn: {}", e))
            }
        })?;

        let out = child
            .stdout
            .take()
            .ok_or_else(|| BackendError::InvocationFailed("codex login: no stdout".into()))?;
        let err = child.stderr.take();
        let oauth_clone = oauth_out.clone();
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(out);
            for line in reader.lines().map_while(Result::ok) {
                if let Some(url) = scrape_codex_oauth_authorize_url_from_text(&line) {
                    if let Err(e) = std::fs::write(&oauth_clone, &url) {
                        log::warn!(
                            "[tddy-codex] codex login: could not write authorize URL to {}: {}",
                            oauth_clone.display(),
                            e
                        );
                    } else {
                        log::debug!("[tddy-codex] codex login: captured URL from stdout");
                    }
                }
            }
        });
        std::thread::spawn(move || {
            if let Some(h) = err {
                let reader = std::io::BufReader::new(h);
                for line in reader.lines().map_while(Result::ok) {
                    log::debug!("[tddy-codex] codex login stderr: {}", line);
                }
            }
        });

        Ok(child)
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
        #[cfg(unix)]
        if let Some(ref sd) = request.session_dir {
            let oauth_out = sd.join(CODEX_OAUTH_AUTHORIZE_URL_FILENAME);
            if let Ok(exe) = std::env::current_exe() {
                log::info!(
                    target: "tddy_core::backend::codex",
                    "[tddy-codex] codex OAuth capture: BROWSER=re-exec, TDDY_CODEX_OAUTH_OUT={}",
                    oauth_out.display()
                );
                cmd.env("TDDY_CODEX_OAUTH_OUT", oauth_out.as_os_str());
                cmd.env("BROWSER", exe.as_os_str());
            }
        }
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
        for line in reader.lines().map_while(Result::ok) {
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

        if exit_code != 0 {
            // Match Claude backend: plan-style goals may see nonzero exit despite usable stdout.
            let tolerate_nonzero = request.hints.claude_nonzero_exit_ok_if_structured_response
                && !parsed.result_text.trim().is_empty();
            if tolerate_nonzero {
                log::debug!(
                    "[tddy-codex] CLI exited with code {} but non-empty parsed output; treating as success",
                    exit_code
                );
            } else {
                if !stderr_buf.trim().is_empty() {
                    log::warn!("[tddy-codex] stderr: {}", stderr_buf.trim());
                }
                // JSONL `error` / `turn.failed` carry the real reason; stderr is often tracing noise.
                let detail = codex_jsonl_last_error_message(&raw_lines)
                    .or_else(|| codex_stderr_brief_for_user(&stderr_buf))
                    .unwrap_or_default();
                let mut msg = if detail.is_empty() {
                    format!(
                        "Codex CLI exited with code {} (no error detail). Invoked: {}",
                        exit_code, cmd_str
                    )
                } else {
                    format!(
                        "Codex CLI exited with code {}: {}\nInvoked: {}",
                        exit_code, detail, cmd_str
                    )
                };
                if let Some(hint) = codex_openai_auth_remediation(&detail) {
                    msg.push_str("\n\n");
                    msg.push_str(hint);
                }
                return Err(BackendError::InvocationFailed(msg));
            }
        }

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

        if let Some(ref sink) = request.progress_sink {
            sink.emit(&crate::stream::ProgressEvent::AgentExited {
                exit_code,
                goal: request.submit_key.to_string(),
            });
        }

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
/// Codex nests flags: **`exec`-level options** (`-C`, `-s`, `--json`) must appear **before** the
/// `resume` subcommand. `resume` only accepts its own `[OPTIONS]` (e.g. `-m`); it does **not** accept
/// `-C`, `--sandbox`, or `--ask-for-approval`.
///
/// - Fresh: `exec [-C dir] [-s SANDBOX] --json [-m model] <PROMPT>`
/// - Resume: `exec [-C dir] [-s SANDBOX] --json resume [-m model] <SESSION_ID> <PROMPT>`
///
/// **Sandbox:** `-s read-only` for plan-style read-only goals; `-s workspace-write` otherwise.
pub(crate) fn build_codex_exec_argv(request: &InvokeRequest, merged_prompt: &str) -> Vec<String> {
    let mut args = vec!["exec".to_string()];

    if let Some(ref wd) = request.working_dir {
        args.push("-C".to_string());
        args.push(wd.display().to_string());
    }

    if request.hints.agent_cli_plan_mode && request.hints.permission == PermissionHint::ReadOnly {
        args.push("-s".to_string());
        args.push("read-only".to_string());
    } else {
        args.push("-s".to_string());
        args.push("workspace-write".to_string());
    }

    args.push("--json".to_string());

    if matches!(&request.session, Some(super::SessionMode::Resume(_))) {
        args.push("resume".to_string());
    }

    if let Some(ref m) = request.model {
        args.push("-m".to_string());
        args.push(m.clone());
    }

    if let Some(super::SessionMode::Resume(id)) = &request.session {
        args.push(id.clone());
    }

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
        let pos_json = args.iter().position(|a| a == "--json").expect("--json");
        let pos_resume = args.iter().position(|a| a == "resume").expect("resume");
        let pos_sid = args
            .iter()
            .position(|a| a == "sess-resume-99")
            .expect("session id");
        assert!(
            pos_json < pos_resume && pos_resume < pos_sid,
            "exec-level --json before resume subcommand, then SESSION_ID; got {:?}",
            args
        );
        assert_eq!(args.last().map(String::as_str), Some("continue"));
    }

    #[test]
    fn codex_exec_argv_resume_model_before_session_id() {
        let mut req = stub_request("go", "red", hints_tdd_red_goal());
        req.session = Some(SessionMode::Resume("sid-1".to_string()));
        req.model = Some("gpt-5".to_string());
        let merged = merge_codex_prompt(&req).expect("merge");
        let args = build_codex_exec_argv(&req, &merged);
        let pos_resume = args.iter().position(|a| a == "resume").expect("resume");
        let pos_m = args.iter().position(|a| a == "-m").expect("-m");
        let pos_sid = args.iter().position(|a| a == "sid-1").expect("sid");
        assert!(
            pos_resume < pos_m && pos_m < pos_sid,
            "resume then -m then SESSION_ID; got {:?}",
            args
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
            args.iter().any(|a| a == "-s") && args.iter().any(|a| a == "read-only"),
            "plan-mode read-only hints should produce -s read-only, got {:?}",
            args
        );
    }

    #[test]
    fn scrape_codex_oauth_url_from_login_line() {
        let line = "https://auth.openai.com/oauth/authorize?x=1&y=2";
        assert_eq!(
            scrape_codex_oauth_authorize_url_from_text(line).as_deref(),
            Some(line)
        );
    }

    #[test]
    fn scrape_codex_oauth_url_strips_trailing_punct() {
        let line = r#"See: https://auth.openai.com/x?y=1)"#;
        assert_eq!(
            scrape_codex_oauth_authorize_url_from_text(line).as_deref(),
            Some("https://auth.openai.com/x?y=1")
        );
    }

    #[test]
    fn codex_openai_auth_remediation_matches_401_missing_bearer() {
        let d = "unexpected status 401 Unauthorized: Missing bearer or basic authentication";
        assert!(super::codex_openai_auth_remediation(d).is_some());
    }

    #[test]
    fn codex_openai_auth_remediation_ignores_unrelated_401() {
        assert!(super::codex_openai_auth_remediation("HTTP 401").is_none());
    }
}

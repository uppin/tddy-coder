//! Cursor agent CLI backend implementation.
//!
//! Invokes the `agent` executable on `PATH` with stream-json output format (`--output-format stream-json`, etc.).
//! Some environments also expose the same flow as `cursor agent`; this backend targets the `agent` binary name.
//! Based on Baker CLI's executeWithCursor.

use super::{InvokeRequest, InvokeResponse};
use crate::adapters::headless::{turn_workdir, write_rendered_files};
use crate::adapters::{CursorStream, HeadlessRenderer, HeadlessTurn, StreamAdapter, StreamSinks};
use crate::error::BackendError;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// Type for progress callback.
type ProgressCallback = Option<Arc<Mutex<Box<dyn FnMut(&crate::stream::ProgressEvent) + Send>>>>;

/// One Cursor child: what it is spawned with, and how its stdout is read.
struct ChildLaunch<'a> {
    program: &'a Path,
    argv: &'a [String],
    /// Set on the child on top of what the backend passes it.
    env: &'a [(String, String)],
    adapter: &'a dyn StreamAdapter,
    exit_ok_if_structured: bool,
    /// The system prompt's text, which the conversation log records.
    system_prompt: Option<&'a str>,
}

/// Cursor CLI has no `--system-prompt`: the system content (the file at `system_prompt_path`, else
/// `system_prompt`) is prepended to the user prompt.
fn system_content(request: &InvokeRequest) -> Result<Option<String>, BackendError> {
    match request.system_prompt_path {
        Some(ref path) => std::fs::read_to_string(path).map(Some).map_err(|e| {
            BackendError::InvocationFailed(format!(
                "failed to read system_prompt_path {}: {}",
                path.display(),
                e
            ))
        }),
        None => Ok(request.system_prompt.clone()),
    }
}

/// `prompt` with `system` in front of it.
fn merged_prompt(system: Option<&str>, prompt: &str) -> String {
    match system {
        Some(sys) => format!("{sys}\n\n{prompt}"),
        None => prompt.to_string(),
    }
}

/// Backend that invokes the Cursor agent CLI binary.
pub struct CursorBackend {
    binary_path: PathBuf,
    progress_callback: ProgressCallback,
    /// The agent's declaration its invocation is rendered from (`with_headless_renderer`,
    /// `#agent-tools 9/12`); `None` builds the argv by hand ([`build_cursor_cli_args`]).
    ///
    /// TODO(headless-others): `create_backend("cursor")` passes one, so the spec is the workflow's
    /// path. `build_cursor_cli_args` and `register_cursor_mcp_config` stay for a backend built
    /// without a renderer (`with_path`); deleting them needs those callers moved to a renderer
    /// first — `docs/dev/todo/2026-11-03-headless-others-legacy-argv-deletion.md`.
    headless: Option<Arc<dyn HeadlessRenderer>>,
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
            .field("headless", &self.headless)
            .finish()
    }
}

impl Clone for CursorBackend {
    fn clone(&self) -> Self {
        Self {
            binary_path: self.binary_path.clone(),
            progress_callback: self.progress_callback.clone(),
            headless: self.headless.clone(),
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
            headless: None,
        }
    }

    /// Create a backend with a custom binary path.
    #[must_use]
    pub fn with_path(path: PathBuf) -> Self {
        Self {
            binary_path: path,
            progress_callback: None,
            headless: None,
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

    /// Render every invocation from `renderer` — `cursor.yaml`'s `coding_agent.headless` — instead
    /// of building the argv by hand (`#agent-tools 9/12`).
    #[must_use]
    pub fn with_headless_renderer(mut self, renderer: Arc<dyn HeadlessRenderer>) -> Self {
        self.headless = Some(renderer);
        self
    }

    /// One invocation rendered from `renderer`: the system prompt merged into the prompt as today,
    /// the goal's plan intent bound as the param `plan` and `force` always bound (the workflow
    /// backend writes), the session onto `session_id`/`resume`, the working directory as `workdir`,
    /// and [`HeadlessRenderer::tool_server`] onto the MCP slot — the headless `workdir` slot that
    /// writes `<working_dir>/.cursor/mcp.json`; the rendered files written, the program spawned
    /// with today's environment, and its stdout read through the adapter it names
    /// (`cursor-stream`).
    fn invoke_from_spec(
        &self,
        request: InvokeRequest,
        renderer: &Arc<dyn HeadlessRenderer>,
    ) -> Result<InvokeResponse, BackendError> {
        let system_prompt = system_content(&request)?;
        let workdir = turn_workdir(request.working_dir.as_deref())?;
        // `plan` is bound for a goal that plans, `force` always: this backend serves the workflow,
        // which writes. A param is bound by being present.
        let mut params = BTreeMap::from([("force".to_string(), "1".to_string())]);
        if request.hints.agent_cli_plan_mode {
            params.insert("plan".to_string(), "1".to_string());
        }
        let turn = HeadlessTurn {
            prompt: merged_prompt(system_prompt.as_deref(), &request.prompt),
            model: request.model.clone(),
            session: request.session.clone(),
            permission_mode: None,
            append_system_prompt_file: None,
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
            permission_prompt_tool: None,
            mcp_server: renderer.tool_server(),
            // Cursor's MCP slot writes under the workdir; nothing renders into a scratch directory.
            scratch_dir: workdir.clone(),
            workdir,
            params,
            route_args: Vec::new(),
        };
        let invocation = renderer.render(&turn)?;
        write_rendered_files(&invocation.files)?;
        self.run_child(
            &request,
            ChildLaunch {
                program: &invocation.program,
                argv: &invocation.argv,
                env: &invocation.env,
                adapter: invocation.adapter.as_ref(),
                exit_ok_if_structured: invocation.exit_ok_if_structured,
                system_prompt: system_prompt.as_deref(),
            },
        )
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

    async fn list_models(&self) -> Result<super::BackendModels, BackendError> {
        let self_clone = self.clone();
        tokio::task::spawn_blocking(move || self_clone.list_models_sync())
            .await
            .map_err(|e| BackendError::InvocationFailed(e.to_string()))?
    }
}

impl CursorBackend {
    /// Enumerate the account's models by running `<binary> --list-models` and parsing its catalog.
    fn list_models_sync(&self) -> Result<super::BackendModels, BackendError> {
        let output = Command::new(&self.binary_path)
            .arg("--list-models")
            .env("PATH", super::path_with_exe_dir())
            .stdin(Stdio::null())
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    BackendError::BinaryNotFound(self.binary_path.to_string_lossy().to_string())
                } else {
                    BackendError::InvocationFailed(format!("agent --list-models failed: {e}"))
                }
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BackendError::InvocationFailed(format!(
                "agent --list-models exited with {}: {}",
                output.status,
                stderr.trim()
            )));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let models = parse_cursor_model_list(&stdout);
        if models.models.is_empty() {
            return Err(BackendError::InvocationFailed(
                "agent --list-models returned no models".to_string(),
            ));
        }
        Ok(models)
    }
}

/// Parse the stdout of `agent --list-models` into a [`BackendModels`] catalog.
///
/// Each catalog line is `"<id> - <label>"`; a header line and blanks are ignored, and the entry
/// tagged `"(current, default)"` becomes `default_model` with the marker stripped from its label.
pub(crate) fn parse_cursor_model_list(stdout: &str) -> super::BackendModels {
    const DEFAULT_MARKER: &str = "(current, default)";
    let mut models = Vec::new();
    let mut default_model: Option<String> = None;
    for line in stdout.lines() {
        let line = line.trim();
        let Some((id, label)) = line.split_once(" - ") else {
            // Header ("Available models") and blank lines carry no " - " separator.
            continue;
        };
        let id = id.trim().to_string();
        let label = label.trim();
        let (label, is_default) = match label.strip_suffix(DEFAULT_MARKER) {
            Some(stripped) => (stripped.trim_end().to_string(), true),
            None => (label.to_string(), false),
        };
        if is_default {
            default_model = Some(id.clone());
        }
        models.push(super::BackendModel::new(id, label));
    }
    let default_model = default_model
        .or_else(|| models.first().map(|m| m.id.clone()))
        .unwrap_or_default();
    super::BackendModels {
        models,
        default_model,
    }
}

/// Builds argv for `cursor agent` (excluding the binary path). Used by [`CursorBackend::invoke_sync`] and tests.
pub(crate) fn build_cursor_cli_args(request: &InvokeRequest, prompt: &str) -> Vec<String> {
    let mut args = Vec::new();
    if request.hints.agent_cli_plan_mode {
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
    args.push("--approve-mcps".to_string());
    args
}

/// Write `.cursor/mcp.json` under `base_dir` registering `tddy-tools --mcp`.
fn register_cursor_mcp_config(base_dir: &std::path::Path) -> Result<(), BackendError> {
    let tddy_tools = super::claude::tddy_tools_path().ok_or_else(|| {
        BackendError::InvocationFailed("tddy-tools binary not found for cursor MCP".into())
    })?;
    let cursor_dir = base_dir.join(".cursor");
    std::fs::create_dir_all(&cursor_dir)
        .map_err(|e| BackendError::InvocationFailed(format!("create .cursor dir: {e}")))?;
    let config = serde_json::json!({
        "mcpServers": {
            "tddy-tools": {
                "command": tddy_tools.to_string_lossy(),
                "args": ["--mcp"]
            }
        }
    });
    crate::atomic_file::write_atomic(&cursor_dir.join("mcp.json"), config.to_string())
        .map_err(|e| BackendError::InvocationFailed(format!("write .cursor/mcp.json: {e}")))?;
    Ok(())
}

impl CursorBackend {
    fn invoke_sync(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        if let Some(renderer) = &self.headless {
            return self.invoke_from_spec(request, renderer);
        }
        let system_prompt = system_content(&request)?;
        let prompt = merged_prompt(system_prompt.as_deref(), &request.prompt);

        let mcp_base = request
            .working_dir
            .as_deref()
            .unwrap_or_else(|| std::path::Path::new("."));
        register_cursor_mcp_config(mcp_base)?;

        let args = build_cursor_cli_args(&request, &prompt);
        self.run_child(
            &request,
            ChildLaunch {
                program: &self.binary_path,
                argv: &args,
                env: &[],
                adapter: &CursorStream,
                exit_ok_if_structured: false,
                system_prompt: system_prompt.as_deref(),
            },
        )
    }

    /// The spawn and read every Cursor invocation shares: `launch`'s program with its argv and
    /// environment on top of the backend's own, its stdout read through `launch.adapter` into the
    /// response, and its exit judged by that adapter.
    fn run_child(
        &self,
        request: &InvokeRequest,
        launch: ChildLaunch<'_>,
    ) -> Result<InvokeResponse, BackendError> {
        let system_content = launch.system_prompt;
        let mut cmd = Command::new(launch.program);
        if let Some(ref wd) = request.working_dir {
            cmd.current_dir(wd);
        }
        cmd.args(launch.argv);

        let resolved = super::claude::which_binary(launch.program);
        let cwd_str = request
            .working_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "(unknown)".into())
            });
        let cmd_str = super::format_command_for_log(launch.program, launch.argv, 200);
        log::info!("[tddy-coder] Cursor backend command: {}", cmd_str);
        log::debug!(
            "[tddy-coder] Cursor backend spawning: {} (resolved: {})",
            launch.program.display(),
            resolved
        );
        log::debug!("[tddy-coder] cwd: {}", cwd_str);
        log::debug!(
            "[tddy-coder] goal: {:?}, model: {:?}, session: {:?}",
            request.goal_id,
            request.model,
            request.session
        );
        log::debug!(
            "[tddy-coder] prompt ({} bytes): {}",
            request.prompt.len(),
            &request.prompt[..request.prompt.floor_char_boundary(500)]
        );
        if let Some(sys) = system_content {
            log::debug!(
                "[tddy-coder] system_prompt ({} bytes): {}",
                sys.len(),
                &sys[..sys.floor_char_boundary(500)]
            );
        }

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
        if let Some(ref remote) = request.remote {
            for (key, value) in remote.env_pairs() {
                cmd.env(key, value);
            }
        }
        cmd.envs(launch.env.iter().map(|(key, value)| (key, value)));
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(if request.inherit_stdin {
            Stdio::inherit()
        } else {
            Stdio::null()
        });

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                BackendError::BinaryNotFound(launch.program.to_string_lossy().to_string())
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
                "goal": request.hints.display_name,
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

        let mut reader = std::io::BufReader::new(stdout_handle);
        let stream_result = launch.adapter.read_stream(
            &mut reader,
            StreamSinks {
                on_progress: &mut on_progress,
                on_raw_output: &mut on_raw_output,
                on_debug_line: if request.debug {
                    Some(&mut on_debug_line)
                } else {
                    None
                },
                on_conversation_line: if request.conversation_output_path.is_some() {
                    Some(&mut on_conversation_line)
                } else {
                    None
                },
            },
        )?;

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
            request.goal_id,
            request.session
        );

        if !launch
            .adapter
            .exit_ok(exit_code, &stream_result, launch.exit_ok_if_structured)
        {
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
                goal: request.submit_key.to_string(),
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
    use super::parse_cursor_model_list;
    use super::CursorBackend;
    use crate::backend::{InvokeRequest, SessionMode};
    use tddy_workflow::{GoalHints, GoalId, PermissionHint};

    const LIST_MODELS_STDOUT: &str = "Available models\n\
        \n\
        auto - Auto\n\
        gpt-5.2 - GPT-5.2\n\
        composer-2.5 - Composer 2.5 (current, default)\n";

    #[test]
    fn parses_list_models_output_into_id_and_label_pairs() {
        // When
        let catalog = parse_cursor_model_list(LIST_MODELS_STDOUT);

        // Then — the header/blank lines are ignored and each entry is split on " - "
        let pairs: Vec<(&str, &str)> = catalog
            .models
            .iter()
            .map(|m| (m.id.as_str(), m.label.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("auto", "Auto"),
                ("gpt-5.2", "GPT-5.2"),
                ("composer-2.5", "Composer 2.5"),
            ]
        );
    }

    #[test]
    fn treats_the_current_default_entry_as_the_default_model() {
        // When
        let catalog = parse_cursor_model_list(LIST_MODELS_STDOUT);

        // Then — the "(current, default)" marker selects the default and is stripped from the label
        assert_eq!(catalog.default_model, "composer-2.5");
    }

    /// Matches the default TDD recipe plan-goal hints (planning workflow → `--plan` on Cursor).
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

    /// Matches the default TDD recipe red-goal hints (no planning intent).
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

    fn minimal_request(
        goal_id: &str,
        model: Option<&str>,
        prompt: &str,
        hints: GoalHints,
    ) -> InvokeRequest {
        let gid = GoalId::new(goal_id);
        let sk = GoalId::new(goal_id);
        InvokeRequest {
            prompt: prompt.to_string(),
            system_prompt: None,
            system_prompt_path: None,
            goal_id: gid,
            submit_key: sk,
            hints,
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
            session_dir: None,
            remote: None,
        }
    }

    #[test]
    fn build_args_includes_model_when_set() {
        // Given
        let request = minimal_request("plan", Some("composer-2.5"), "test", hints_tdd_plan_goal());

        // When
        let args = build_cursor_cli_args(&request, "test prompt");

        // Then
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"composer-2.5".to_string()));
    }

    #[test]
    fn build_args_omits_model_when_none() {
        // Given
        let request = minimal_request("plan", None, "test", hints_tdd_plan_goal());

        // When
        let args = build_cursor_cli_args(&request, "test prompt");

        // Then
        assert!(!args.contains(&"--model".to_string()));
    }

    #[test]
    fn build_args_plan_includes_plan_flag_when_recipe_sets_planning_intent() {
        // Given
        let request = minimal_request("plan", None, "x", hints_tdd_plan_goal());

        // When
        let args = build_cursor_cli_args(&request, "p");

        // Then
        assert!(args.contains(&"--plan".to_string()));
    }

    #[test]
    fn build_args_session_fresh_omits_session_flags() {
        // Given
        let mut request = minimal_request("red", None, "x", hints_tdd_red_goal());
        request.session = Some(SessionMode::Fresh("sid-1".to_string()));

        // When
        let args = build_cursor_cli_args(&request, "p");

        // Then
        assert!(!args.iter().any(|a| a == "--session-id"));
        assert!(!args.iter().any(|a| a == "--resume"));
    }

    #[test]
    fn build_args_session_resume_includes_resume() {
        // Given
        let mut request = minimal_request("red", None, "x", hints_tdd_red_goal());
        request.session = Some(SessionMode::Resume("resume-id".to_string()));

        // When
        let args = build_cursor_cli_args(&request, "p");

        // Then
        assert!(args.iter().any(|a| a == "--resume"));
        assert!(args.contains(&"resume-id".to_string()));
    }

    #[test]
    fn build_args_includes_headless_mcp_approval_flags() {
        let request = minimal_request("red", None, "x", hints_tdd_red_goal());
        let args = build_cursor_cli_args(&request, "p");
        assert!(args.contains(&"--approve-mcps".to_string()));
        assert!(args.contains(&"--force".to_string()));
        assert!(args.contains(&"--trust".to_string()));
    }

    #[test]
    fn default_cursor_cli_binary_on_path_is_agent() {
        // When
        let backend = CursorBackend::new();

        // Then
        assert_eq!(
            backend.binary_path,
            std::path::PathBuf::from("agent"),
            "invokes the Cursor CLI as `agent` on PATH; BinaryNotFound must report agent, not cursor"
        );
    }
}

//! The one headless workflow backend (`#agent-tools 15/19`): every headless coding agent's
//! workflow turn — `claude -p`, `agent -p`, `codex exec --json`, `oz agent run`, or any agent a
//! YAML file declares — runs through [`SpecHeadlessBackend`], configured by the agent's spec.
//!
//! Changeset: `docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md`.
//!
//! What differs between agents is data, never a Rust branch on an agent's name:
//!
//! - the argv, the files it writes and the adapter its stdout is read through come from the spec's
//!   renderer ([`HeadlessRenderer`]);
//! - how the system prompt is delivered, which params a goal's hints bind, whether the approval
//!   tool and the tool server are registered, whether a route replaces the model, who mints a fresh
//!   session and where registration files go come from the spec's protocol fields
//!   ([`HeadlessProtocol`]);
//! - whether a provider route can point the agent elsewhere comes from its [`RoutedAgent`].
//!
//! A turn's child runs in **its own process group**, under a **turn deadline**: a cancel (the
//! turn's future dropped) or the deadline kills that group and no other, so two concurrent turns
//! never reach each other's processes. The agent process sees **no `TDDY_*` variable**: a
//! `tddy-tools` it runs — over MCP or from its shell — finds the session through the
//! [`crate::session_pointer`] file the turn writes in its working directory.

use std::collections::BTreeMap;
use std::io::{BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::adapters::headless::{
    render_env, EnvTemplate, HeadlessModelListing, HeadlessProtocol, SystemPromptDelivery,
};
use crate::adapters::{HeadlessInvocation, HeadlessRenderer, HeadlessTurn, StreamSinks};
use crate::error::BackendError;
use crate::provider_route::{agent_routable, HarnessRoute, RouteRefusal, RoutedAgent};
use crate::session_pointer::SessionPointer;
use crate::stream::{self, ProgressEvent};
use crate::turn_files::TurnFiles;

use super::routed::route_child_env;
use super::turn_process::{
    describe_deadline, spawn_turn, strip_tddy_env, TurnCommand, TurnGroup, STDERR_GRACE,
};
use super::{
    curated_models_for_agent, BackendModels, CodingBackend, InvokeRequest, InvokeResponse,
    SessionMode,
};

/// The tool the agent asks before anything not pre-approved.
const APPROVAL_PROMPT_TOOL: &str = "mcp__tddy-tools__approval_prompt";

/// How long a killed turn is given to wind down before its result is abandoned.
const KILL_GRACE: Duration = Duration::from_secs(5);

/// How long a workflow turn may run before it is cancelled and its process group killed, when the
/// backend is given no other deadline ([`SpecHeadlessBackend::with_turn_deadline`]).
pub const DEFAULT_TURN_DEADLINE: Duration = Duration::from_secs(4 * 60 * 60);

/// How long a model listing (`models.list_command`) may run.
pub const MODEL_LISTING_DEADLINE: Duration = Duration::from_secs(30);

type ProgressCallback = Option<Arc<Mutex<Box<dyn FnMut(&ProgressEvent) + Send>>>>;

/// A headless coding agent's workflow backend, configured by its spec.
#[derive(Clone)]
pub struct SpecHeadlessBackend {
    renderer: Arc<dyn HeadlessRenderer>,
    protocol: HeadlessProtocol,
    agent: RoutedAgent,
    provider_route: Option<HarnessRoute>,
    model_listing: Option<HeadlessModelListing>,
    progress_callback: ProgressCallback,
    turn_deadline: Duration,
}

impl std::fmt::Debug for SpecHeadlessBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpecHeadlessBackend")
            .field("renderer", &self.renderer)
            .field("protocol", &self.protocol)
            .field("agent", &self.agent)
            .field("provider_route", &self.provider_route)
            .field("model_listing", &self.model_listing)
            .field("turn_deadline", &self.turn_deadline)
            .finish_non_exhaustive()
    }
}

impl SpecHeadlessBackend {
    /// A backend rendering every turn from `renderer`, run as `protocol` says, routable as `agent`
    /// says, with [`DEFAULT_TURN_DEADLINE`], no provider route, no model listing (the curated
    /// catalogue answers `list_models`) and no progress callback.
    pub fn new(
        renderer: Arc<dyn HeadlessRenderer>,
        protocol: HeadlessProtocol,
        agent: RoutedAgent,
    ) -> Self {
        Self {
            renderer,
            protocol,
            agent,
            provider_route: None,
            model_listing: None,
            progress_callback: None,
            turn_deadline: DEFAULT_TURN_DEADLINE,
        }
    }

    /// Every turn is cancelled, and its process group killed, once it has run for `deadline`.
    #[must_use]
    pub fn with_turn_deadline(mut self, deadline: Duration) -> Self {
        self.turn_deadline = deadline;
        self
    }

    /// `list_models` runs `listing` (the spec's `models.list_command`, rendered with the
    /// spec-resolved binary) under [`MODEL_LISTING_DEADLINE`] and reads it through its adapter.
    #[must_use]
    pub fn with_model_listing(mut self, listing: HeadlessModelListing) -> Self {
        self.model_listing = Some(listing);
        self
    }

    /// Progress events go to `f` for a request that names no `progress_sink`.
    #[must_use]
    pub fn with_progress<F>(mut self, f: F) -> Self
    where
        F: FnMut(&ProgressEvent) + Send + 'static,
    {
        self.progress_callback = Some(Arc::new(Mutex::new(Box::new(f))));
        self
    }

    /// Every turn runs on `route`: its environment on the child and its argv spliced as
    /// `route_args`, the request's model replaced as the protocol's `route_model` says. Refused,
    /// naming the agent's product, for an agent whose spec declares no provider
    /// (`provider_route::agent_routable`) — never silently run on the vendor.
    pub fn with_provider_route(mut self, route: HarnessRoute) -> Result<Self, RouteRefusal> {
        agent_routable(&self.agent)?;
        self.provider_route = Some(route);
        Ok(self)
    }

    /// The protocol this backend runs its turns with.
    pub fn protocol(&self) -> &HeadlessProtocol {
        &self.protocol
    }

    fn agent_id(&self) -> &str {
        self.renderer.agent_id()
    }

    /// The turn's parameters, read off the request as the protocol says.
    fn turn_for(
        &self,
        request: &InvokeRequest,
        system_text: Option<&str>,
        system_prompt_file: Option<&Path>,
        workdir: &Path,
    ) -> Result<HeadlessTurn, BackendError> {
        let protocol = &self.protocol;
        let mut params = protocol.goal_params.bind(&request.hints);
        let permission_mode = params.remove("permission_mode");
        let tool_server = if protocol.workflow_mcp {
            Some(self.renderer.tool_server().ok_or_else(|| {
                BackendError::InvocationFailed(format!(
                    "the {} workflow registers its tool server, and the agent's spec has no tddy-tools dependency to run it",
                    self.agent_id()
                ))
            })?)
        } else {
            None
        };
        let route_args = self
            .provider_route
            .as_ref()
            .map(|route| route.args.clone())
            .unwrap_or_default();
        let scratch_dir = match protocol.scratch {
            crate::adapters::ScratchDir::SessionDir => {
                request.session_dir.clone().ok_or_else(|| {
                    BackendError::InvocationFailed(format!(
                        "a headless {} turn writes its registration to the session directory, and the request names none",
                        self.agent_id()
                    ))
                })?
            }
            crate::adapters::ScratchDir::Workdir => workdir.to_path_buf(),
        };
        let session = match (&request.session, protocol.fresh_session) {
            (Some(SessionMode::Fresh(_)), crate::adapters::FreshSession::Agent) => None,
            (session, _) => session.clone(),
        };
        let prompt = match (protocol.system_prompt, system_text) {
            (SystemPromptDelivery::Prepend, Some(system)) => {
                format!("{system}\n\n{}", request.prompt)
            }
            _ => request.prompt.clone(),
        };
        let mut allowed_tools = request.hints.allowed_tools.clone();
        allowed_tools.extend(request.extra_allowed_tools.iter().flatten().cloned());
        Ok(HeadlessTurn {
            prompt,
            model: protocol
                .route_model
                .model_for(request.model.clone(), &route_args),
            session,
            permission_mode,
            append_system_prompt_file: system_prompt_file.map(Path::to_path_buf),
            allowed_tools,
            denied_tools: Vec::new(),
            permission_prompt_tool: tool_server
                .as_ref()
                .filter(|_| protocol.approval_tool)
                .map(|_| APPROVAL_PROMPT_TOOL.to_string()),
            mcp_server: tool_server,
            workdir: workdir.to_path_buf(),
            scratch_dir,
            params,
            route_args,
        })
    }

    /// The request's system prompt text: the file's, else the inline one. A prompt that goes in
    /// front of the user's has to be read, and an unreadable file is an error; one passed to the
    /// agent as a file is only read for the conversation log, best effort.
    fn system_text(&self, request: &InvokeRequest) -> Result<Option<String>, BackendError> {
        let Some(path) = &request.system_prompt_path else {
            return Ok(request.system_prompt.clone());
        };
        match (std::fs::read_to_string(path), self.protocol.system_prompt) {
            (Ok(text), _) => Ok(Some(text)),
            (Err(e), SystemPromptDelivery::Prepend) => Err(BackendError::InvocationFailed(
                format!("failed to read system_prompt_path {}: {e}", path.display()),
            )),
            (Err(_), SystemPromptDelivery::Flag) => Ok(None),
        }
    }

    /// The environment the child is given on top of the inherited one (minus its `TDDY_*`
    /// variables): the route's, then the spec's own `headless.env` rendered.
    fn child_env(
        &self,
        invocation: &HeadlessInvocation,
        request: &InvokeRequest,
    ) -> Result<Vec<(String, String)>, BackendError> {
        let self_exe = std::env::current_exe().map_err(|e| {
            BackendError::InvocationFailed(format!("this executable cannot be named: {e}"))
        })?;
        let mut env: Vec<(String, String)> = self
            .provider_route
            .iter()
            .flat_map(route_child_env)
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        env.extend(render_env(
            &invocation.env,
            &EnvTemplate {
                session_dir: request.session_dir.clone(),
                self_exe,
            },
        )?);
        Ok(env)
    }
}

/// What a turn's reader thread needs besides the child.
struct TurnRead {
    request: InvokeRequest,
    invocation: HeadlessInvocation,
    agent_id: String,
    system_text: Option<String>,
    progress_callback: ProgressCallback,
    /// The request's conversation log, opened for appending.
    conversation: Option<std::fs::File>,
}

#[async_trait::async_trait]
impl CodingBackend for SpecHeadlessBackend {
    /// One workflow turn: the request mapped onto a `HeadlessTurn` by the protocol, rendered, its
    /// files and the session pointer written in the working directory (git-excluded) and removed
    /// when the turn ends, the program spawned in its own process group with no inherited `TDDY_*`
    /// variable, its stdout read through the adapter, and its exit judged by the adapter — all under
    /// the turn deadline, with a bounded stderr grace.
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        // A workflow turn runs where the request says — never in whichever directory this process
        // happens to run in: the turn's files and session pointer are written there.
        let workdir = request.working_dir.clone().ok_or_else(|| {
            BackendError::InvocationFailed(format!(
                "a headless {} turn runs in the request's working directory, and the request names none",
                self.agent_id()
            ))
        })?;
        let system_text = self.system_text(&request)?;
        // A system prompt passed as a file is the request's own, or its inline text in a temporary
        // file the turn removes.
        let (system_prompt_path, temp_system_prompt) = match (
            self.protocol.system_prompt,
            &request.system_prompt_path,
            &system_text,
        ) {
            (SystemPromptDelivery::Flag, Some(path), _) => (Some(path.clone()), None),
            (SystemPromptDelivery::Flag, None, Some(text)) => {
                let path = std::env::temp_dir().join(format!(
                    "tddy-sys-{}-{}.txt",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_nanos())
                ));
                (Some(path.clone()), Some((path, text.clone().into_bytes())))
            }
            _ => (None, None),
        };
        let turn = self.turn_for(
            &request,
            system_text.as_deref(),
            system_prompt_path.as_deref(),
            &workdir,
        )?;
        let invocation = self.renderer.render(&turn)?;
        let env = self.child_env(&invocation, &request)?;

        let mut files: Vec<(PathBuf, Vec<u8>)> = temp_system_prompt.into_iter().collect();
        files.extend(invocation.files.iter().cloned());
        let mut turn_files = TurnFiles::write(&files, &workdir)?;
        if let Some(socket) = &request.socket_path {
            turn_files.write_session_pointer(
                &workdir,
                &SessionPointer {
                    socket: socket.clone(),
                    repo_dir: workdir.clone(),
                    session_dir: request.session_dir.clone(),
                    remote: request
                        .remote
                        .as_ref()
                        .map(|remote| remote.env_pairs().into_iter().collect::<BTreeMap<_, _>>())
                        .unwrap_or_default(),
                },
            )?;
        }

        log::debug!(
            "[headless] {} command: {}",
            self.agent_id(),
            super::format_command_for_log(&invocation.program, &invocation.argv, 200)
        );
        // Opened before the spawn, so a log that cannot be opened refuses the turn with no child to
        // reap.
        let conversation = open_conversation_log(&request)?;
        let mut child = spawn_turn(&TurnCommand {
            program: &invocation.program,
            argv: &invocation.argv,
            env: &env,
            cwd: &workdir,
            inherit_stdin: request.inherit_stdin,
        })?;
        let (group, reaped) = TurnGroup::new(child.id());
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let read = TurnRead {
            request,
            invocation,
            agent_id: self.agent_id().to_string(),
            system_text,
            progress_callback: self.progress_callback.clone(),
            conversation,
        };
        let mut reader =
            tokio::task::spawn_blocking(move || read_turn(read, child, stdout, stderr, &reaped));
        let outcome = match tokio::time::timeout(self.turn_deadline, &mut reader).await {
            Ok(joined) => joined.map_err(|e| BackendError::InvocationFailed(e.to_string()))?,
            Err(_elapsed) => {
                group.kill();
                // The kill closes its stdout; the reader then ends, and is not waited for longer.
                let _ = tokio::time::timeout(KILL_GRACE, &mut reader).await;
                Err(BackendError::InvocationFailed(format!(
                    "{} turn did not end within its {} deadline; its process group was killed",
                    self.agent_id(),
                    describe_deadline(self.turn_deadline)
                )))
            }
        };
        drop(group);
        drop(turn_files);
        outcome
    }

    /// The spec id (`claude`, `cursor`, `codex`, `oz`, or a YAML-only agent's), so the workflow
    /// engine's codex thread-id swap and the context-glob and model tables keep keying on it.
    fn name(&self) -> &str {
        self.agent_id()
    }

    /// The spec's model listing when it declares one, else the curated catalogue for
    /// [`CodingBackend::name`].
    async fn list_models(&self) -> Result<BackendModels, BackendError> {
        let Some(listing) = &self.model_listing else {
            return Ok(curated_models_for_agent(self.name()));
        };
        let shown = format!("{} {}", listing.program.display(), listing.argv.join(" "));
        let mut command = tokio::process::Command::new(&listing.program);
        command
            .args(&listing.argv)
            .env("PATH", super::path_with_exe_dir())
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        strip_tddy_env(command.as_std_mut());
        let output = tokio::time::timeout(MODEL_LISTING_DEADLINE, command.output())
            .await
            .map_err(|_| {
                BackendError::InvocationFailed(format!(
                    "{shown} did not end within its {} deadline",
                    describe_deadline(MODEL_LISTING_DEADLINE)
                ))
            })?
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    BackendError::BinaryNotFound(listing.program.to_string_lossy().to_string())
                } else {
                    BackendError::InvocationFailed(format!("{shown} failed: {e}"))
                }
            })?;
        if !output.status.success() {
            return Err(BackendError::InvocationFailed(format!(
                "{shown} exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let models = listing
            .adapter
            .parse_models(&String::from_utf8_lossy(&output.stdout));
        if models.models.is_empty() {
            return Err(BackendError::InvocationFailed(format!(
                "{shown} returned no models"
            )));
        }
        Ok(models)
    }
}

/// Reads one spawned turn to its end on a blocking thread: stdout through the adapter, the exit
/// through the adapter's judgement, stderr kept, the conversation log appended.
fn read_turn(
    read: TurnRead,
    mut child: std::process::Child,
    stdout: Option<std::process::ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    reaped: &AtomicBool,
) -> Result<InvokeResponse, BackendError> {
    let TurnRead {
        request,
        invocation,
        agent_id,
        system_text,
        progress_callback,
        mut conversation,
    } = read;
    let Some(stdout) = stdout else {
        // Never the case for a piped stdout; the child is still reaped, so none is left behind.
        let _ = child.kill();
        let _ = child.wait();
        reaped.store(true, Ordering::SeqCst);
        return Err(BackendError::InvocationFailed(
            "failed to capture stdout".to_string(),
        ));
    };
    let stderr_reader = stderr.map(read_stderr);

    let emit_progress = {
        let progress_sink = request.progress_sink.clone();
        move |event: &ProgressEvent| {
            if let Some(sink) = &progress_sink {
                sink.emit(event);
            } else if let Some(callback) = &progress_callback {
                if let Ok(mut callback) = callback.lock() {
                    callback(event);
                }
            }
        }
    };
    let mut on_progress = emit_progress.clone();
    let agent_output = request.agent_output;
    let agent_output_sink = request.agent_output_sink.clone();
    let mut on_raw_output = move |text: &str| {
        if agent_output {
            if let Some(sink) = &agent_output_sink {
                sink.emit(text);
            } else if std::env::var("TDDY_QUIET").is_err() {
                eprint!("{text}");
            }
        }
    };
    let debug = request.debug;
    let mut on_debug_line = |line: &str| {
        if debug {
            log::debug!("[tddy-coder debug] {line}");
        }
    };
    if let Some(file) = conversation.as_mut() {
        let (session_id, is_resume) = request
            .session
            .as_ref()
            .map(|s| (s.session_id().to_string(), s.is_resume()))
            .unwrap_or_default();
        let entry = serde_json::json!({
            "type": "tddy-request",
            "goal": request.hints.display_name,
            "prompt": request.prompt,
            "system_prompt": system_text,
            "model": request.model,
            "session_id": session_id,
            "is_resume": is_resume,
        });
        let _ = writeln!(file, "{entry}");
        let _ = file.flush();
    }
    let mut on_conversation_line = |line: &str| {
        if let Some(file) = conversation.as_mut() {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
    };
    let has_conversation = request.conversation_output_path.is_some();

    let stream_result = invocation.adapter.read_stream(
        &mut BufReader::new(stdout),
        StreamSinks {
            on_progress: &mut on_progress,
            on_raw_output: &mut on_raw_output,
            on_debug_line: if debug {
                Some(&mut on_debug_line)
            } else {
                None
            },
            on_conversation_line: if has_conversation {
                Some(&mut on_conversation_line)
            } else {
                None
            },
        },
    );
    let status = child
        .wait()
        .map_err(|e| BackendError::InvocationFailed(e.to_string()));
    reaped.store(true, Ordering::SeqCst);
    let stream_result = stream_result?;
    let status = status?;
    let stderr = stderr_reader.map(StderrReader::finish).unwrap_or_default();
    let exit_code = status.code().unwrap_or(-1);
    log::debug!(
        "[headless] {agent_id} exited with code {exit_code} (goal: {:?})",
        request.goal_id
    );

    if !invocation
        .adapter
        .exit_ok(exit_code, &stream_result, invocation.exit_ok_if_structured)
    {
        let detail = invocation.adapter.failure_detail(&stream_result, &stderr);
        return Err(BackendError::InvocationFailed(if detail.is_empty() {
            format!("{agent_id} exited with code {exit_code}")
        } else {
            format!("{agent_id} exited with code {exit_code}: {detail}")
        }));
    }

    emit_progress(&stream::ProgressEvent::AgentExited {
        exit_code,
        goal: request.submit_key.to_string(),
    });
    Ok(InvokeResponse {
        output: stream_result.result_text,
        exit_code,
        session_id: Some(stream_result.session_id).filter(|id| !id.is_empty()),
        questions: stream_result.questions,
        raw_stream: if stream_result.raw_lines.is_empty() {
            None
        } else {
            Some(stream_result.raw_lines.join("\n"))
        },
        stderr: Some(stderr).filter(|stderr| !stderr.trim().is_empty()),
    })
}

/// The request's conversation log, opened for appending, when it names one.
fn open_conversation_log(request: &InvokeRequest) -> Result<Option<std::fs::File>, BackendError> {
    request
        .conversation_output_path
        .as_ref()
        .map(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| {
                    BackendError::InvocationFailed(format!(
                        "failed to open conversation output {}: {e}",
                        path.display()
                    ))
                })
        })
        .transpose()
}

/// A child's stderr, read to its end on a thread of its own.
struct StderrReader {
    collected: Arc<Mutex<Vec<u8>>>,
    done: std::sync::mpsc::Receiver<()>,
}

fn read_stderr(mut stderr: std::process::ChildStderr) -> StderrReader {
    let collected: Arc<Mutex<Vec<u8>>> = Arc::default();
    let (sender, done) = std::sync::mpsc::channel();
    let sink = Arc::clone(&collected);
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match std::io::Read::read(&mut stderr, &mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => sink
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .extend_from_slice(&chunk[..read]),
            }
        }
        let _ = sender.send(());
    });
    StderrReader { collected, done }
}

impl StderrReader {
    /// What the child wrote to stderr, once it has closed it — or, after a bounded grace (a
    /// grandchild can hold the pipe open), what had been read by then.
    fn finish(self) -> String {
        let _ = self.done.recv_timeout(STDERR_GRACE);
        let bytes = self
            .collected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

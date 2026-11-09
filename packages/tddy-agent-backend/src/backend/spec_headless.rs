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

mod turn_read;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::adapters::headless::{
    render_env, EnvTemplate, HeadlessModelListing, HeadlessProtocol, SystemPromptDelivery,
};
use crate::adapters::{HeadlessInvocation, HeadlessRenderer, HeadlessTurn};
use crate::error::BackendError;
use crate::provider_route::{agent_routable, HarnessRoute, RouteRefusal, RoutedAgent};
use crate::session_pointer::{SessionPointer, SessionPointerOwner};
use crate::stream::ProgressEvent;
use crate::turn_files::TurnFiles;

use super::routed::route_child_env;
use super::turn_process::{describe_deadline, spawn_turn, strip_tddy_env, TurnCommand, TurnGroup};
use super::{BackendModels, CodingBackend, InvokeRequest, InvokeResponse, SessionMode};
use crate::backend::spec_headless::turn_read::open_conversation_log;
use crate::backend::spec_headless::turn_read::read_turn;

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
    curated_models: BackendModels,
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
            curated_models: BackendModels::default(),
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

    /// `list_models` answers `catalog` (the spec's `coding_agent.models` curated list, read by
    /// the caller that built this backend from that spec) when the spec declares no listing.
    #[must_use]
    pub fn with_curated_models(mut self, catalog: BackendModels) -> Self {
        self.curated_models = catalog;
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

/// The session pointer a turn of `request` writes in `workdir`, owned by this process: `None` when
/// the request names no session socket to point at.
fn turn_pointer(request: &InvokeRequest, workdir: &Path) -> Option<SessionPointer> {
    let socket = request.socket_path.clone()?;
    Some(SessionPointer {
        socket: Some(socket),
        repo_dir: workdir.to_path_buf(),
        session_dir: request.session_dir.clone(),
        remote: request
            .remote
            .as_ref()
            .map(|remote| remote.env_pairs().into_iter().collect::<BTreeMap<_, _>>())
            .unwrap_or_default(),
        notify: BTreeMap::new(),
        owner: Some(SessionPointerOwner::this_process()),
    })
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
        if let Some(pointer) = turn_pointer(&request, &workdir) {
            turn_files.write_session_pointer(&workdir, &pointer)?;
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
            return Ok(self.curated_models.clone());
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

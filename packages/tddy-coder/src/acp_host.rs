//! ACP-host bridge: drive a `tddy-coder --acp` ACP agent subprocess and translate its inbound
//! `session/update` notifications into internal [`PresenterEvent`]s.
//!
//! This is the mirror image of the `tddy-coder --acp` agent (see [`crate::acp_agent`]): where the
//! agent turns a running workflow's presenter events into outbound ACP updates, this bridge turns
//! an agent's outbound ACP updates back into presenter events. Those events are exactly what the
//! existing `TddyRemoteService` already serves to the web, so the browser's `TddyRemote` stream is
//! unchanged whether the session runs the in-process `WorkflowEngine` or a bridged ACP agent.
//!
//! The pure ACP → presenter mapping lives in [`tddy_acp::mapping::session_update_to_presenter_event`];
//! this module owns only the transport: spawning the subprocess, speaking JSON-RPC over stdio via
//! [`acp::ClientSideConnection`], and forwarding mapped events to a caller-supplied sink.
//!
//! # Scope
//! Additive and opt-in. This does not replace the in-process `WorkflowEngine` → Presenter path, and
//! nothing wires the session host to use the bridge by default.
//!
// TODO(acp-host-rewire): flipping the per-session host to drive the workflow "fully via ACP"
// through this bridge by default needs end-to-end LiveKit validation on a real host and is out of
// scope here. The bridge stays an opt-in library component until that validation lands.

use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_client_protocol::{self as acp, Agent as _, Client};
use anyhow::Context as _;
use tddy_core::PresenterEvent;
use tokio_util::compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _};

/// Upper bound on a single bridged prompt turn. A well-behaved agent ends the turn promptly; this
/// only guards against a wedged subprocess so the caller never blocks forever.
const TURN_TIMEOUT: Duration = Duration::from_secs(30);

/// A resolved command for spawning an ACP agent subprocess: the program plus its argument vector.
///
/// Kept as plain data (rather than a live `Command`) so callers — and tests — can inspect the exact
/// argv without spawning anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpAgentCommand {
    /// The binary to spawn (e.g. the `tddy-coder` executable).
    pub program: PathBuf,
    /// The arguments to pass, in order.
    pub args: Vec<String>,
}

/// Build the command that runs `tddy-coder --acp` for the given agent, recipe, and data directory.
///
/// Pure and side-effect free: it only assembles the argv. The returned [`AcpAgentCommand`] carries
/// `--acp` plus the selected `--agent`, `--recipe`, and `--tddy-data-dir`, matching the CLI flags
/// that `tddy-coder`'s `--acp` mode honours.
#[must_use]
pub fn build_acp_agent_command(
    coder_bin: &Path,
    agent: &str,
    recipe: &str,
    data_dir: &Path,
) -> AcpAgentCommand {
    AcpAgentCommand {
        program: coder_bin.to_path_buf(),
        args: vec![
            "--acp".to_string(),
            "--agent".to_string(),
            agent.to_string(),
            "--recipe".to_string(),
            recipe.to_string(),
            "--tddy-data-dir".to_string(),
            data_dir.to_string_lossy().to_string(),
        ],
    }
}

/// ACP client that maps each inbound `session/update` to a [`PresenterEvent`] and forwards it to the
/// caller's sink. Permission requests are auto-approved so an agent that elicits still completes.
struct AcpHostClient {
    sink: Box<dyn Fn(PresenterEvent)>,
}

#[async_trait::async_trait(?Send)]
impl Client for AcpHostClient {
    async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
        if let Some(event) = tddy_acp::mapping::session_update_to_presenter_event(&args.update) {
            (self.sink)(event);
        }
        Ok(())
    }

    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        // Auto-approve by selecting the agent's first offered option (its id must be one the agent
        // advertised — a fixed id may match nothing). Deny when the agent offered no options.
        match args.options.first() {
            Some(option) => Ok(acp::RequestPermissionResponse::new(
                acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
                    option.option_id.clone(),
                )),
            )),
            None => Ok(acp::RequestPermissionResponse::new(
                acp::RequestPermissionOutcome::Cancelled,
            )),
        }
    }
}

/// Drives a `tddy-coder --acp` ACP agent subprocess, forwarding its outbound `session/update`
/// notifications to a caller-supplied sink as [`PresenterEvent`]s.
///
/// Construct from a resolved [`AcpAgentCommand`] via [`AcpHostBridge::from_command`], or directly
/// from a program path and args via [`AcpHostBridge::with_agent_command`] (used by tests to point
/// the bridge at `tddy-acp-stub`).
#[derive(Debug, Clone)]
pub struct AcpHostBridge {
    agent_path: PathBuf,
    agent_args: Vec<String>,
}

impl AcpHostBridge {
    /// Create a bridge that spawns `program` with `args` as the ACP agent.
    #[must_use]
    pub fn with_agent_command(program: PathBuf, args: Vec<String>) -> Self {
        Self {
            agent_path: program,
            agent_args: args,
        }
    }

    /// Create a bridge from a resolved [`AcpAgentCommand`] (e.g. [`build_acp_agent_command`]).
    #[must_use]
    pub fn from_command(command: AcpAgentCommand) -> Self {
        Self::with_agent_command(command.program, command.args)
    }

    /// Run one prompt turn against the ACP agent.
    ///
    /// Spawns the agent, performs `initialize` → `new_session` → `prompt`, and for every inbound
    /// `session/update` that maps to a [`PresenterEvent`] calls `sink` with it (on the bridge's
    /// worker thread). Returns the turn's [`acp::StopReason`] when the agent ends the turn.
    ///
    /// Blocking: the ACP connection uses a `?Send` current-thread runtime, so the whole turn runs
    /// on a dedicated worker thread and this call blocks the caller until the turn completes. That
    /// keeps the method usable from either sync or async contexts without nesting runtimes.
    pub fn run_prompt(
        &self,
        prompt: &str,
        working_dir: &Path,
        sink: impl Fn(PresenterEvent) + Send + 'static,
    ) -> anyhow::Result<acp::StopReason> {
        let agent_path = self.agent_path.clone();
        let agent_args = self.agent_args.clone();
        let prompt = prompt.to_string();
        let working_dir = working_dir.to_path_buf();

        let worker = std::thread::spawn(move || -> anyhow::Result<acp::StopReason> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("build ACP host bridge runtime")?;
            let local_set = tokio::task::LocalSet::new();
            rt.block_on(local_set.run_until(async move {
                match tokio::time::timeout(
                    TURN_TIMEOUT,
                    run_turn(agent_path, agent_args, prompt, working_dir, sink),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => Err(anyhow::anyhow!("ACP host bridge turn timed out")),
                }
            }))
        });

        worker
            .join()
            .map_err(|_| anyhow::anyhow!("ACP host bridge worker panicked"))?
    }
}

/// Spawn the agent subprocess, connect over stdio, and run a single prompt turn to its stop reason.
async fn run_turn(
    agent_path: PathBuf,
    agent_args: Vec<String>,
    prompt: String,
    working_dir: PathBuf,
    sink: impl Fn(PresenterEvent) + 'static,
) -> anyhow::Result<acp::StopReason> {
    let mut child = spawn_agent(&agent_path, &agent_args)?;
    let stdout = child.stdout.take().context("agent stdout missing")?;
    let stdin = child.stdin.take().context("agent stdin missing")?;
    let outgoing = stdin.compat_write();
    let incoming = stdout.compat();

    let client = AcpHostClient {
        sink: Box::new(sink),
    };
    let (conn, handle_io) = acp::ClientSideConnection::new(client, outgoing, incoming, |fut| {
        tokio::task::spawn_local(fut);
    });
    tokio::task::spawn_local(handle_io);

    let init_req = acp::InitializeRequest::new(acp::ProtocolVersion::V1).client_info(
        acp::Implementation::new("tddy-coder", env!("CARGO_PKG_VERSION")).title("TDDY Coder"),
    );
    conn.initialize(init_req)
        .await
        .map_err(|e| anyhow::anyhow!("ACP initialize failed: {e}"))?;

    let session = conn
        .new_session(acp::NewSessionRequest::new(working_dir))
        .await
        .map_err(|e| anyhow::anyhow!("ACP new_session failed: {e}"))?;

    let prompt_req = acp::PromptRequest::new(session.session_id, vec![prompt.into()]);
    let response = conn
        .prompt(prompt_req)
        .await
        .map_err(|e| anyhow::anyhow!("ACP prompt failed: {e}"))?;

    let _ = child.kill().await;
    Ok(response.stop_reason)
}

/// Spawn the ACP agent subprocess with piped stdio, killed on drop so a cancelled turn cleans up.
fn spawn_agent(agent_path: &Path, agent_args: &[String]) -> anyhow::Result<tokio::process::Child> {
    let mut cmd = tokio::process::Command::new(agent_path);
    cmd.args(agent_args);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());
    cmd.kill_on_drop(true);
    cmd.spawn().with_context(|| {
        format!(
            "spawn ACP agent subprocess: {}",
            agent_path.to_string_lossy()
        )
    })
}

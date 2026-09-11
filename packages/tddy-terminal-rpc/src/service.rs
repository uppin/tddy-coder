//! The `terminal_session.TerminalSessionService` implementation, and the entry a host's wiring
//! layer registers.
//!
//! All nine methods are terminal behaviour this crate already owns: seven of them are the
//! [`crate::bridge`] functions plus the auth gate in front of them, and the two lifecycle families
//! (multiple terminals per session, the single-screen control mutex) are the state a *host* keeps
//! about its own PTYs. That state is why the ports below are traits rather than values — resolving
//! a session token to an OS user, spawning a login shell as that user and holding the control lease
//! are all the serving host's answers, not this subsystem's behaviour.
//!
//! # What is deliberately not here
//!
//! Nothing resolves a `session_id` to a worktree, a task registry or a sandbox. The host's
//! [`TerminalRoster`] and [`TerminalSessionStore`] impls do that, because a terminal's *origin*
//! (a `CliSessionManager` task, a sandboxed session, a coder-local PTY) differs per host while the
//! streaming, replay and control semantics above do not.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tokio::sync::broadcast;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::bridge::{
    resolved_terminal_id, serve_get_terminal_history_with, serve_send_terminal_input,
    serve_stream_session_terminal_io_with, serve_stream_terminal_output_with, MAIN_TERMINAL_ID,
};
use crate::proto::terminal_session::{
    ClaimTerminalControlRequest, ClaimTerminalControlResponse, GetTerminalHistoryRequest,
    ListTerminalSessionsRequest, ListTerminalSessionsResponse, SendTerminalInputResponse,
    SessionTerminalInput, SessionTerminalOutput, StartTerminalSessionRequest,
    StartTerminalSessionResponse, StopTerminalSessionRequest, StopTerminalSessionResponse,
    StreamTerminalOutputRequest, TerminalControlEvent, TerminalHistoryChunk, TerminalSessionInfo,
    WatchTerminalControlRequest,
};
use crate::proto::terminal_session::{TerminalSessionService, TerminalSessionServiceServer};
use crate::session::TerminalSessionStore;

/// How many control events are buffered for a `WatchTerminalControl` subscriber before the relay
/// waits on it.
///
/// A lease changes when a human clicks "Claim terminal", so a screen that cannot keep up with
/// sixteen of those is not reading its stream at all — and making the relay wait is the right
/// answer there, rather than growing a queue nobody drains.
const CONTROL_EVENT_CHANNEL_CAPACITY: usize = 16;

/// Resolves a session token to the identity it authenticates, or `None` when the token is unknown
/// or expired (`UNAUTHENTICATED`).
pub type GithubUserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Maps an authenticated identity to the OS user this host runs its terminals as, or `None` when
/// the identity has no mapping (`PERMISSION_DENIED` — only an operator can fix that, which is why
/// it is a different refusal from an unknown token).
pub type OsUserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The outcome of a [`TerminalControl::claim`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlClaim {
    /// The caller now holds the lease and must present `control_token` on subsequent control RPCs.
    Granted { control_token: String },
    /// Another screen holds the lease.
    Denied { holder_screen_id: String },
}

/// Broadcast payload emitted when a session's control lease changes hands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlChange {
    pub session_id: String,
    pub holder_screen_id: String,
}

/// The single-screen terminal control mutex: at most one screen drives a session's terminals.
///
/// A port rather than state this crate keeps, because the lease is per *host*: the same session is
/// served by exactly one daemon, and that daemon's other surfaces (session deletion, the LiveKit
/// bridge) read and clear the same lease.
#[async_trait]
pub trait TerminalControl: Send + Sync {
    /// Issue the lease, or refuse and name the holder. `steal` evicts the current holder.
    async fn claim(&self, session_id: &str, screen_id: &str, steal: bool) -> ControlClaim;

    /// Whether `control_token` may drive `session_id`'s terminals. A session with no lease is
    /// uncontrolled and accepts any token, including the empty one.
    async fn verify(&self, session_id: &str, control_token: &str) -> bool;

    /// The screen holding the lease, or `None` when the session is uncontrolled.
    async fn holder_screen_id(&self, session_id: &str) -> Option<String>;

    /// Lease changes across all sessions; [`watch_terminal_control`] filters to its own.
    ///
    /// [`watch_terminal_control`]: TerminalSessionService::watch_terminal_control
    fn subscribe(&self) -> broadcast::Receiver<ControlChange>;
}

/// One running terminal as `ListTerminalSessions` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalDescriptor {
    pub terminal_id: String,
    /// `"claude-cli"` for the reserved main tool, `"bash"` for a started shell.
    pub kind: String,
    pub pid: u32,
}

/// The terminals a host runs for a session: start a login shell, stop one, list them.
///
/// A port because every step needs host state this crate has none of — which worktree the session
/// checked out and which task registry owns the child. Which *shell* to run is not among them:
/// [`crate::login_shell::login_shell_for`] answers that here, so a host cannot start a terminal in
/// a shell the target user does not have.
#[async_trait]
pub trait TerminalRoster: Send + Sync {
    /// Start a terminal running `shell_path` for `session_id`, and return its fresh id (never
    /// [`MAIN_TERMINAL_ID`]).
    async fn start(&self, session_id: &str, shell_path: &str) -> Result<String, Status>;

    /// Stop a started terminal. `false` when the session has no such terminal. The caller has
    /// already refused [`MAIN_TERMINAL_ID`], which is not stoppable through this surface.
    async fn stop(&self, session_id: &str, terminal_id: &str) -> bool;

    /// Every running terminal of `session_id`, including [`MAIN_TERMINAL_ID`].
    async fn list(&self, session_id: &str) -> Vec<TerminalDescriptor>;
}

/// Everything the nine handlers need from the host they run on.
///
/// A struct rather than six positional parameters: two of them are resolvers of the same shape
/// (`&str -> Option<String>`) that a call site could silently swap for each other, which would
/// answer every request with the wrong refusal.
pub struct TerminalSessionPorts {
    /// Session token to the identity it authenticates.
    pub github_users: GithubUserResolver,
    /// Authenticated identity to the OS user this host impersonates for it.
    pub os_users: OsUserResolver,
    /// Resolves `(session_id, terminal_id)` to the live terminal the streaming methods attach to.
    pub terminals: Arc<dyn TerminalSessionStore>,
    /// The session's control lease.
    pub control: Arc<dyn TerminalControl>,
    /// The session's running terminals.
    pub roster: Arc<dyn TerminalRoster>,
    /// The per-frame replay budget: how many bytes of retained output one open frame may carry.
    ///
    /// Injected because it is a *transport* limit — the largest message the host's gRPC / LiveKit
    /// link accepts — rather than a property of the capture ring. Hosts that have no reason to
    /// choose pass [`crate::bridge::DEFAULT_INITIAL_FRAME_BYTES`].
    pub initial_frame_bytes: usize,
}

/// The `terminal_session.TerminalSessionService` implementation.
pub struct TerminalSessionServiceImpl {
    ports: TerminalSessionPorts,
}

impl TerminalSessionServiceImpl {
    #[must_use]
    pub fn new(ports: TerminalSessionPorts) -> Self {
        Self { ports }
    }

    /// The identity a token authenticates, or why it may not be served.
    ///
    /// The two control-mutex methods stop here: claiming and watching the lease is a screen-identity
    /// question, and a caller whose identity this host has not mapped to an OS user can still be
    /// told who is driving the terminal it is looking at.
    fn github_user(&self, session_token: &str) -> Result<String, Status> {
        (self.ports.github_users)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))
    }

    /// The OS user a token's terminals run as, or why they may not be served.
    ///
    /// Every method that reaches a PTY — the four streaming/input ones and the three lifecycle ones
    /// — goes through here, because a terminal only exists under some OS user's identity.
    fn os_user(&self, session_token: &str) -> Result<String, Status> {
        let github_user = self.github_user(session_token)?;
        (self.ports.os_users)(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))
    }

    /// Refuse the call unless `control_token` may drive the session's terminals.
    async fn require_control(&self, session_id: &str, control_token: &str) -> Result<(), Status> {
        if self.ports.control.verify(session_id, control_token).await {
            Ok(())
        } else {
            Err(Status::failed_precondition(
                "terminal controlled by another screen",
            ))
        }
    }

    /// A `verify_control` closure for the bidi bridge, which re-checks every subsequent input chunk
    /// so a screen that loses the lease mid-stream stops being able to type.
    fn control_verifier(&self) -> impl Fn(&str, &str) -> ControlVerification + Send + Sync + use<> {
        let control = Arc::clone(&self.ports.control);
        move |session_id: &str, control_token: &str| {
            let control = Arc::clone(&control);
            let session_id = session_id.to_string();
            let control_token = control_token.to_string();
            Box::pin(async move { control.verify(&session_id, &control_token).await })
        }
    }
}

/// The future [`TerminalSessionServiceImpl::control_verifier`] hands the bridge. Boxed because the
/// closure it comes from is returned from a method, so its `async move` body has no nameable type.
type ControlVerification = std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>>;

/// A session id as the handlers key on it: whitespace-trimmed.
///
/// Trimmed in all nine methods rather than the six `connection.ConnectionService` trims in, so one
/// client cannot reach two different control leases — or two different terminals — for the same
/// session by sending the id with a stray space.
fn keyed_session_id(raw: &str) -> String {
    raw.trim().to_string()
}

#[async_trait]
impl TerminalSessionService for TerminalSessionServiceImpl {
    type StreamSessionTerminalIoStream = ReceiverStream<Result<SessionTerminalOutput, Status>>;

    /// Bidi terminal I/O. The first message carries the identity, the control token and the replay
    /// selection, so authentication and the control check happen on it before the stream opens —
    /// the bridge then re-checks control on every subsequent chunk.
    async fn stream_session_terminal_io(
        &self,
        request: Request<tddy_rpc::Streaming<SessionTerminalInput>>,
    ) -> Result<Response<Self::StreamSessionTerminalIoStream>, Status> {
        let mut in_stream = request.into_inner();
        let first = in_stream
            .next()
            .await
            .ok_or_else(|| Status::invalid_argument("stream ended before first message"))??;

        self.os_user(&first.session_token)?;
        let session_id = keyed_session_id(&first.session_id);
        let terminal_id = resolved_terminal_id(&first.terminal_id).to_string();
        self.require_control(&session_id, &first.control_token)
            .await?;

        let session = self
            .ports
            .terminals
            .get_terminal(&session_id, &terminal_id)
            .await
            .ok_or_else(|| Status::not_found("terminal not found or not running"))?;

        let rx = serve_stream_session_terminal_io_with(
            session,
            session_id,
            first,
            in_stream,
            self.control_verifier(),
            self.ports.initial_frame_bytes,
        )
        .await?;
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type StreamTerminalOutputStream = ReceiverStream<Result<SessionTerminalOutput, Status>>;

    /// The browser-compatible output half: connect-web's Fetch transport cannot send a streaming
    /// request body, so a browser opens this and sends input through `SendTerminalInput`.
    async fn stream_terminal_output(
        &self,
        request: Request<StreamTerminalOutputRequest>,
    ) -> Result<Response<Self::StreamTerminalOutputStream>, Status> {
        let mut req = request.into_inner();
        self.os_user(&req.session_token)?;
        req.session_id = keyed_session_id(&req.session_id);

        let rx = serve_stream_terminal_output_with(
            &*self.ports.terminals,
            req,
            self.ports.initial_frame_bytes,
        )
        .await?;
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    /// The browser-compatible input half. Control is checked before the terminal is resolved, so a
    /// displaced screen is told it lost the lease rather than that the terminal is missing.
    async fn send_terminal_input(
        &self,
        request: Request<SessionTerminalInput>,
    ) -> Result<Response<SendTerminalInputResponse>, Status> {
        let mut req = request.into_inner();
        self.os_user(&req.session_token)?;
        req.session_id = keyed_session_id(&req.session_id);
        self.require_control(&req.session_id, &req.control_token)
            .await?;

        let response = serve_send_terminal_input(&*self.ports.terminals, req).await?;
        Ok(Response::new(response))
    }

    type GetTerminalHistoryStream = ReceiverStream<Result<TerminalHistoryChunk, Status>>;

    /// One forward chunk of older output, for the scroll-up fill. Reading history is not driving
    /// the terminal, so it needs no control token — every screen watching a session may scroll.
    async fn get_terminal_history(
        &self,
        request: Request<GetTerminalHistoryRequest>,
    ) -> Result<Response<Self::GetTerminalHistoryStream>, Status> {
        let mut req = request.into_inner();
        self.os_user(&req.session_token)?;
        req.session_id = keyed_session_id(&req.session_id);

        let rx = serve_get_terminal_history_with(
            &*self.ports.terminals,
            req,
            self.ports.initial_frame_bytes,
        )
        .await?;
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    // TODO(#unbundle-node-6): `StartTerminalSession` / `StopTerminalSession` document a
    // `control_token` "required when the session has an active terminal controller", but neither
    // checks it — the same gap `connection.ConnectionService` has. Closing it changes who may
    // start a shell in someone else's session, which is a product decision rather than a move.
    async fn start_terminal_session(
        &self,
        request: Request<StartTerminalSessionRequest>,
    ) -> Result<Response<StartTerminalSessionResponse>, Status> {
        let req = request.into_inner();
        let os_user = self.os_user(&req.session_token)?;
        let session_id = keyed_session_id(&req.session_id);

        let shell_path = crate::login_shell::login_shell_for(&os_user);
        let terminal_id = self.ports.roster.start(&session_id, &shell_path).await?;
        Ok(Response::new(StartTerminalSessionResponse { terminal_id }))
    }

    async fn stop_terminal_session(
        &self,
        request: Request<StopTerminalSessionRequest>,
    ) -> Result<Response<StopTerminalSessionResponse>, Status> {
        let req = request.into_inner();
        self.os_user(&req.session_token)?;
        let session_id = keyed_session_id(&req.session_id);
        let terminal_id = req.terminal_id.trim();

        // The main terminal is the session's agent: stopping it here would leave a session whose
        // agent is gone but whose record says it is running. Ending a session is its own RPC.
        if terminal_id == MAIN_TERMINAL_ID {
            return Err(Status::invalid_argument(
                "the main terminal cannot be stopped via StopTerminalSession; \
                 use SignalSession or DeleteSession",
            ));
        }

        if self.ports.roster.stop(&session_id, terminal_id).await {
            Ok(Response::new(StopTerminalSessionResponse {
                ok: true,
                message: String::new(),
            }))
        } else {
            Err(Status::not_found("terminal not found"))
        }
    }

    async fn list_terminal_sessions(
        &self,
        request: Request<ListTerminalSessionsRequest>,
    ) -> Result<Response<ListTerminalSessionsResponse>, Status> {
        let req = request.into_inner();
        self.os_user(&req.session_token)?;
        let session_id = keyed_session_id(&req.session_id);

        let terminals = self
            .ports
            .roster
            .list(&session_id)
            .await
            .into_iter()
            .map(|terminal| TerminalSessionInfo {
                terminal_id: terminal.terminal_id,
                kind: terminal.kind,
                pid: terminal.pid,
            })
            .collect();
        Ok(Response::new(ListTerminalSessionsResponse { terminals }))
    }

    async fn claim_terminal_control(
        &self,
        request: Request<ClaimTerminalControlRequest>,
    ) -> Result<Response<ClaimTerminalControlResponse>, Status> {
        let req = request.into_inner();
        self.github_user(&req.session_token)?;
        let session_id = keyed_session_id(&req.session_id);

        let response = match self
            .ports
            .control
            .claim(&session_id, &req.screen_id, req.steal)
            .await
        {
            ControlClaim::Granted { control_token } => ClaimTerminalControlResponse {
                granted: true,
                control_token,
                current_holder_screen_id: String::new(),
            },
            ControlClaim::Denied { holder_screen_id } => ClaimTerminalControlResponse {
                granted: false,
                control_token: String::new(),
                current_holder_screen_id: holder_screen_id,
            },
        };
        Ok(Response::new(response))
    }

    type WatchTerminalControlStream = ReceiverStream<Result<TerminalControlEvent, Status>>;

    /// Lease ownership: a snapshot immediately, then one event per change, so a screen that has
    /// been displaced can render the "Claim terminal" CTA without polling.
    async fn watch_terminal_control(
        &self,
        request: Request<WatchTerminalControlRequest>,
    ) -> Result<Response<Self::WatchTerminalControlStream>, Status> {
        let req = request.into_inner();
        self.github_user(&req.session_token)?;
        let session_id = keyed_session_id(&req.session_id);

        // Subscribe before reading the snapshot, so a change landing between the two is delivered
        // rather than dropped into the gap.
        let changes = self.ports.control.subscribe();
        let you_are_controller = self
            .ports
            .control
            .verify(&session_id, &req.control_token)
            .await;
        let holder_screen_id = self
            .ports
            .control
            .holder_screen_id(&session_id)
            .await
            .unwrap_or_default();

        let (tx, rx) = tokio::sync::mpsc::channel(CONTROL_EVENT_CHANNEL_CAPACITY);
        let _ = tx
            .send(Ok(TerminalControlEvent {
                holder_screen_id,
                you_are_controller,
            }))
            .await;

        tokio::spawn(relay_control_changes(
            session_id,
            req.control_token,
            Arc::clone(&self.ports.control),
            changes,
            tx,
        ));
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

/// Forward the lease changes of one session as `TerminalControlEvent`s.
///
/// `you_are_controller` is re-derived from the watcher's own token on every change rather than
/// inferred from the holder id: the two disagree exactly when this screen is the one being
/// displaced, which is the case the CTA exists for.
async fn relay_control_changes(
    session_id: String,
    control_token: String,
    control: Arc<dyn TerminalControl>,
    mut changes: broadcast::Receiver<ControlChange>,
    tx: tokio::sync::mpsc::Sender<Result<TerminalControlEvent, Status>>,
) {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        match changes.recv().await {
            Ok(change) if change.session_id == session_id => {
                let you_are_controller = control.verify(&session_id, &control_token).await;
                let event = TerminalControlEvent {
                    holder_screen_id: change.holder_screen_id,
                    you_are_controller,
                };
                if tx.send(Ok(event)).await.is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}

/// The `terminal_session.TerminalSessionService` entry a host's wiring layer registers.
///
/// `#unbundle` node 6 made this crate *serve* the proto it already owned: the nine methods had been
/// extracted here as [`crate::bridge`] functions but were only ever reachable through
/// `connection.ConnectionService`'s duplicate copy of the same schema, with hand-written converters
/// between the two. Registering this entry is the coordinate that copy is retired in favour of.
///
/// Takes the whole [`TerminalSessionPorts`] because every field is wiring rather than behaviour:
/// which identity a token belongs to, which OS user it maps to, where the live terminals and the
/// control lease live, and what the host's transport accepts as one frame.
#[must_use]
pub fn build_terminal_session_entry(ports: TerminalSessionPorts) -> tddy_rpc::ServiceEntry {
    let server = TerminalSessionServiceServer::new(TerminalSessionServiceImpl::new(ports));
    tddy_rpc::ServiceEntry {
        name: "terminal_session.TerminalSessionService",
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}

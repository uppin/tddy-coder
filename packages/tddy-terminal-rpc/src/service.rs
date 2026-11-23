//! The `terminal_session.TerminalSessionService` implementation, and the entry a host's wiring
//! layer registers.
//!
//! All nine methods are terminal behaviour this crate already owns: seven of them are the
//! [`crate::bridge`] functions plus the auth gate in front of them, and the two lifecycle families
//! (multiple terminals per session, the single-screen control mutex) are the state a *host* keeps
//! about its own PTYs. That state is why the host ports (declared in `ports`, re-exported here)
//! are traits rather than values — resolving a session token to an OS user, spawning a terminal as
//! that user and holding the control lease are all the serving host's answers, not this
//! subsystem's behaviour.
//!
//! # What is deliberately not here
//!
//! Nothing resolves a `session_id` to a worktree, a task registry or a sandbox. The host's
//! [`TerminalRoster`] and [`TerminalSessionStore`](crate::session::TerminalSessionStore) impls do
//! that, because a terminal's *origin* (a `CliSessionManager` task, a sandboxed session, a
//! coder-local PTY) differs per host while the streaming, replay and control semantics above do
//! not.

mod control_relay;
mod entry;
mod ports;
mod recorded_history;

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::bridge::{
    resolved_terminal_id, serve_get_terminal_history_with, serve_send_terminal_input_to,
    serve_stream_session_terminal_io_with, serve_stream_terminal_output_with, MAIN_TERMINAL_ID,
};
use crate::proto::terminal_session::TerminalSessionService;
use crate::proto::terminal_session::{
    ClaimTerminalControlRequest, ClaimTerminalControlResponse, GetTerminalHistoryRequest,
    ListTerminalSessionsRequest, ListTerminalSessionsResponse, ListTmuxSessionsRequest,
    ListTmuxSessionsResponse, SendTerminalInputResponse, SessionTerminalInput,
    SessionTerminalOutput, StartTerminalSessionRequest, StartTerminalSessionResponse,
    StopTerminalSessionRequest, StopTerminalSessionResponse, StreamTerminalOutputRequest,
    TerminalControlEvent, TerminalHistoryChunk, TerminalSessionInfo, TmuxSessionInfo,
    WatchTerminalControlRequest,
};
use crate::session::TerminalSession;

use control_relay::{relay_control_changes, CONTROL_EVENT_CHANNEL_CAPACITY};
pub use entry::{build_terminal_session_entry, TERMINAL_SESSION_SERVICE};
pub use ports::*;
use recorded_history::serve_recorded_history;
pub use recorded_history::{NoRecordedHistory, RecordedTerminalHistory};

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

    /// Brings `session_id`'s main terminal back when it is the one addressed and the store has no
    /// running one (see [`MainReattach`]); a no-op for every other terminal and for a running main.
    async fn ensure_main(&self, os_user: &str, session_id: &str, terminal_id: &str) {
        if resolved_terminal_id(terminal_id) != MAIN_TERMINAL_ID {
            return;
        }
        if self
            .ports
            .terminals
            .get_terminal(session_id, MAIN_TERMINAL_ID)
            .await
            .is_none()
        {
            self.ports
                .main_reattach
                .reattach_main(os_user, session_id)
                .await;
        }
    }

    /// The live terminal a driving call addresses, once the caller is allowed to drive it.
    ///
    /// The terminal is resolved *before* the lease is checked because whether the lease applies is
    /// the terminal's own answer: one that reports itself not
    /// [`TerminalSession::requires_control`] is driven by the process that owns its PTY, which
    /// carries no control token, so demanding one would let any screen's claim sever the owner's
    /// own input. A terminal that resolves to nothing is still asked the lease question first, so a
    /// displaced screen is told it lost control rather than that the terminal is missing.
    async fn controlled_terminal(
        &self,
        session_id: &str,
        terminal_id: &str,
        control_token: &str,
    ) -> Result<Arc<dyn TerminalSession>, Status> {
        let resolved = self
            .ports
            .terminals
            .get_terminal(session_id, terminal_id)
            .await;
        if resolved
            .as_ref()
            .is_none_or(|session| session.requires_control())
        {
            self.require_control(session_id, control_token).await?;
        }
        resolved.ok_or_else(|| Status::not_found("terminal not found or not running"))
    }
}

/// A session id as the handlers key on it: whitespace-trimmed.
///
/// Trimmed in all nine methods rather than the six `the pre-unbundle monolithic RPC coordinate` trims in, so one
/// client cannot reach two different control leases — or two different terminals — for the same
/// session by sending the id with a stray space.
fn keyed_session_id(raw: &str) -> String {
    raw.trim().to_string()
}

#[async_trait]
impl TerminalSessionService for TerminalSessionServiceImpl {
    type StreamSessionTerminalIoStream = ReceiverStream<Result<SessionTerminalOutput, Status>>;

    /// Bidi terminal I/O. The first message carries the identity, the control token and the replay
    /// selection; authentication happens on it before the stream opens. Opening is watching and
    /// needs no lease — a screen without it is served the output with its grid ignored — while
    /// the bridge judges every input chunk against the lease and answers a refused one with a
    /// refusal frame, unless the terminal is one [`Self::controlled_terminal`] exempts.
    async fn stream_session_terminal_io(
        &self,
        request: Request<tddy_rpc::Streaming<SessionTerminalInput>>,
    ) -> Result<Response<Self::StreamSessionTerminalIoStream>, Status> {
        let mut in_stream = request.into_inner();
        let first = in_stream
            .next()
            .await
            .ok_or_else(|| Status::invalid_argument("stream ended before first message"))??;

        let os_user = self.os_user(&first.session_token)?;
        let session_id = keyed_session_id(&first.session_id);
        let terminal_id = resolved_terminal_id(&first.terminal_id).to_string();
        self.ensure_main(&os_user, &session_id, &terminal_id).await;
        // Watching needs no lease: every screen is served the output. Only writing is gated —
        // the grid here, and each input chunk in the bridge.
        let session = self
            .ports
            .terminals
            .get_terminal(&session_id, &terminal_id)
            .await
            .ok_or_else(|| Status::not_found("terminal not found or not running"))?;
        let mut first = first;
        if !self
            .may_resize_on_open(&session_id, &terminal_id, &first.control_token)
            .await
        {
            first.initial_cols = 0;
            first.initial_rows = 0;
        }

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
        let os_user = self.os_user(&req.session_token)?;
        req.session_id = keyed_session_id(&req.session_id);
        self.ensure_main(&os_user, &req.session_id, &req.terminal_id)
            .await;
        if !self
            .may_resize_on_open(&req.session_id, &req.terminal_id, &req.control_token)
            .await
        {
            req.initial_cols = 0;
            req.initial_rows = 0;
        }

        let rx = serve_stream_terminal_output_with(
            &*self.ports.terminals,
            req,
            self.ports.initial_frame_bytes,
        )
        .await?;
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    /// The browser-compatible input half, gated by [`Self::controlled_terminal`]: a displaced
    /// screen is told it lost the lease rather than that the terminal is missing.
    async fn send_terminal_input(
        &self,
        request: Request<SessionTerminalInput>,
    ) -> Result<Response<SendTerminalInputResponse>, Status> {
        let mut req = request.into_inner();
        let os_user = self.os_user(&req.session_token)?;
        req.session_id = keyed_session_id(&req.session_id);
        let terminal_id = resolved_terminal_id(&req.terminal_id).to_string();
        self.ensure_main(&os_user, &req.session_id, &terminal_id)
            .await;
        let session = self
            .controlled_terminal(&req.session_id, &terminal_id, &req.control_token)
            .await?;

        Ok(Response::new(serve_send_terminal_input_to(&session, req)))
    }

    type GetTerminalHistoryStream = ReceiverStream<Result<TerminalHistoryChunk, Status>>;

    /// One forward chunk of older output, for the scroll-up fill. Reading history is not driving
    /// the terminal, so it needs no control token — every screen watching a session may scroll.
    async fn get_terminal_history(
        &self,
        request: Request<GetTerminalHistoryRequest>,
    ) -> Result<Response<Self::GetTerminalHistoryStream>, Status> {
        let mut req = request.into_inner();
        let os_user = self.os_user(&req.session_token)?;
        req.session_id = keyed_session_id(&req.session_id);

        // A running terminal is answered from its live capture and never reads its recording. Only a
        // terminal that is not running is answered from what its host recorded of it.
        let terminal_id = resolved_terminal_id(&req.terminal_id);
        self.ensure_main(&os_user, &req.session_id, terminal_id)
            .await;
        let running = self
            .ports
            .terminals
            .get_terminal(&req.session_id, terminal_id)
            .await
            .is_some();
        let rx = if running {
            serve_get_terminal_history_with(
                &*self.ports.terminals,
                req,
                self.ports.initial_frame_bytes,
            )
            .await?
        } else {
            serve_recorded_history(
                &*self.ports.recorded_history,
                &os_user,
                &req,
                self.ports.initial_frame_bytes,
            )
            .await?
        };
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    // TODO(#unbundle-node-6): `StartTerminalSession` / `StopTerminalSession` document a
    // `control_token` "required when the session has an active terminal controller", but neither
    // checks it — the same gap `the pre-unbundle monolithic RPC coordinate` has. Closing it changes who may
    // start a shell in someone else's session, which is a product decision rather than a move.
    async fn start_terminal_session(
        &self,
        request: Request<StartTerminalSessionRequest>,
    ) -> Result<Response<StartTerminalSessionResponse>, Status> {
        let req = request.into_inner();
        let os_user = self.os_user(&req.session_token)?;
        let subject = self.github_user(&req.session_token)?;
        let session_id = keyed_session_id(&req.session_id);

        let launch = TerminalLaunch {
            kind: TerminalKind::from_wrapped_request(&req.kind, &req.tmux_session, &req.wrappers)?,
            login_shell: crate::login_shell::login_shell_for(&os_user),
            os_user,
            subject,
        };
        let terminal_id = self.ports.roster.start(&session_id, &launch).await?;
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

        let mut launch_chains = self.ports.roster.launch_chains(&session_id).await?;
        let terminals = self
            .ports
            .roster
            .list(&session_id)
            .await
            .into_iter()
            .map(|terminal| TerminalSessionInfo {
                launch: launch_chains
                    .remove(&terminal.terminal_id)
                    .unwrap_or_default(),
                terminal_id: terminal.terminal_id,
                kind: terminal.kind,
                pid: terminal.pid,
                tmux_session: terminal.tmux_session.unwrap_or_default(),
            })
            .collect();
        Ok(Response::new(ListTerminalSessionsResponse { terminals }))
    }

    async fn list_tmux_sessions(
        &self,
        request: Request<ListTmuxSessionsRequest>,
    ) -> Result<Response<ListTmuxSessionsResponse>, Status> {
        let req = request.into_inner();
        let subject = self.github_user(&req.session_token)?;
        let os_user = self.os_user(&req.session_token)?;
        let session_id = keyed_session_id(&req.session_id);
        let sessions = self
            .ports
            .tmux
            .list_for_session(&TmuxListing {
                os_user: &os_user,
                subject: &subject,
                session_id: &session_id,
            })
            .await?
            .into_iter()
            .map(|session| TmuxSessionInfo {
                name: session.name,
                windows: session.windows,
                attached: session.attached,
            })
            .collect();
        Ok(Response::new(ListTmuxSessionsResponse { sessions }))
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

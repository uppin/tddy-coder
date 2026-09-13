//! The coder's `terminal_session.TerminalSessionService` coordinate.
//!
//! The coder is the *second* server of the terminal family: a session reached over LiveKit is
//! answered by this process, and the same session reached over HTTP is answered by the daemon.
//! The repo has already been bitten by those two disagreeing — changeset
//! `2026-08-02-activities-tail-first-autoscroll` records a session that "would have opened
//! tail-first when reached over HTTP and head-first when reached over LiveKit" — so the handlers
//! are not written here at all. [`tddy_terminal_rpc::build_terminal_session_entry`] is the same
//! constructor `tddy-daemon` registers, and what this module supplies is only the three answers
//! that are genuinely the coder's: which terminals it runs, who may drive them, and which OS user
//! they belong to.
//!
//! # Seven of the nine
//!
//! `StreamSessionTerminalIO` and `WatchTerminalControl` are refused here — see
//! [`CoderTerminalSessionRpc`] for why each is a lie rather than an omission, and
//! `docs/dev/todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md` for what a client
//! sees as a result.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{BidiStreamOutput, RpcMessage, RpcResult, RpcService, ServiceEntry, Status};
use tddy_terminal_rpc::{
    build_terminal_session_entry, ControlChange, ControlClaim, TerminalControl, TerminalDescriptor,
    TerminalRoster, TerminalSessionPorts,
};
use tokio::sync::broadcast;

use super::connection_service_participant::SessionConnectionService;
use super::terminal_session_adapter::CoderTerminalSessionStore;

/// The identity every caller of this participant is served as.
///
/// The coder has no signer with which to verify a `session_token` — it is admitted to its own
/// LiveKit room by a token the *daemon* minted, and every caller that reaches this participant was
/// admitted to the same room by the same authority. So the token is not re-checked here, and the
/// port answers with the one identity this process serves. `run.rs` states the same reasoning for
/// the unauthenticated `TokenService` it registers beside this one.
const CODER_IDENTITY: &str = "coder";

/// The two methods the coder's terminal coordinate refuses, and the bidi one among them.
const UNSERVED_METHODS: [&str; 2] = ["StreamSessionTerminalIO", "WatchTerminalControl"];

/// The `terminal_session.TerminalSessionService` entry the coder's participant registers.
///
/// Built from the same [`SessionConnectionService`] the `the pre-unbundle monolithic RPC coordinate` entry is, so
/// the session's tools and its terminals reach one
/// [`TerminalManager`](super::terminal_manager::TerminalManager) and one control lease — a second
/// manager here would mean a terminal started through this coordinate was invisible to the session
/// that owns it.
///
/// # A process that cannot name its own OS user serves no terminal
///
/// [`current_os_user`](super::terminal_manager::current_os_user) reads the passwd entry of this
/// process's effective uid and falls back to `$USER`. If both come up empty, all seven served
/// methods answer `PERMISSION_DENIED`, where the pre-move `the pre-unbundle monolithic RPC coordinate` handlers
/// had no such gate. That is deliberate and not softened here: every one of those methods reaches
/// a PTY, `StartTerminalSession` resolves the login shell *from the named user*, and a terminal
/// whose owning OS user this process cannot name is one nothing downstream can attribute. Naming
/// some other user instead — a literal, a uid, "root" — would spawn a shell under an identity
/// nobody asked for, which is worse than a refusal a caller can read.
#[must_use]
pub fn coder_terminal_session_entry(svc: Arc<SessionConnectionService>) -> ServiceEntry {
    let served = build_terminal_session_entry(TerminalSessionPorts {
        github_users: Arc::new(|_session_token: &str| Some(CODER_IDENTITY.to_string())),
        os_users: Arc::new(|_identity: &str| super::terminal_manager::current_os_user()),
        terminals: Arc::new(CoderTerminalSessionStore::new(Arc::clone(
            &svc.terminal_manager,
        ))),
        control: Arc::new(CoderTerminalControl::new(Arc::clone(&svc))),
        roster: Arc::new(CoderTerminalRoster::new(Arc::clone(&svc))),
        initial_frame_bytes: tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES,
    });
    ServiceEntry {
        name: served.name,
        service: Arc::new(CoderTerminalSessionRpc {
            served: served.service,
        }) as Arc<dyn RpcService>,
    }
}

/// The shared `terminal_session.TerminalSessionService`, with the two methods the coder has no
/// honest answer for refused.
///
/// * `WatchTerminalControl` reports whether the caller drives the session's terminals. The coder's
///   lease is a permanent grant ([`CoderTerminalControl`]), so this participant would tell *every*
///   watching screen that it is the controller — including one the daemon's real lease has just
///   displaced, which is exactly the screen the event exists to correct.
/// * `StreamSessionTerminalIO` carries terminal bytes bidirectionally. This participant already
///   serves `terminal.TerminalService/StreamTerminalIO`, which is the wire the web opens against
///   it; a second bidi terminal on the same participant is net-new behaviour rather than a
///   relocation of anything.
///
/// Refusing them is what keeps the seven that *are* served identical to the daemon's — they run
/// the crate's handlers untouched, through this one delegation.
struct CoderTerminalSessionRpc {
    served: Arc<dyn RpcService>,
}

impl CoderTerminalSessionRpc {
    fn refusal(method: &str) -> Status {
        Status::unimplemented(format!(
            "the coder session participant does not serve \
             terminal_session.TerminalSessionService/{method}; reach the daemon for it"
        ))
    }

    fn serves(method: &str) -> bool {
        !UNSERVED_METHODS.contains(&method)
    }
}

#[async_trait]
impl RpcService for CoderTerminalSessionRpc {
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        Self::serves(method) && self.served.is_bidi_stream(service, method)
    }

    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        if !Self::serves(method) {
            return RpcResult::Unary(Err(Self::refusal(method)));
        }
        self.served.handle_rpc(service, method, message).await
    }

    async fn handle_rpc_stream(
        &self,
        service: &str,
        method: &str,
        messages: &[RpcMessage],
    ) -> RpcResult {
        if !Self::serves(method) {
            return RpcResult::Unary(Err(Self::refusal(method)));
        }
        self.served
            .handle_rpc_stream(service, method, messages)
            .await
    }

    async fn start_bidi_stream(
        &self,
        service: &str,
        method: &str,
        input_rx: tokio::sync::mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, Status> {
        if !Self::serves(method) {
            return Err(Self::refusal(method));
        }
        self.served
            .start_bidi_stream(service, method, input_rx)
            .await
    }
}

/// The coder's control lease: a permanent grant to whoever asks.
///
/// The coder owns exactly one session and its terminals are its own, so there is no second screen
/// to arbitrate against here — the daemon's control registry is the one that arbitrates, and this
/// process is not it. [`SessionConnectionService::claim_terminal_control`] is the single source of
/// the grant, so the token a caller is handed is the same one on either of this participant's
/// coordinates.
pub(crate) struct CoderTerminalControl {
    svc: Arc<SessionConnectionService>,
    /// Nothing is ever sent on this: the coder's lease never changes hands. Held so `subscribe`
    /// hands out a live receiver rather than a closed one.
    changes: broadcast::Sender<ControlChange>,
}

impl CoderTerminalControl {
    #[must_use]
    pub(crate) fn new(svc: Arc<SessionConnectionService>) -> Self {
        let (changes, _) = broadcast::channel(1);
        CoderTerminalControl { svc, changes }
    }
}

#[async_trait]
impl TerminalControl for CoderTerminalControl {
    /// The lease's own `granted` flag decides the answer, rather than every outcome being reported
    /// as a grant. [`SessionConnectionService::claim_terminal_control`] grants unconditionally
    /// today, so this is one arm in practice — but mapping a refusal to a grant is how a screen
    /// would be handed a token the lease just declined to give it, and the flag the pre-move
    /// handler propagated is the only thing that says which happened. A refusal names no rival
    /// holder because this process arbitrates between none: `holder_screen_id` answers `None` for
    /// the same reason.
    async fn claim(&self, _session_id: &str, screen_id: &str, steal: bool) -> ControlClaim {
        let claim = self.svc.claim_terminal_control(screen_id, steal);
        if claim.granted {
            ControlClaim::Granted {
                control_token: claim.control_token,
            }
        } else {
            ControlClaim::Denied {
                holder_screen_id: String::new(),
            }
        }
    }

    /// Every token drives the coder's terminals, including the empty one — the grant above is
    /// unconditional, so refusing here would refuse a caller this process just said yes to.
    async fn verify(&self, _session_id: &str, _control_token: &str) -> bool {
        true
    }

    async fn holder_screen_id(&self, _session_id: &str) -> Option<String> {
        None
    }

    fn subscribe(&self) -> broadcast::Receiver<ControlChange> {
        self.changes.subscribe()
    }
}

/// The bash "tabs" this coder runs, backed by its
/// [`TerminalManager`](super::terminal_manager::TerminalManager).
///
/// The manager is single-session, so `session_id` *selects* nothing here — every terminal it holds
/// belongs to the one session this process is running, and `stop` and `list` resolve by
/// `terminal_id` alone (as does [`CoderTerminalSessionStore`]).
/// [`start`](CoderTerminalRoster::start) is the one method that would *record* the id it is given,
/// which is why it is the one that checks it against the session this participant serves.
pub struct CoderTerminalRoster {
    svc: Arc<SessionConnectionService>,
}

impl CoderTerminalRoster {
    #[must_use]
    pub fn new(svc: Arc<SessionConnectionService>) -> Self {
        CoderTerminalRoster { svc }
    }
}

#[async_trait]
impl TerminalRoster for CoderTerminalRoster {
    /// Started shells run in the session's worktree — the coder's own agent working directory —
    /// and as the coder's own OS user, because this process already *is* that user.
    ///
    /// # The id the PTY is registered under is this participant's, never the request's
    ///
    /// A started terminal is recorded in the PTY and task registries under a session id
    /// ([`tddy_pty::PtySpawnSpec::session_id`], which becomes `TaskHandle::session_id` — the field
    /// auth scoping keys on), so the id that reaches
    /// [`TerminalManager::start_terminal`](super::terminal_manager::TerminalManager::start_terminal)
    /// is `self.svc.session_id`: the one session this process runs. Taking the request's would let
    /// a caller choose the name its terminal is filed under.
    ///
    /// A request naming a *different* session is **refused** rather than quietly served as this
    /// one. The daemon's roster cannot serve a session it does not hold either — it looks the
    /// session up and answers `FAILED_PRECONDITION` when it is absent — so refusing (with that
    /// same code) is what keeps the two servers answering a misaddressed start the same way, which
    /// is the point of serving one implementation on both. Quietly substituting would have this
    /// server succeed where the other fails, and tell the caller a terminal now exists in a
    /// session it does not.
    async fn start(&self, session_id: &str, shell_path: &str) -> Result<String, Status> {
        let served = self.svc.session_id.trim();
        if session_id != served {
            return Err(Status::failed_precondition(format!(
                "this session participant runs session {served}, not {session_id}"
            )));
        }
        self.svc
            .terminal_manager
            .start_terminal(served, self.svc.worktree.clone(), shell_path)
            .await
            .map(|handle| handle.terminal_id.clone())
            .map_err(|e| Status::internal(format!("failed to start terminal: {e}")))
    }

    async fn stop(&self, _session_id: &str, terminal_id: &str) -> bool {
        self.svc.terminal_manager.stop_terminal(terminal_id).await
    }

    /// The started shells only. The session's main terminal is the coder's *own* agent PTY, which
    /// this process serves on `terminal.TerminalService` and never registered as a tab.
    async fn list(&self, _session_id: &str) -> Vec<TerminalDescriptor> {
        self.svc
            .terminal_manager
            .list_terminals()
            .await
            .iter()
            .map(|handle| TerminalDescriptor {
                terminal_id: handle.terminal_id.clone(),
                kind: handle.kind.clone(),
                pid: handle.pid,
            })
            .collect()
    }
}

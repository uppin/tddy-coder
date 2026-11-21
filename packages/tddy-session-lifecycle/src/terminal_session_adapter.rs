//! Adapters binding the daemon's own terminal owners to [`tddy_terminal_rpc::session`]'s traits, so
//! every terminal RPC — whichever coordinate it arrives on — is served by the one streaming bridge
//! in `tddy-terminal-rpc`.
//!
//! Two owners, one store. A session's terminal is either a claude-cli [`PtyHandle`] out of
//! [`CliSessionManager`] or the PTY of a sandboxed session out of
//! [`SandboxSessionManager`](tddy_daemon_sandbox::sandbox_session::SandboxSessionManager);
//! [`DaemonTerminalSessionStore`] resolves the sandbox first and falls back to the CLI manager.
//! Before `#unbundle` node 6 the sandbox case was a per-handler `if let Some(sandbox) = …` branch
//! in four RPCs, one of which carried its own copy of the bridge's replay/offset loop.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use tddy_daemon_sandbox::sandbox_session::{SandboxSessionManager, SandboxSessionState};
use tddy_daemon_sandbox::tab_jails::TabJails;
use tddy_task::TerminalCapture;
use tddy_terminal_rpc::bridge::MAIN_TERMINAL_ID;
use tokio::sync::{broadcast, watch};

use crate::cli_session_manager::{CliSessionManager, PtyHandle};

/// A live claude-cli terminal exposed to the unified bridge.
pub struct DaemonTerminalSession {
    handle: Arc<PtyHandle>,
}

impl DaemonTerminalSession {
    pub fn new(handle: Arc<PtyHandle>) -> Self {
        DaemonTerminalSession { handle }
    }

    /// The underlying handle, for daemon handlers that need direct access (e.g. control-token checks).
    pub fn handle(&self) -> &Arc<PtyHandle> {
        &self.handle
    }
}

#[async_trait]
impl tddy_terminal_rpc::session::TerminalSession for DaemonTerminalSession {
    fn capture(&self) -> Arc<Mutex<TerminalCapture>> {
        Arc::clone(&self.handle.capture)
    }

    fn subscribe_stdout(&self) -> broadcast::Receiver<Bytes> {
        self.handle.stdout_tx.subscribe()
    }

    fn subscribe_pty_done(&self) -> watch::Receiver<bool> {
        self.handle.pty_done.clone()
    }

    fn subscribe_acked_offset(&self) -> watch::Receiver<u64> {
        self.handle.subscribe_acked_offset()
    }

    async fn resize(&self, rows: u16, cols: u16) {
        self.handle.resize(rows, cols);
    }

    fn send_input(&self, data: Bytes, input_offset: u64) {
        self.handle.send_input(data, input_offset);
    }

    fn trigger_redraw(&self) {
        self.handle.trigger_redraw();
    }
}

/// The PTY of a sandboxed session exposed to the unified bridge.
///
/// A sandboxed session's terminal lacks four of the things the trait asks for, and this adapter
/// answers each of the four absences with what the surface did *before* it went through the bridge
/// rather than by inventing the capability:
///
/// - **Input offsets are not acknowledged.** `stdin_tx` is a plain channel into the jail with no
///   applied-byte counter behind it, so [`Self::subscribe_acked_offset`] hands out a watch that
///   never advances and the bridge emits no ACK frame — matching the old sandbox path, which had
///   no ACK source either. `input_offset` is likewise dropped rather than acked back.
/// - **There is no SIGWINCH.** Nothing in `tddy-daemon-sandbox` holds the jail's PTY master, so
///   [`Self::resize`] is a no-op, as the old sandbox path was.
/// - **There is no process-exit watch.** What ended the old sandbox stream was the stdout broadcast
///   closing when the jail died, so [`Self::subscribe_pty_done`] derives the watch from exactly
///   that: a probe task fires it once the broadcast closes, and stands down when the stream that
///   asked for it is gone.
/// - **There is no control token.** The in-jail bridge that drives this PTY sends none
///   (`tddy-sandbox-app`'s `daemon_client`), and has no way to claim one, so
///   [`Self::requires_control`] answers `false` — as the old sandbox path did by forwarding to
///   `stdin_tx` and returning before the lease was ever consulted.
pub(crate) struct SandboxTerminalSession {
    state: Arc<SandboxSessionState>,
}

impl SandboxTerminalSession {
    fn new(state: Arc<SandboxSessionState>) -> Self {
        SandboxTerminalSession { state }
    }
}

#[async_trait]
impl tddy_terminal_rpc::session::TerminalSession for SandboxTerminalSession {
    fn capture(&self) -> Arc<Mutex<TerminalCapture>> {
        Arc::clone(&self.state.capture)
    }

    fn subscribe_stdout(&self) -> broadcast::Receiver<Bytes> {
        self.state.stdout_tx.subscribe()
    }

    /// A watch fired by the stdout broadcast closing — the jail's death, and what ended this
    /// stream before it went through the bridge.
    ///
    /// The sender must outlive the receiver: the bridge's live loop `break`s on *either* outcome of
    /// `pty_done.changed()`, so a dropped sender would end every sandbox stream the instant it
    /// opened. The probe task owns it, and stands down when the last receiver goes away so a
    /// session reconnected to many times does not accumulate probes.
    fn subscribe_pty_done(&self) -> watch::Receiver<bool> {
        let (pty_done_tx, pty_done_rx) = watch::channel(false);
        let mut stdout_rx = self.state.stdout_tx.subscribe();
        tokio::spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                tokio::select! {
                    _ = pty_done_tx.closed() => return,
                    received = stdout_rx.recv() => match received {
                        Ok(_) => {}
                        Err(RecvError::Lagged(_)) => {}
                        Err(RecvError::Closed) => break,
                    },
                }
            }
            let _ = pty_done_tx.send(true);
        });
        pty_done_rx
    }

    /// A watch that never advances: sandbox input is not acknowledged, so the bridge emits no ACK
    /// frames. The sender is dropped here on purpose — the bridge reads the initial `0` (which it
    /// does not frame), then stops polling for changes.
    fn subscribe_acked_offset(&self) -> watch::Receiver<u64> {
        watch::channel(0u64).1
    }

    /// The lease arbitrates between browser *screens*; the caller here is the in-jail bridge that
    /// owns this PTY and sends no control token at all.
    ///
    /// Gating it on the lease means a screen claiming control severs the jail's own I/O: the open
    /// is refused `FAILED_PRECONDITION`, or an open stream's input forwarder ends while output
    /// keeps flowing, so the user's typing stops with no error frame and no visible cause.
    fn requires_control(&self) -> bool {
        false
    }

    /// Nothing here holds the jail's PTY master, so there is no SIGWINCH to send.
    fn resizable(&self) -> bool {
        false
    }

    /// Never reached: the bridge does not resize a terminal that reports itself unresizable.
    async fn resize(&self, _rows: u16, _cols: u16) {}

    /// Forward the bytes into the jail. `input_offset` is dropped because nothing on the far side
    /// counts applied bytes back — acking an offset this host never confirmed would collapse the
    /// client's un-acknowledged-input overlay on a promise it cannot keep.
    fn send_input(&self, data: Bytes, _input_offset: u64) {
        let _ = self.state.stdin_tx.send(data);
    }
}

/// A [`TerminalSessionStore`](tddy_terminal_rpc::session::TerminalSessionStore) over both of the
/// daemon's terminal owners. Auth (session-token resolution + OS-user mapping) and control-token
/// checks remain the caller's responsibility — this store only resolves the live terminal.
#[derive(Clone)]
pub struct DaemonTerminalSessionStore {
    manager: Arc<CliSessionManager>,
    sandboxes: Arc<SandboxSessionManager>,
    /// The jails of sandboxed sessions' jailed shell tabs (`#session-liveness` 5/8).
    tab_jails: Arc<TabJails>,
}

#[async_trait]
impl tddy_daemon_kernel::live_sessions::HeldSessions for DaemonTerminalSessionStore {
    async fn live_sessions(&self) -> tddy_daemon_kernel::live_sessions::LiveSessions {
        DaemonTerminalSessionStore::live_sessions(self).await
    }

    async fn stop_session(&self, session_id: &str) {
        self.manager.stop_session(session_id).await;
        if let Some(sandbox) = self.sandboxes.get(session_id).await {
            sandbox.stop();
        }
        self.tab_jails.stop_session(session_id).await;
    }
}

impl DaemonTerminalSessionStore {
    /// A store over `manager` and `sandboxes`, holding no tab jails until [`Self::with_tab_jails`].
    pub fn new(manager: Arc<CliSessionManager>, sandboxes: Arc<SandboxSessionManager>) -> Self {
        DaemonTerminalSessionStore {
            manager,
            sandboxes,
            tab_jails: Arc::new(TabJails::new()),
        }
    }

    /// Resolve jailed shell tabs out of `tab_jails`.
    #[must_use]
    pub fn with_tab_jails(mut self, tab_jails: Arc<TabJails>) -> Self {
        self.tab_jails = tab_jails;
        self
    }

    /// A tab other than a session's main: its own jail's terminal when it runs in one, else the
    /// CLI manager's host terminal of that id.
    async fn tab_terminal(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> Option<Arc<dyn tddy_terminal_rpc::session::TerminalSession>> {
        if let Some(jail) = self.tab_jails.get(session_id, terminal_id).await {
            return Some(Arc::new(SandboxTerminalSession::new(jail))
                as Arc<dyn tddy_terminal_rpc::session::TerminalSession>);
        }
        self.manager
            .get_terminal(session_id, terminal_id)
            .await
            .map(|handle| {
                Arc::new(DaemonTerminalSession::new(handle))
                    as Arc<dyn tddy_terminal_rpc::session::TerminalSession>
            })
    }

    /// Signals the main agent this daemon holds for the session in `session_dir`; refused, with
    /// nothing signalled, when none is held (`tddy_agent_launch::svc_signal_delete_session`).
    pub async fn signal_session_dir(
        &self,
        session_dir: &std::path::Path,
        session_id: &str,
        signal: i32,
    ) -> Result<String, tddy_rpc::Status> {
        tddy_agent_launch::svc_signal_delete_session::signal_held_session(
            &self.manager,
            &self.sandboxes,
            session_dir,
            session_id,
            signal,
        )
        .await
    }

    /// The sessions this daemon holds a live agent for: its terminals', its sandboxed runners' and its
    /// jailed agent tabs'.
    /// The live set every session listing reads (`list_sessions_in_dir`).
    pub async fn live_sessions(&self) -> tddy_daemon_kernel::live_sessions::LiveSessions {
        self.manager
            .live_agent_sessions()
            .await
            .union(self.sandboxes.live_sessions().await)
            .union(self.tab_jails.live_agent_sessions().await)
    }
}

#[async_trait]
impl tddy_terminal_rpc::session::TerminalSessionStore for DaemonTerminalSessionStore {
    /// A sandboxed session wins the lookup for its main: while a session is running in a jail its
    /// main PTY is the jail's, and the CLI manager holds nothing for it.
    ///
    /// Any other `terminal_id` of a sandboxed session is resolved by its owner: a jailed shell
    /// tab's own jail, or the CLI manager for an "On host" tab (`#session-liveness` 5/8).
    async fn get_terminal(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> Option<Arc<dyn tddy_terminal_rpc::session::TerminalSession>> {
        if terminal_id != MAIN_TERMINAL_ID {
            return self.tab_terminal(session_id, terminal_id).await;
        }
        if let Some(state) = self.sandboxes.get(session_id).await {
            return Some(Arc::new(SandboxTerminalSession::new(state))
                as Arc<dyn tddy_terminal_rpc::session::TerminalSession>);
        }
        self.manager
            .get_terminal(session_id, terminal_id)
            .await
            .map(|handle| {
                Arc::new(DaemonTerminalSession::new(handle))
                    as Arc<dyn tddy_terminal_rpc::session::TerminalSession>
            })
    }
}

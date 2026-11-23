//! The host ports [`super::TerminalSessionServiceImpl`] is built over: the identity resolvers, the
//! control lease, the terminal roster and the launch request it receives, gathered in
//! [`TerminalSessionPorts`]. Re-exported from [`crate::service`], so their paths are unchanged.

use crate::session::TerminalSessionStore;
use async_trait::async_trait;
use std::sync::Arc;
use tddy_rpc::Status;
use tokio::sync::broadcast;

mod terminal_kind;
pub use terminal_kind::*;

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

    /// Give the lease up: `true` when `control_token` was the session's current lease and the
    /// session is now uncontrolled, `false` when it was not — another screen took it since, or it
    /// was never held — and nothing changed (`#orchestra` 7/9).
    async fn release(&self, session_id: &str, control_token: &str) -> bool;

    /// Lease changes across all sessions; [`watch_terminal_control`] filters to its own.
    ///
    /// [`watch_terminal_control`]: TerminalSessionService::watch_terminal_control
    fn subscribe(&self) -> broadcast::Receiver<ControlChange>;
}

/// One running terminal as `ListTerminalSessions` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalDescriptor {
    pub terminal_id: String,
    /// The reserved main tool's session type, `"bash"` for a started shell, or the session type of
    /// the CLI harness a started agent terminal runs.
    pub kind: String,
    pub pid: u32,
    /// The tmux session a [`TerminalKind::Tmux`] terminal is attached to; `None` for every other
    /// kind.
    pub tmux_session: Option<String>,
}

/// One terminal start as the roster receives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalLaunch {
    pub kind: TerminalKind,
    /// The session user's login shell, resolved by [`crate::login_shell::login_shell_for`]; what a
    /// [`TerminalKind::Bash`] terminal runs.
    pub login_shell: String,
    /// The OS user the request's token maps to — who a [`TerminalKind::Tmux`] terminal runs as, so
    /// it reaches that user's own tmux server.
    pub os_user: String,
    /// The GitHub user the request's session token resolves to — whose vault a tab that must act for
    /// the session's owner (a jailed shell's provider route, `#session-liveness` 5/8) reads.
    pub subject: String,
}

/// One tmux session on a host's tmux server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxSession {
    pub name: String,
    /// Windows in the session.
    pub windows: u32,
    /// Clients attached to it.
    pub attached: u32,
}

/// Who asks for a session's tmux listing, and for which session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TmuxListing<'a> {
    /// The OS user the request's token maps to: whose tmux server a host listing reads.
    pub os_user: &'a str,
    /// The GitHub user the request's token resolves to: whose credentials reach a target.
    pub subject: &'a str,
    /// The session whose "+" menu asks.
    pub session_id: &'a str,
}

/// The tmux sessions a host can attach a terminal to.
#[async_trait]
pub trait TmuxHost: Send + Sync {
    /// The sessions on `os_user`'s tmux server: empty when no server runs, `FAILED_PRECONDITION`
    /// when the host has no tmux.
    async fn list(&self, os_user: &str) -> Result<Vec<TmuxSession>, Status>;

    /// The sessions a tmux tab of `listing.session_id` can attach to: the host's, by default; a
    /// daemon whose session runs on a target lists the target's (`#target-tmux` 2/2).
    async fn list_for_session(
        &self,
        listing: &TmuxListing<'_>,
    ) -> Result<Vec<TmuxSession>, Status> {
        self.list(listing.os_user).await
    }
}

/// A host that offers no tmux: every listing is refused.
pub struct NoTmux;

#[async_trait]
impl TmuxHost for NoTmux {
    async fn list(&self, _os_user: &str) -> Result<Vec<TmuxSession>, Status> {
        Err(Status::failed_precondition(
            "this session's terminals run bash only; tmux is not offered here",
        ))
    }
}

/// The terminals a host runs for a session: start one, stop one, list them.
///
/// A port because every step needs host state this crate has none of — which worktree the session
/// checked out and which task registry owns the child. Which *shell* to run is not among them:
/// [`crate::login_shell::login_shell_for`] answers that here, so a host cannot start a terminal in
/// a shell the target user does not have.
#[async_trait]
pub trait TerminalRoster: Send + Sync {
    /// Start the terminal `launch` describes for `session_id`, and return its fresh id (never
    /// [`MAIN_TERMINAL_ID`]).
    async fn start(&self, session_id: &str, launch: &TerminalLaunch) -> Result<String, Status>;

    /// Stop a started terminal. `false` when the session has no such terminal. The caller has
    /// already refused [`MAIN_TERMINAL_ID`], which is not stoppable through this surface.
    async fn stop(&self, session_id: &str, terminal_id: &str) -> bool;

    /// Stop a started terminal on behalf of `subject`, the GitHub user the request's token resolves
    /// to: whose credentials reach a target, where a tab's tmux session is ended with it
    /// (`#target-tmux` 2/2). By default, [`Self::stop`].
    async fn stop_as(&self, session_id: &str, terminal_id: &str, subject: &str) -> bool {
        let _ = subject;
        self.stop(session_id, terminal_id).await
    }

    /// Every running terminal of `session_id`, including [`MAIN_TERMINAL_ID`].
    async fn list(&self, session_id: &str) -> Vec<TerminalDescriptor>;

    /// The launch chain each of `session_id`'s terminals was started through, by terminal id,
    /// outermost first — what the session's tab roster records. A host that keeps no roster answers
    /// none, and a terminal with no recorded chain is listed with an empty one.
    async fn launch_chains(
        &self,
        _session_id: &str,
    ) -> Result<
        std::collections::HashMap<String, Vec<crate::proto::terminal_session::LaunchNode>>,
        Status,
    > {
        Ok(std::collections::HashMap::new())
    }
}

/// Brings a session's `main` terminal back when the host has none registered but its agent still
/// runs — a tmux-run agent that outlived its client or its daemon (`#tmux-agent` 2/3).
///
/// Consulted by the stream, history and input handlers for the `main` terminal **only** when
/// [`TerminalSessionPorts::terminals`] has no running one; the handler then asks the store again.
#[async_trait]
pub trait MainReattach: Send + Sync {
    /// Re-attach `session_id`'s main terminal on behalf of `os_user` (the requester's OS user, as
    /// the service resolved it from the token). `true` when a main terminal is registered now;
    /// `false` when the session's agent is not running, or the host has nothing to re-attach.
    /// Single-flight per session: concurrent first accesses attach one client.
    async fn reattach_main(&self, os_user: &str, session_id: &str) -> bool;
}

/// A host whose main terminals end with their process: nothing is ever re-attached.
pub struct NoMainReattach;

#[async_trait]
impl MainReattach for NoMainReattach {
    async fn reattach_main(&self, _os_user: &str, _session_id: &str) -> bool {
        false
    }
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
    /// The history of a terminal that is no longer running — `GetTerminalHistory` reads it only when
    /// [`Self::terminals`] has no running terminal by that id.
    pub recorded_history: Arc<dyn super::RecordedTerminalHistory>,
    /// Brings the `main` terminal back when [`Self::terminals`] has none running.
    pub main_reattach: Arc<dyn MainReattach>,
    /// The tmux sessions `ListTmuxSessions` answers with.
    pub tmux: Arc<dyn TmuxHost>,
    /// The per-frame replay budget: how many bytes of retained output one open frame may carry.
    ///
    /// Injected because it is a *transport* limit — the largest message the host's gRPC / LiveKit
    /// link accepts — rather than a property of the capture ring. Hosts that have no reason to
    /// choose pass [`crate::bridge::DEFAULT_INITIAL_FRAME_BYTES`].
    pub initial_frame_bytes: usize,
}

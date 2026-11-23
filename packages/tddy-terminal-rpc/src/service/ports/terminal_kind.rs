//! What a started terminal runs — [`TerminalKind`] — and how a `StartTerminalSessionRequest`'s kind,
//! tmux session and wrappers are read into one. Split out of `ports.rs` at the 500-line budget
//! (`#target-tmux` 2/2).

use tddy_rpc::Status;

/// What a started terminal runs, as `StartTerminalSessionRequest.kind` names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalKind {
    /// The session user's login shell — an empty kind or `"bash"`.
    Bash,
    /// A CLI harness, by its session type (`"warp-cli"`, …). Whether the host runs it is the
    /// roster's to decide: only the host knows its harness registry.
    Agent(String),
    /// A tmux client attached to `session` on the host's tmux server for the session's OS user —
    /// kind `"tmux"` with a non-empty `tmux_session`. Stopping the terminal detaches it.
    Tmux { session: String },
    /// A tmux client on a **new** tmux session `session` started in the session's worktree (or
    /// attached, if one of that name appeared meanwhile) — kind `"tmux-new"`.
    TmuxNew { session: String },
    /// A CLI harness, by its session type, run in a tmux session of its own
    /// (`tddy-<session_id>-<terminal_id>`) as the session's OS user, the tab being a client of it —
    /// an agent kind with `wrappers: [tmux]`. A client that ends while its tmux session lives
    /// leaves the agent running.
    TmuxWrappedAgent { agent: String },
    /// The session user's login shell in a jail of its own over the session's worktree and sandbox
    /// grants — an empty kind or `"bash"` with `wrappers: [sandbox]`. Only a sandboxed session runs
    /// one; the jail ends with the tab (`#session-liveness` 5/8).
    SandboxedShell,
    /// A CLI harness, by its session type, in a jail of its own over the session's worktree and the
    /// agent's sandbox grants, in a conversation of its own — an agent kind with `wrappers:
    /// [sandbox]`. Only a sandboxed session runs one; the jail ends with the tab, and the tab keeps
    /// its session live while its agent runs (`#session-liveness` 7/8).
    SandboxedAgent { agent: String },
    /// The target user's login shell on the session's **target** — the machine its `.session.yaml`
    /// `target` names — in the session's worktree there, run through the session's remote shell as
    /// a local tab terminal: an empty kind or `"bash"` with `wrappers: [target]`. Only a target
    /// session runs one; it writes nothing of the session's main on the target
    /// (`#session-liveness` 6/8).
    TargetShell,
    /// A CLI harness, by its session type, on the session's **target** — found by its bare command on
    /// the target login's own `PATH`, in the session's worktree there, in a conversation of its own
    /// settled on the target — run through the session's remote shell as a local tab terminal: an
    /// agent kind with `wrappers: [target]`. Only a target session runs one; the daemon's host needs
    /// no copy of the agent, and nothing of the session's main is written on the target
    /// (`#session-liveness` 8/8).
    TargetAgent { agent: String },
    /// A tmux client attached to `session` on the session's **target**, through the target login's
    /// own tmux, in the session's worktree there — kind `"tmux"` with `wrappers: [target]`
    /// (`#target-tmux` 2/2).
    TargetTmux { session: String },
    /// A tmux client on a new tmux session `session` on the session's **target**, started in the
    /// session's worktree there (or attached, if one of that name is there) — kind `"tmux-new"` with
    /// `wrappers: [target]` (`#target-tmux` 2/2).
    TargetTmuxNew { session: String },
    /// A CLI harness, by its session type, run in a tmux session of its own on the session's
    /// **target**, the tab being a client of it — an agent kind with `wrappers: [target, tmux]`
    /// (`#target-tmux` 2/2). Closing the tab kills that tmux session there.
    TargetTmuxAgent { agent: String },
}

/// The wrapper kind `StartTerminalSessionRequest.wrappers` names for a tmux-wrapped agent.
pub const TMUX_WRAPPER_KIND: &str = "tmux";

/// The wrapper kind `StartTerminalSessionRequest.wrappers` names for a tab run in a jail of its own.
pub const SANDBOX_WRAPPER_KIND: &str = "sandbox";

/// The wrapper kind `StartTerminalSessionRequest.wrappers` names for a tab run on the session's
/// target.
pub const TARGET_WRAPPER_KIND: &str = "target";

/// The kind `StartTerminalSessionRequest.kind` names for a tmux terminal.
pub const TMUX_KIND: &str = "tmux";

/// The kind `StartTerminalSessionRequest.kind` names for a tmux terminal on a new tmux session.
pub const TMUX_NEW_KIND: &str = "tmux-new";

impl TerminalKind {
    /// The kind a request names, with the tmux session it names for a [`TMUX_KIND`] terminal;
    /// surrounding whitespace is ignored. A tmux kind without a session is `INVALID_ARGUMENT`.
    pub fn from_request(kind: &str, tmux_session: &str) -> Result<TerminalKind, Status> {
        let kind = kind.trim();
        if kind == TMUX_KIND {
            let session = tmux_session.trim();
            if session.is_empty() {
                return Err(Status::invalid_argument(
                    "a tmux terminal names the tmux session it attaches to",
                ));
            }
            return Ok(TerminalKind::Tmux {
                session: session.to_string(),
            });
        }
        if kind == TMUX_NEW_KIND {
            // Only the empty name is refused here. The name is trimmed and validated by the host's
            // `tmux_session_name`, so a whitespace-only name is refused with that message.
            if tmux_session.is_empty() {
                return Err(Status::invalid_argument(
                    "a tmux terminal on a new session names the session it creates",
                ));
            }
            return Ok(TerminalKind::TmuxNew {
                session: tmux_session.to_string(),
            });
        }
        Ok(match kind {
            "" | "bash" => TerminalKind::Bash,
            agent => TerminalKind::Agent(agent.to_string()),
        })
    }

    /// The kind a request names wrapped in `wrappers` (outermost first): with no wrappers, what
    /// [`Self::from_request`] reads. A single [`TMUX_WRAPPER_KIND`] wrapper around an agent kind is
    /// a [`TerminalKind::TmuxWrappedAgent`]; a single [`SANDBOX_WRAPPER_KIND`] wrapper around the
    /// login shell is a [`TerminalKind::SandboxedShell`], and around an agent kind a
    /// [`TerminalKind::SandboxedAgent`]; a single [`TARGET_WRAPPER_KIND`] wrapper around the login
    /// shell is a [`TerminalKind::TargetShell`], and around an agent kind a
    /// [`TerminalKind::TargetAgent`]. Any other wrapper kind, a tmux wrapper around something that is
    /// not an agent, or a sandbox or target wrapper around something that is neither the login shell
    /// nor an agent, is `INVALID_ARGUMENT` naming what was refused.
    pub fn from_wrapped_request(
        kind: &str,
        tmux_session: &str,
        wrappers: &[crate::proto::terminal_session::LaunchNode],
    ) -> Result<TerminalKind, Status> {
        let Some((wrapper, inner)) = wrappers.split_first() else {
            return Self::from_request(kind, tmux_session);
        };
        if wrapper.kind == SANDBOX_WRAPPER_KIND {
            // An agent in a jail of its own is `sandbox-agent-tab`'s (`#session-liveness` 7/8).
            if let Ok(TerminalKind::Agent(agent)) = Self::from_request(kind, tmux_session) {
                return Self::sandboxed_agent(agent, inner);
            }
            return Self::sandboxed(kind, tmux_session, inner);
        }
        if wrapper.kind == TARGET_WRAPPER_KIND {
            // An agent on a target is `target-agent-tab`'s (`#session-liveness` 8/8); in tmux there,
            // `#target-tmux` 2/2's.
            if let Ok(TerminalKind::Agent(agent)) = Self::from_request(kind, tmux_session) {
                if let Some((_, rest)) = inner
                    .split_first()
                    .filter(|(tmux, _)| tmux.kind == TMUX_WRAPPER_KIND)
                {
                    return Self::on_target_in_tmux(agent, rest);
                }
                return Self::on_target_agent(agent, inner);
            }
            return Self::on_target(kind, tmux_session, inner);
        }
        if wrapper.kind != TMUX_WRAPPER_KIND {
            return Err(Status::invalid_argument(format!(
                "a terminal cannot be wrapped in {:?}: only {TMUX_WRAPPER_KIND:?} wraps a terminal",
                wrapper.kind
            )));
        }
        if let Some(refused) = inner.first() {
            return Err(Status::invalid_argument(format!(
                "a terminal cannot be wrapped in {:?} inside {TMUX_WRAPPER_KIND:?}: only one \
                 {TMUX_WRAPPER_KIND:?} wrapper is accepted",
                refused.kind
            )));
        }
        match Self::from_request(kind, tmux_session)? {
            TerminalKind::Agent(agent) => Ok(TerminalKind::TmuxWrappedAgent { agent }),
            _ => Err(Status::invalid_argument(format!(
                "only an agent can be wrapped in {TMUX_WRAPPER_KIND:?}, not {:?}",
                kind.trim()
            ))),
        }
    }
}

impl TerminalKind {
    /// What `kind` wrapped in a [`SANDBOX_WRAPPER_KIND`] wrapper (with `inner` wrappers inside it)
    /// runs: the login shell only.
    fn sandboxed(
        kind: &str,
        tmux_session: &str,
        inner: &[crate::proto::terminal_session::LaunchNode],
    ) -> Result<TerminalKind, Status> {
        if let Some(refused) = inner.first() {
            return Err(Status::invalid_argument(format!(
                "a terminal cannot be wrapped in {:?} inside {SANDBOX_WRAPPER_KIND:?}: a sandbox \
                 wraps the login shell alone",
                refused.kind
            )));
        }
        match Self::from_request(kind, tmux_session)? {
            TerminalKind::Bash => Ok(TerminalKind::SandboxedShell),
            _ => Err(Status::invalid_argument(format!(
                "only the login shell can be wrapped in {SANDBOX_WRAPPER_KIND:?} here, not {:?}",
                kind.trim()
            ))),
        }
    }

    /// What the agent kind `agent` wrapped in a [`SANDBOX_WRAPPER_KIND`] wrapper (with `inner`
    /// wrappers inside it) runs: the agent in a jail of its own. A wrapper inside the sandbox is
    /// `INVALID_ARGUMENT` naming it.
    fn sandboxed_agent(
        agent: String,
        inner: &[crate::proto::terminal_session::LaunchNode],
    ) -> Result<TerminalKind, Status> {
        if let Some(refused) = inner.first() {
            return Err(Status::invalid_argument(format!(
                "a terminal cannot be wrapped in {:?} inside {SANDBOX_WRAPPER_KIND:?}: a sandbox \
                 wraps an agent alone",
                refused.kind
            )));
        }
        Ok(TerminalKind::SandboxedAgent { agent })
    }

    /// What `kind` wrapped in a [`TARGET_WRAPPER_KIND`] wrapper (with `inner` wrappers inside it)
    /// runs: the login shell only; an agent on a target is [`Self::on_target_agent`]'s.
    fn on_target(
        kind: &str,
        tmux_session: &str,
        inner: &[crate::proto::terminal_session::LaunchNode],
    ) -> Result<TerminalKind, Status> {
        if let Some(refused) = inner.first() {
            return Err(Status::invalid_argument(format!(
                "a terminal cannot be wrapped in {:?} inside {TARGET_WRAPPER_KIND:?}: a target \
                 wraps the login shell or tmux alone",
                refused.kind
            )));
        }
        match Self::from_request(kind, tmux_session)? {
            TerminalKind::Bash => Ok(TerminalKind::TargetShell),
            TerminalKind::Tmux { session } => Ok(TerminalKind::TargetTmux { session }),
            TerminalKind::TmuxNew { session } => Ok(TerminalKind::TargetTmuxNew { session }),
            _ => Err(Status::invalid_argument(format!(
                "only the login shell or tmux can be wrapped in {TARGET_WRAPPER_KIND:?} here, not {:?}",
                kind.trim()
            ))),
        }
    }

    /// What the agent kind `agent` wrapped in `[target, tmux]` (with `inner` wrappers inside the
    /// tmux) runs: the agent in a tmux session of its own on the session's target
    /// (`#target-tmux` 2/2). A wrapper inside the tmux is `INVALID_ARGUMENT` naming it.
    fn on_target_in_tmux(
        agent: String,
        inner: &[crate::proto::terminal_session::LaunchNode],
    ) -> Result<TerminalKind, Status> {
        if let Some(refused) = inner.first() {
            return Err(Status::invalid_argument(format!(
                "a terminal cannot be wrapped in {:?} inside {TARGET_WRAPPER_KIND:?} and \
                 {TMUX_WRAPPER_KIND:?}: tmux on a target wraps an agent alone",
                refused.kind
            )));
        }
        Ok(TerminalKind::TargetTmuxAgent { agent })
    }

    /// What the agent kind `agent` wrapped in a [`TARGET_WRAPPER_KIND`] wrapper (with `inner`
    /// wrappers inside it) runs: the agent on the session's target. A wrapper inside the target is
    /// `INVALID_ARGUMENT` naming it.
    fn on_target_agent(
        agent: String,
        inner: &[crate::proto::terminal_session::LaunchNode],
    ) -> Result<TerminalKind, Status> {
        if let Some(refused) = inner.first() {
            return Err(Status::invalid_argument(format!(
                "a terminal cannot be wrapped in {:?} inside {TARGET_WRAPPER_KIND:?}: a target \
                 wraps an agent alone",
                refused.kind
            )));
        }
        Ok(TerminalKind::TargetAgent { agent })
    }
}

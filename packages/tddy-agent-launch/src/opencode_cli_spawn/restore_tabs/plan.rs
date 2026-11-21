//! The pure half of the roster restore: what a resume does with one roster entry
//! ([`restore_plan`]), over the entry and a [`LiveTabs`] snapshot.

use std::collections::BTreeSet;

use tddy_core::tab_roster::{
    LaunchNode, TabEntry, AGENT_KIND, CONVERSATION_ID_PROPERTY, HARNESS_PROPERTY, OS_USER_PROPERTY,
    SANDBOX_KIND, SESSION_PROPERTY, SHELL_KIND, TARGET_KIND, TMUX_ATTACH_KIND, TMUX_KIND,
};

/// An [`AGENT_KIND`](tddy_core::tab_roster::AGENT_KIND) node's resumability: `"false"` on a tab
/// whose harness cannot resume a given conversation (opencode and muse `--continue` the worktree's
/// last one; warp needs an exit-hook token a tab does not capture). Absent means resumable.
pub const RESUMABLE_PROPERTY: &str = "resumable";

/// What a resume does with one roster entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabRestore {
    /// The tab's terminal is already live: it is not restarted.
    LeaveLive,
    /// An agent tab started under its id, resuming its own conversation.
    ResumeAgent {
        harness: String,
        conversation_id: String,
    },
    /// An agent tab whose harness cannot resume a given conversation, started fresh under its id.
    FreshAgent { harness: String },
    /// A tmux-wrapped tab whose tmux session lives: a client re-attached under its id; the agent in
    /// the tmux session is not touched.
    ReattachTmux { session: String, os_user: String },
    /// A tmux-wrapped agent tab whose tmux session is gone: the agent relaunched in a new tmux
    /// session, resuming `resume_conversation` when it is resumable, fresh when it is `None`.
    RelaunchTmux {
        session: String,
        os_user: String,
        harness: String,
        resume_conversation: Option<String>,
    },
    /// The session user's login shell, reopened fresh under its id.
    FreshShell,
    /// The session user's login shell, reopened fresh under its id in a new jail of its own over
    /// the session's worktree and grants (`sandbox` → `shell`, `#session-liveness` 5/8).
    JailedShell,
    /// The target user's login shell, reopened fresh under its id on the session's target, in the
    /// session's worktree there (`target` → `shell`, `#session-liveness` 6/8).
    TargetShell,
    /// A tab attached to the user's own tmux session, which still exists: re-attached under its id.
    ReattachTmuxAttach { session: String, os_user: String },
    /// Not restorable: dropped from the roster and reported with `reason`.
    Drop { reason: String },
}

/// What is live when the roster is restored: the session's served terminals, and the tmux sessions
/// on each user's tmux server.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveTabs {
    /// The terminal ids the daemon serves for the session.
    pub terminals: BTreeSet<String>,
    /// `(os_user, tmux session name)` of every tmux session that exists.
    pub tmux_sessions: BTreeSet<(String, String)>,
}

impl LiveTabs {
    /// Nothing live.
    pub fn none() -> Self {
        Self::default()
    }

    /// With `terminal_id` served.
    pub fn with_terminal(mut self, terminal_id: &str) -> Self {
        self.terminals.insert(terminal_id.to_string());
        self
    }

    /// With the tmux session `session` existing on `os_user`'s tmux server.
    pub fn with_tmux_session(mut self, os_user: &str, session: &str) -> Self {
        self.tmux_sessions
            .insert((os_user.to_string(), session.to_string()));
        self
    }

    pub fn terminal_is_live(&self, terminal_id: &str) -> bool {
        self.terminals.contains(terminal_id)
    }

    pub fn tmux_session_exists(&self, os_user: &str, session: &str) -> bool {
        self.tmux_sessions
            .contains(&(os_user.to_string(), session.to_string()))
    }
}

/// What a resume does with `entry`, given what is `live`:
///
/// - a live terminal → [`TabRestore::LeaveLive`];
/// - `agent` → [`TabRestore::ResumeAgent`], or [`TabRestore::FreshAgent`] when marked
///   `resumable: false`;
/// - `tmux` → `agent` → [`TabRestore::ReattachTmux`] when the tmux session exists, otherwise
///   [`TabRestore::RelaunchTmux`];
/// - `shell` → [`TabRestore::FreshShell`];
/// - `sandbox` → `shell` → [`TabRestore::JailedShell`]; a `sandbox` around anything else is
///   [`TabRestore::Drop`] naming why;
/// - `target` → `shell` → [`TabRestore::TargetShell`]; a `target` around anything else is
///   [`TabRestore::Drop`] naming why;
/// - `tmux-attach` → [`TabRestore::ReattachTmuxAttach`] when the user's tmux session exists,
///   otherwise [`TabRestore::Drop`] naming it;
/// - an empty chain, an unknown kind, or a node missing what it needs → [`TabRestore::Drop`]
///   naming why.
pub fn restore_plan(entry: &TabEntry, live: &LiveTabs) -> TabRestore {
    if live.terminal_is_live(&entry.terminal_id) {
        return TabRestore::LeaveLive;
    }
    let Some(outermost) = entry.launch.first() else {
        return drop_because("the roster records no launch chain for it");
    };
    // A node this daemon does not know — a wrapper a later one added — anywhere in the chain: the
    // tab is not one this daemon can bring back as recorded.
    if let Some(unknown) = entry.launch.iter().find(|node| !is_known_kind(&node.kind)) {
        return drop_because(format!(
            "its launch kind {:?} is not one this daemon can restore",
            unknown.kind
        ));
    }
    match outermost.kind.as_str() {
        AGENT_KIND => agent_plan(outermost),
        SHELL_KIND => TabRestore::FreshShell,
        TMUX_ATTACH_KIND => tmux_attach_plan(outermost, live),
        TMUX_KIND => tmux_wrapped_plan(entry, outermost, live),
        SANDBOX_KIND => sandbox_plan(entry),
        TARGET_KIND => target_plan(entry),
        other => drop_because(format!(
            "its launch kind {other:?} is not one this daemon can restore"
        )),
    }
}

fn is_known_kind(kind: &str) -> bool {
    [
        AGENT_KIND,
        SHELL_KIND,
        TMUX_KIND,
        TMUX_ATTACH_KIND,
        SANDBOX_KIND,
        TARGET_KIND,
    ]
    .contains(&kind)
}

fn drop_because(reason: impl Into<String>) -> TabRestore {
    TabRestore::Drop {
        reason: reason.into(),
    }
}

/// The property `key` of `node`, or why the tab cannot be restored without it.
fn required<'a>(node: &'a LaunchNode, key: &str) -> Result<&'a str, TabRestore> {
    node.property(key)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| drop_because(format!("its {} launch node records no {key}", node.kind)))
}

/// Whether `agent` records a conversation its harness can resume: absent `resumable` means it can.
fn records_a_resumable_conversation(agent: &LaunchNode) -> bool {
    agent.property(RESUMABLE_PROPERTY) != Some("false")
}

/// An `agent` node's restore: resuming its conversation, or fresh when it is not resumable.
fn agent_plan(agent: &LaunchNode) -> TabRestore {
    let harness = match required(agent, HARNESS_PROPERTY) {
        Ok(harness) => harness.to_string(),
        Err(dropped) => return dropped,
    };
    if !records_a_resumable_conversation(agent) {
        return TabRestore::FreshAgent { harness };
    }
    match required(agent, CONVERSATION_ID_PROPERTY) {
        Ok(conversation_id) => TabRestore::ResumeAgent {
            harness,
            conversation_id: conversation_id.to_string(),
        },
        Err(dropped) => dropped,
    }
}

fn tmux_attach_plan(node: &LaunchNode, live: &LiveTabs) -> TabRestore {
    let (session, os_user) = match tmux_names(node) {
        Ok(names) => names,
        Err(dropped) => return dropped,
    };
    if live.tmux_session_exists(&os_user, &session) {
        TabRestore::ReattachTmuxAttach { session, os_user }
    } else {
        drop_because(format!(
            "the tmux session {session:?} of {os_user} no longer exists"
        ))
    }
}

/// A tmux-wrapped agent: re-attached while its tmux session lives, relaunched in it otherwise.
fn tmux_wrapped_plan(entry: &TabEntry, tmux: &LaunchNode, live: &LiveTabs) -> TabRestore {
    let (session, os_user) = match tmux_names(tmux) {
        Ok(names) => names,
        Err(dropped) => return dropped,
    };
    if live.tmux_session_exists(&os_user, &session) {
        return TabRestore::ReattachTmux { session, os_user };
    }
    let Some(agent) = entry.launch.get(1).filter(|node| node.kind == AGENT_KIND) else {
        return drop_because(
            "its tmux session is gone and the roster records no agent to relaunch",
        );
    };
    match agent_plan(agent) {
        TabRestore::ResumeAgent {
            harness,
            conversation_id,
        } => TabRestore::RelaunchTmux {
            session,
            os_user,
            harness,
            resume_conversation: Some(conversation_id),
        },
        TabRestore::FreshAgent { harness } => TabRestore::RelaunchTmux {
            session,
            os_user,
            harness,
            resume_conversation: None,
        },
        dropped => dropped,
    }
}

/// A `sandbox`-wrapped tab: a shell is reopened in a new jail; anything else in the jail is dropped
/// naming why.
fn sandbox_plan(entry: &TabEntry) -> TabRestore {
    match entry.launch.get(1) {
        None => drop_because("its sandbox launch node records nothing run inside the jail"),
        Some(inner) if inner.kind == SHELL_KIND => TabRestore::JailedShell,
        Some(inner) => drop_because(format!(
            "a {:?} run in a sandbox is not one this daemon can restore",
            inner.kind
        )),
    }
}

/// A `target`-wrapped tab: a shell is reopened on the session's target; anything else on a target is
/// dropped naming why (an agent on a target is `target-agent-tab`'s).
fn target_plan(entry: &TabEntry) -> TabRestore {
    // TODO(target-shell-tab): implement
    let _ = entry;
    todo!("the restore of a target-wrapped tab")
}

/// The tmux session name and OS user a `tmux` or `tmux-attach` node records.
pub(super) fn tmux_names(node: &LaunchNode) -> Result<(String, String), TabRestore> {
    let session = required(node, SESSION_PROPERTY)?.to_string();
    let os_user = required(node, OS_USER_PROPERTY)?.to_string();
    Ok((session, os_user))
}

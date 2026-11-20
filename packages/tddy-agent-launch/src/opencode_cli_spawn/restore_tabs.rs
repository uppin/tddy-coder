//! Restoring a session's **tab roster** on Resume (`#session-liveness` 4/4): after main's resume,
//! every other entry of `tabs.yaml` ([`tddy_core::tab_roster`]) is brought back under its recorded
//! terminal id, each as its launch chain allows.
//!
//! The rule per entry is [`restore_plan`] — pure, over the entry and a [`LiveTabs`] snapshot — and
//! [`restore_tab_roster`] carries it out, one [`RestoredTab`] per entry. A tab that cannot be restored
//! is dropped from the roster and reported; it never fails the resume.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-19-resume-restores-tab-roster.md

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use tddy_cli_sessions::cli_session_manager::{
    CliSessionManager, RecordedTerminal, MAIN_TERMINAL_ID,
};
use tddy_cli_sessions::tab_roster_store::TabRosterStore;
use tddy_cli_sessions::tmux::{
    ensure_reachable, kill_tmux_session, start_agent_tmux_session, tmux_session_alive,
};
use tddy_coding_agent_specs::slots::conversation::ConversationSlot;
use tddy_coding_agent_specs::{CodingAgentSpec, SpecError};
use tddy_core::tab_roster::{
    read_tab_roster, LaunchNode, TabEntry, AGENT_KIND, CONVERSATION_ID_PROPERTY, HARNESS_PROPERTY,
    OS_USER_PROPERTY, SESSION_PROPERTY, SHELL_KIND, TMUX_ATTACH_KIND, TMUX_KIND,
};
use tddy_daemon_kernel::privilege_drop::EffectiveIdentity;
use tddy_rpc::Status;
use tddy_service::proto::session::{RestoredTab, RestoredTabOutcome};

use crate::cli_agent_steps::conversation::{start_conversation, ConversationStart};
use crate::spec_launch::HostCodingAgents;

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
        other => drop_because(format!(
            "its launch kind {other:?} is not one this daemon can restore"
        )),
    }
}

fn is_known_kind(kind: &str) -> bool {
    [AGENT_KIND, SHELL_KIND, TMUX_KIND, TMUX_ATTACH_KIND].contains(&kind)
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

/// The tmux session name and OS user a `tmux` or `tmux-attach` node records.
fn tmux_names(node: &LaunchNode) -> Result<(String, String), TabRestore> {
    let session = required(node, SESSION_PROPERTY)?.to_string();
    let os_user = required(node, OS_USER_PROPERTY)?.to_string();
    Ok((session, os_user))
}

/// What [`restore_tab_roster`] restores, and with what.
pub struct RosterRestore<'a> {
    pub session_id: &'a str,
    /// The session directory holding `tabs.yaml`.
    pub session_dir: &'a Path,
    /// The worktree every restored tab runs in: main's.
    pub worktree: &'a Path,
    /// The session's OS user, as main was resumed as.
    pub os_user: &'a str,
    pub cli_manager: &'a Arc<CliSessionManager>,
    pub coding_agents: &'a HostCodingAgents,
    /// The roster's single writer: a dropped tab is forgotten through it.
    pub roster_store: &'a TabRosterStore,
}

/// Restore every entry of the session's roster other than main, in roster order, each under its
/// recorded terminal id as [`restore_plan`] decides, and report each. A cursor agent tab resumes the
/// chat it was minted; an agent tab started fresh is reported `FRESH` with the reason; a tab that
/// cannot be restored is forgotten from the roster and reported `DROPPED` — the resume itself goes
/// on.
pub async fn restore_tab_roster(restore: RosterRestore<'_>) -> Vec<RestoredTab> {
    let roster = match read_tab_roster(restore.session_dir) {
        Ok(roster) => roster,
        Err(error) => {
            log::warn!(
                "the tab roster of session {} is not restored: {error:#}",
                restore.session_id
            );
            return Vec::new();
        }
    };
    let tabs: Vec<TabEntry> = roster
        .tabs
        .into_iter()
        .filter(|tab| tab.terminal_id != MAIN_TERMINAL_ID)
        .collect();
    if tabs.is_empty() {
        return Vec::new();
    }
    let live = live_tabs(&restore, &tabs).await;
    let mut report = Vec::with_capacity(tabs.len());
    for tab in &tabs {
        report.push(restore_one(&restore, tab, restore_plan(tab, &live)).await);
    }
    report
}

/// What is live for the session now: its served terminals, and the tmux sessions its tabs name.
async fn live_tabs(restore: &RosterRestore<'_>, tabs: &[TabEntry]) -> LiveTabs {
    let mut live = LiveTabs::none();
    for handle in restore.cli_manager.list_terminals(restore.session_id).await {
        live = live.with_terminal(&handle.terminal_id);
    }
    let named: BTreeSet<(String, String)> = tabs
        .iter()
        .flat_map(|tab| tab.launch.iter())
        .filter(|node| node.kind == TMUX_KIND || node.kind == TMUX_ATTACH_KIND)
        .filter_map(|node| tmux_names(node).ok())
        .map(|(session, os_user)| (os_user, session))
        .collect();
    for (os_user, session) in named {
        let (user, name) = (os_user.clone(), session.clone());
        let alive = tokio::task::spawn_blocking(move || tmux_session_alive(&user, &name))
            .await
            .unwrap_or_else(|error| {
                log::warn!("the tmux liveness check of {session:?} did not finish: {error}");
                false
            });
        if alive {
            live = live.with_tmux_session(&os_user, &session);
        }
    }
    live
}

/// What carrying out one tab's restore ended as.
enum Restored {
    /// Back as it was (or never gone).
    AsRecorded,
    /// Back, but not as it was: started fresh, for this reason.
    Fresh(String),
}

/// Carry out `plan` for `tab` and report it. A tab that cannot be restored is forgotten from the
/// roster and reported dropped, with why: the resume goes on.
async fn restore_one(restore: &RosterRestore<'_>, tab: &TabEntry, plan: TabRestore) -> RestoredTab {
    let done = match plan {
        TabRestore::Drop { reason } => Err(reason),
        plan => carry_out(restore, tab, plan)
            .await
            .map_err(|error| format!("{error:#}")),
    };
    let (outcome, reason) = match done {
        Ok(Restored::AsRecorded) => (RestoredTabOutcome::Restored, String::new()),
        Ok(Restored::Fresh(reason)) => (RestoredTabOutcome::Fresh, reason),
        Err(reason) => {
            log::warn!(
                "tab {} of session {} is dropped from its roster: {reason}",
                tab.terminal_id,
                restore.session_id
            );
            if let Err(error) = restore
                .roster_store
                .forget(restore.session_dir, &tab.terminal_id)
                .await
            {
                log::error!(
                    "dropped tab {} of session {} is still in its roster: {error:#}",
                    tab.terminal_id,
                    restore.session_id
                );
            }
            (RestoredTabOutcome::Dropped, reason)
        }
    };
    RestoredTab {
        terminal_id: tab.terminal_id.clone(),
        outcome: outcome as i32,
        reason,
    }
}

/// Why a tab of `harness` reopened without its conversation.
fn reopened_fresh_reason(harness: &str) -> String {
    format!("{harness} cannot resume a given conversation, so the tab reopened fresh")
}

async fn carry_out(
    restore: &RosterRestore<'_>,
    tab: &TabEntry,
    plan: TabRestore,
) -> anyhow::Result<Restored> {
    let terminal_id = tab.terminal_id.as_str();
    let (session_id, worktree) = (restore.session_id, restore.worktree.to_path_buf());
    let manager = restore.cli_manager;
    match plan {
        TabRestore::LeaveLive => Ok(Restored::AsRecorded),
        TabRestore::ResumeAgent {
            harness,
            conversation_id,
        } => {
            let argv = tab_argv(restore, &harness, &conversation_id, true)?;
            let agent = RecordedTerminal::Agent {
                kind: harness,
                argv,
            };
            manager
                .start_terminal_under_id(session_id, terminal_id, worktree, agent)
                .await?;
            Ok(Restored::AsRecorded)
        }
        TabRestore::FreshAgent { harness } => {
            let argv = tab_argv(restore, &harness, &recorded_conversation(tab), false)?;
            let reason = reopened_fresh_reason(&harness);
            let agent = RecordedTerminal::Agent {
                kind: harness,
                argv,
            };
            manager
                .start_terminal_under_id(session_id, terminal_id, worktree, agent)
                .await?;
            Ok(Restored::Fresh(reason))
        }
        TabRestore::FreshShell => {
            let shell_path = tddy_terminal_rpc::login_shell_for(restore.os_user);
            let shell = RecordedTerminal::Shell { shell_path };
            manager
                .start_terminal_under_id(session_id, terminal_id, worktree, shell)
                .await?;
            Ok(Restored::AsRecorded)
        }
        TabRestore::ReattachTmux { session, os_user } => {
            let harness = recorded_harness(tab)?;
            manager
                .reattach_terminal(
                    session_id,
                    terminal_id,
                    &harness,
                    worktree,
                    &os_user,
                    &session,
                )
                .await?;
            Ok(Restored::AsRecorded)
        }
        TabRestore::ReattachTmuxAttach { session, os_user } => {
            let kind = tddy_terminal_rpc::service::TMUX_KIND;
            manager
                .reattach_terminal(session_id, terminal_id, kind, worktree, &os_user, &session)
                .await?;
            Ok(Restored::AsRecorded)
        }
        TabRestore::RelaunchTmux {
            session,
            os_user,
            harness,
            resume_conversation,
        } => {
            let conversation = resume_conversation
                .clone()
                .unwrap_or_else(|| recorded_conversation(tab));
            let argv = tab_argv(
                restore,
                &harness,
                &conversation,
                resume_conversation.is_some(),
            )?;
            relaunch_in_tmux(restore, tab, &harness, &os_user, &session, argv).await?;
            Ok(match resume_conversation {
                Some(_) => Restored::AsRecorded,
                None => Restored::Fresh(reopened_fresh_reason(&harness)),
            })
        }
        TabRestore::Drop { reason } => Err(anyhow::anyhow!(reason)),
    }
}

/// Start `argv` in the tmux session `session` of `os_user` again, and attach the tab to it. The agent
/// does not outlive an attach that failed: nothing would know to stop it.
async fn relaunch_in_tmux(
    restore: &RosterRestore<'_>,
    tab: &TabEntry,
    harness: &str,
    os_user: &str,
    session: &str,
    argv: Vec<String>,
) -> anyhow::Result<()> {
    ensure_reachable(os_user, &EffectiveIdentity::current())
        .map_err(|refusal| anyhow::anyhow!(refusal.message().to_string()))?;
    {
        let (user, name, worktree, session_dir) = (
            os_user.to_string(),
            session.to_string(),
            restore.worktree.to_path_buf(),
            restore.session_dir.to_path_buf(),
        );
        tokio::task::spawn_blocking(move || {
            start_agent_tmux_session(&user, &name, &worktree, &session_dir, &[], &argv, false)
        })
        .await
        .context("the tmux start did not finish")?
        .map_err(|error| anyhow::anyhow!("failed to relaunch {harness} in tmux: {error}"))?;
    }
    let attached = restore
        .cli_manager
        .attach_terminal_to_tmux(
            restore.session_id,
            &tab.terminal_id,
            harness,
            restore.worktree.to_path_buf(),
            os_user,
            session,
        )
        .await;
    if let Err(error) = attached {
        let (user, name) = (os_user.to_string(), session.to_string());
        if let Ok(Err(kill)) =
            tokio::task::spawn_blocking(move || kill_tmux_session(&user, &name)).await
        {
            log::warn!("could not end tmux session {session:?} of {os_user}: {kill}");
        }
        return Err(anyhow::anyhow!(
            "failed to attach to tmux session {session:?}: {error}"
        ));
    }
    Ok(())
}

/// The harness the tab's `agent` node records.
fn recorded_harness(tab: &TabEntry) -> anyhow::Result<String> {
    tab.launch
        .iter()
        .find(|node| node.kind == AGENT_KIND)
        .and_then(|node| node.property(HARNESS_PROPERTY))
        .map(str::to_string)
        .context("the roster records no harness for the tab")
}

/// The conversation the tab's `agent` node records; empty when it records none (a fresh start has
/// no use for one).
fn recorded_conversation(tab: &TabEntry) -> String {
    tab.launch
        .iter()
        .find(|node| node.kind == AGENT_KIND)
        .and_then(|node| node.property(CONVERSATION_ID_PROPERTY))
        .unwrap_or_default()
        .to_string()
}

/// The argv of an agent tab of session type `harness`, run as the binary this host resolves for it.
fn tab_argv(
    restore: &RosterRestore<'_>,
    harness: &str,
    conversation_id: &str,
    resume: bool,
) -> anyhow::Result<Vec<String>> {
    let spec = restore
        .coding_agents
        .agents()
        .for_session_type(harness)
        .with_context(|| format!("no coding agent runs session type {harness:?}"))?;
    let binary = restore.coding_agents.resolve(&spec.id, None)?.tool.binary;
    Ok(agent_tab_argv(spec, &binary, conversation_id, resume)?)
}

/// The argv of an agent tab of `spec` holding `conversation_id`, run as `binary`: `resume` set,
/// it continues that conversation (claude `--resume <id>`, cursor `--resume <chat id>`); unset, it
/// starts it (claude `--session-id <id>`).
pub fn agent_tab_argv(
    spec: &CodingAgentSpec,
    binary: &Path,
    conversation_id: &str,
    resume: bool,
) -> Result<Vec<String>, SpecError> {
    let flags: &[&str] = if resume { &["resume"] } else { &[] };
    spec.render_run_argv(
        binary,
        &[
            ("session_id", conversation_id),
            ("chat_id", conversation_id),
        ],
        flags,
    )
}

/// Whether a tab of `spec` can resume a given conversation: its conversation is the daemon's session
/// id or one the agent minted. A worktree's own (`--continue`) or a resume token read off an exit
/// is not the tab's to name.
///
/// TODO(opencode-cli): a tab resumes its worktree's last conversation, which is main's, not its own.
/// TODO(muse-cli): same as opencode: `--continue` is per worktree.
/// TODO(warp-cli): a tab does not capture the exit-hook token a resume needs.
pub fn tab_conversation_is_resumable(spec: &CodingAgentSpec) -> bool {
    matches!(
        spec.conversation(),
        ConversationSlot::DaemonSessionId { .. } | ConversationSlot::Minted { .. }
    )
}

/// The `agent` launch node of a tab of `harness` in `conversation_id`, marked `resumable: false`
/// when its harness cannot resume a given conversation.
pub fn agent_launch_node(harness: &str, conversation_id: &str, resumable: bool) -> LaunchNode {
    let mut node = LaunchNode::agent(harness, conversation_id);
    if !resumable {
        node.properties
            .insert(RESUMABLE_PROPERTY.to_string(), "false".to_string());
    }
    node
}

/// A new agent tab, ready to start: what it runs, the conversation it holds and whether a resume can
/// name that conversation.
pub struct NewAgentTab {
    pub argv: Vec<String>,
    pub conversation_id: String,
    pub resumable: bool,
}

/// Settle a new agent tab of session type `kind` in `worktree`: its conversation — the spec's own
/// rule, a fresh id for a `daemon_session_id` one, a chat the agent mints for a `minted` one — and
/// the argv that starts it. A tab is never the session's own conversation.
pub async fn new_agent_tab(
    coding_agents: &HostCodingAgents,
    kind: &str,
    worktree: &Path,
) -> Result<NewAgentTab, Status> {
    let spec = coding_agents
        .agents()
        .for_session_type(kind)
        .ok_or_else(|| Status::invalid_argument(format!("unknown terminal kind {kind:?}")))?;
    // The binary is the agent spec's, resolved on this host (`#agent-tools 19/19`): its
    // `binary.env`, then the host's search, refusing what is found nowhere.
    let agent = coding_agents
        .resolve(&spec.id, None)
        .map_err(|e| Status::failed_precondition(e.to_string()))?;
    let fresh_id = uuid::Uuid::now_v7().to_string();
    let conversation = start_conversation(ConversationStart {
        spec: &agent.spec,
        binary: &agent.tool.binary,
        worktree,
        env_base: &agent.env_base,
        env: &[],
        session_id: &fresh_id,
        target: None,
    })
    .await?;
    let conversation_id = conversation.usage_id().map_or(fresh_id, str::to_string);
    let argv = agent_tab_argv(&agent.spec, &agent.tool.binary, &conversation_id, false)
        .map_err(|e| Status::failed_precondition(e.to_string()))?;
    Ok(NewAgentTab {
        argv,
        conversation_id,
        resumable: tab_conversation_is_resumable(&agent.spec),
    })
}

#[cfg(test)]
#[path = "restore_tabs_tests.rs"]
mod tests;

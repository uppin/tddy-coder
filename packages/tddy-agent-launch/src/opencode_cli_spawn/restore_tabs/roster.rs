//! Carrying the restore out: [`restore_tab_roster`] reads the roster, takes a [`LiveTabs`] snapshot,
//! and carries out [`restore_plan`] for every entry other than main, one [`RestoredTab`] each.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use tddy_cli_sessions::cli_session_manager::{
    CliSessionManager, RecordedTerminal, MAIN_TERMINAL_ID,
};
use tddy_cli_sessions::tab_roster_store::TabRosterStore;
use tddy_cli_sessions::tmux::{
    ensure_reachable, kill_tmux_session_logged, start_agent_tmux_session, tmux_session_alive,
};
use tddy_core::tab_roster::{
    read_tab_roster, TabEntry, AGENT_KIND, CONVERSATION_ID_PROPERTY, HARNESS_PROPERTY,
    TMUX_ATTACH_KIND, TMUX_KIND,
};
use tddy_daemon_kernel::privilege_drop::EffectiveIdentity;
use tddy_daemon_sandbox::tab_jails::{TabJailSpawn, TabJails};
use tddy_service::proto::session::{RestoredTab, RestoredTabOutcome};

use super::new_tab::agent_tab_argv;
use super::plan::{restore_plan, tmux_names, LiveTabs, TabRestore};
use crate::spec_launch::HostCodingAgents;
use crate::tab_target_shell::TabTargetShell;
use wrapped_tabs::{
    restore_jailed_agent, restore_jailed_shell, restore_target_agent, restore_target_shell,
};

mod wrapped_tabs;

/// What [`restore_tab_roster`] restores, and with what.
pub struct RosterRestore<'a> {
    pub session_id: &'a str,
    /// The session directory holding `tabs.yaml`.
    pub session_dir: &'a Path,
    /// The worktree every restored tab runs in: main's.
    pub worktree: &'a Path,
    /// The session's OS user, as main was resumed as.
    pub os_user: &'a str,
    /// The GitHub user the resume's session token resolves to: whose vault a restored jailed tab's
    /// provider route is resolved from.
    pub subject: &'a str,
    pub cli_manager: &'a Arc<CliSessionManager>,
    pub coding_agents: &'a HostCodingAgents,
    /// The roster's single writer: a dropped tab is forgotten through it.
    pub roster_store: &'a TabRosterStore,
    /// The registry a `sandbox` → `shell` or `sandbox` → `agent` tab's new jail is registered in
    /// (`#session-liveness` 5/8, 7/8).
    pub tab_jails: &'a TabJails,
    /// What starts that jail.
    pub tab_jail_spawn: &'a dyn TabJailSpawn,
    /// What resolves the session's target a `target` → `shell` tab reopens on
    /// (`#session-liveness` 6/8).
    pub tab_target_shell: &'a dyn TabTargetShell,
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
    for terminal_id in restore.tab_jails.live(restore.session_id).await {
        live = live.with_terminal(&terminal_id);
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

/// A tab restored where and as it was recorded, once `started` succeeded.
fn recorded(started: anyhow::Result<()>) -> anyhow::Result<Restored> {
    started.map(|()| Restored::AsRecorded)
}

async fn carry_out(
    restore: &RosterRestore<'_>,
    tab: &TabEntry,
    plan: TabRestore,
) -> anyhow::Result<Restored> {
    match plan {
        TabRestore::LeaveLive => Ok(Restored::AsRecorded),
        TabRestore::ResumeAgent {
            harness,
            conversation_id,
        } => recorded(start_agent(restore, tab, harness, &conversation_id, true).await),
        TabRestore::FreshAgent { harness } => {
            let reason = reopened_fresh_reason(&harness);
            start_agent(restore, tab, harness, &recorded_conversation(tab), false).await?;
            Ok(Restored::Fresh(reason))
        }
        TabRestore::FreshShell => {
            let shell_path = tddy_terminal_rpc::login_shell_for(restore.os_user);
            recorded(start_under_id(restore, tab, RecordedTerminal::Shell { shell_path }).await)
        }
        TabRestore::JailedShell => recorded(restore_jailed_shell(restore, tab).await),
        TabRestore::JailedAgent {
            harness,
            resume_conversation,
        } => restore_jailed_agent(restore, tab, harness, resume_conversation).await,
        TabRestore::TargetShell => recorded(restore_target_shell(restore, tab).await),
        TabRestore::TargetAgent {
            harness,
            resume_conversation,
        } => restore_target_agent(restore, tab, harness, resume_conversation).await,
        TabRestore::ReattachTmux { session, os_user } => {
            let harness = recorded_harness(tab)?;
            reattach(restore, tab, &harness, &os_user, &session).await?;
            Ok(Restored::AsRecorded)
        }
        TabRestore::ReattachTmuxAttach { session, os_user } => {
            let kind = tddy_terminal_rpc::service::TMUX_KIND;
            reattach(restore, tab, kind, &os_user, &session).await?;
            Ok(Restored::AsRecorded)
        }
        TabRestore::RelaunchTmux {
            session,
            os_user,
            harness,
            resume_conversation,
        } => {
            relaunch_tmux_tab(
                restore,
                tab,
                &session,
                &os_user,
                harness,
                resume_conversation,
            )
            .await
        }
        TabRestore::Drop { reason } => Err(anyhow::anyhow!(reason)),
    }
}

/// Start `terminal` under the tab's recorded id.
async fn start_under_id(
    restore: &RosterRestore<'_>,
    tab: &TabEntry,
    terminal: RecordedTerminal,
) -> anyhow::Result<()> {
    restore
        .cli_manager
        .start_terminal_under_id(
            restore.session_id,
            &tab.terminal_id,
            restore.worktree.to_path_buf(),
            terminal,
        )
        .await?;
    Ok(())
}

/// Start the agent tab of `harness` under the tab's id, resuming `conversation_id` or starting it.
async fn start_agent(
    restore: &RosterRestore<'_>,
    tab: &TabEntry,
    harness: String,
    conversation_id: &str,
    resume: bool,
) -> anyhow::Result<()> {
    let argv = tab_argv(restore, &harness, conversation_id, resume)?;
    let agent = RecordedTerminal::Agent {
        kind: harness,
        argv,
    };
    start_under_id(restore, tab, agent).await
}

/// Attach a client to the live tmux session `session` of `os_user`, under the tab's id.
async fn reattach(
    restore: &RosterRestore<'_>,
    tab: &TabEntry,
    kind: &str,
    os_user: &str,
    session: &str,
) -> anyhow::Result<()> {
    restore
        .cli_manager
        .reattach_terminal(
            restore.session_id,
            &tab.terminal_id,
            kind,
            restore.worktree.to_path_buf(),
            os_user,
            session,
        )
        .await?;
    Ok(())
}

/// A tmux-wrapped agent tab whose tmux session is gone: the agent relaunched in a new one, resuming
/// `resume_conversation` when there is one, fresh otherwise.
async fn relaunch_tmux_tab(
    restore: &RosterRestore<'_>,
    tab: &TabEntry,
    session: &str,
    os_user: &str,
    harness: String,
    resume_conversation: Option<String>,
) -> anyhow::Result<Restored> {
    let conversation = resume_conversation
        .clone()
        .unwrap_or_else(|| recorded_conversation(tab));
    let argv = tab_argv(
        restore,
        &harness,
        &conversation,
        resume_conversation.is_some(),
    )?;
    relaunch_in_tmux(restore, tab, &harness, os_user, session, argv).await?;
    Ok(match resume_conversation {
        Some(_) => Restored::AsRecorded,
        None => Restored::Fresh(reopened_fresh_reason(&harness)),
    })
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
        kill_tmux_session_logged(os_user, session, "a restore that failed").await;
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

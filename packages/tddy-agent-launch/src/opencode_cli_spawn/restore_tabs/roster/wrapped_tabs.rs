//! The restores of a tab that is not run on this host as the session's user: reopened in a jail of
//! its own (`sandbox`, `#session-liveness` 5/8 and 7/8) or on the session's target (`target`,
//! 6/8 and 8/8). Each carries one [`TabRestore`](super::TabRestore) of that kind out under the tab's
//! recorded id.

use tddy_core::tab_roster::TabEntry;
use tddy_daemon_sandbox::tab_jails::{TabJailAgent, TabJailLaunch, TabJailOwner};

use super::super::new_target_tab::target_agent_tab;
use super::{recorded_conversation, reopened_fresh_reason, Restored, RosterRestore};
use crate::remote_agent_launch::{launch_agent_tab_on_target, launch_tab_on_target, TargetTab};

/// Start the session user's login shell under the tab's recorded id, in a new jail of its own over
/// the session's worktree and grants. A jail that cannot start fails the tab with the reason.
pub(super) async fn restore_jailed_shell(restore: &RosterRestore<'_>, tab: &TabEntry) -> anyhow::Result<()> {
    let login_shell = tddy_terminal_rpc::login_shell_for(restore.os_user);
    let launch = TabJailLaunch::for_tab(
        restore.session_id,
        &tab.terminal_id,
        restore.session_dir,
        restore.worktree,
        TabJailOwner {
            os_user: restore.os_user,
            login_shell: &login_shell,
            subject: restore.subject,
        },
    )
    .map_err(|refusal| anyhow::anyhow!(refusal.message().to_string()))?;
    restore
        .tab_jails
        .start(restore.tab_jail_spawn, launch)
        .await
        .map_err(|refusal| anyhow::anyhow!(refusal.message().to_string()))?;
    Ok(())
}

/// Start the agent of `harness` under the tab's recorded id in a new jail of its own over the
/// session's worktree and the agent's grants (`#session-liveness` 7/8), resuming
/// `resume_conversation` when there is one — [`Restored::AsRecorded`] — and fresh otherwise —
/// [`Restored::Fresh`] with the reason. A jail that cannot start fails the tab with the reason.
pub(super) async fn restore_jailed_agent(
    restore: &RosterRestore<'_>,
    tab: &TabEntry,
    harness: String,
    resume_conversation: Option<String>,
) -> anyhow::Result<Restored> {
    let login_shell = tddy_terminal_rpc::login_shell_for(restore.os_user);
    let launch = TabJailLaunch::for_tab(
        restore.session_id,
        &tab.terminal_id,
        restore.session_dir,
        restore.worktree,
        TabJailOwner {
            os_user: restore.os_user,
            login_shell: &login_shell,
            subject: restore.subject,
        },
    )
    .map_err(|refusal| anyhow::anyhow!(refusal.message().to_string()))?;
    let resumed = resume_conversation.is_some();
    let agent = TabJailAgent {
        conversation_id: resume_conversation.unwrap_or_else(|| recorded_conversation(tab)),
        resume: resumed,
        harness: harness.clone(),
    };
    restore
        .tab_jails
        .start_agent(restore.tab_jail_spawn, launch, agent)
        .await
        .map_err(|refusal| anyhow::anyhow!(refusal.message().to_string()))?;
    Ok(if resumed {
        Restored::AsRecorded
    } else {
        Restored::Fresh(reopened_fresh_reason(&harness))
    })
}

/// Start the target user's login shell under the tab's recorded id on the session's target, in the
/// session's worktree there ([`crate::remote_agent_launch::launch_tab_on_target`]). A target that
/// cannot be resolved (a locked vault) or reached fails the tab with the reason.
pub(super) async fn restore_target_shell(restore: &RosterRestore<'_>, tab: &TabEntry) -> anyhow::Result<()> {
    let target = restore
        .tab_target_shell
        .target_of(restore.session_dir, restore.subject)
        .map_err(|refusal| anyhow::anyhow!(refusal.message().to_string()))?;
    launch_tab_on_target(
        restore.cli_manager,
        restore.session_id,
        &tab.terminal_id,
        TargetTab {
            shell: target.shell,
            worktree: &target.worktree,
            session_dir: restore.session_dir,
        },
    )
    .await?;
    Ok(())
}

/// Start the agent of `harness` under the tab's recorded id on the session's target, in the
/// session's worktree there (`#session-liveness` 8/8): the target resolved through
/// `restore.tab_target_shell` as `restore.subject`, the agent run by its bare command
/// ([`super::super::target_agent_tab`]) through
/// [`crate::remote_agent_launch::launch_agent_tab_on_target`], resuming `resume_conversation` when
/// there is one — [`Restored::AsRecorded`] — and fresh otherwise — [`Restored::Fresh`] with the
/// reason. A target that cannot be resolved (a locked vault) or reached fails the tab with the
/// reason.
pub(super) async fn restore_target_agent(
    restore: &RosterRestore<'_>,
    tab: &TabEntry,
    harness: String,
    resume_conversation: Option<String>,
) -> anyhow::Result<Restored> {
    let target = restore
        .tab_target_shell
        .target_of(restore.session_dir, restore.subject)
        .map_err(|refusal| anyhow::anyhow!(refusal.message().to_string()))?;
    let resumed = resume_conversation.is_some();
    let conversation = resume_conversation.unwrap_or_else(|| recorded_conversation(tab));
    let agent = target_agent_tab(restore.coding_agents, &harness, &conversation, resumed)
        .map_err(|status| anyhow::anyhow!(status.message().to_string()))?;
    launch_agent_tab_on_target(
        restore.cli_manager,
        restore.session_id,
        &tab.terminal_id,
        TargetTab {
            shell: target.shell,
            worktree: &target.worktree,
            session_dir: restore.session_dir,
        },
        &agent,
    )
    .await?;
    Ok(if resumed {
        Restored::AsRecorded
    } else {
        Restored::Fresh(reopened_fresh_reason(&harness))
    })
}

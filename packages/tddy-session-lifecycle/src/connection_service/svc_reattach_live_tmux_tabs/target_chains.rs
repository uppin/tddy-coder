//! The target chains of the startup re-attach (`#target-reconnect` 3/3): a `[target, tmux]`
//! main and every `[target, tmux, agent]` tab, re-attached on their target through the
//! session's recorded `owner_subject`, with the hook-log follower restarted and an
//! unreachable target handed to a detach watcher.

use std::path::Path;
use std::sync::Arc;

use tddy_agent_launch::remote_agent_launch::{tmux_location_of, TargetTmux};
use tddy_cli_sessions::cli_session_manager::ReattachSkip;
use tddy_cli_sessions::tmux::agent_tmux_session_name;
use tddy_cli_sessions::tmux_location::SharedTmuxLocation;
use tddy_core::read_session_metadata;
use tddy_core::tab_roster::{
    TabEntry, AGENT_KIND, HARNESS_PROPERTY, OS_USER_PROPERTY, SESSION_PROPERTY,
};

use super::{Reattach, ReattachReport};
use crate::cli_session_manager::{ReattachOutcome, MAIN_TERMINAL_ID};

impl Reattach<'_> {
    /// The OS user the session's recorded owner subject maps to, when configured.
    fn owner_os_user(&self, session_dir: &Path) -> Option<String> {
        let owner = read_session_metadata(session_dir).ok()?.owner_subject?;
        self.host.config.users.os_user_for_github(&owner)
    }

    /// Hand `session_id`'s terminal to a detach watcher at `location`, holding it live in the
    /// watcher's unreachable state (D5): no eager re-dial of this pass's own.
    async fn watch_unreachable(
        &self,
        session_id: &str,
        terminal_id: &str,
        name: &str,
        location: SharedTmuxLocation,
        session_dir: &Path,
    ) {
        let os_user = self.owner_os_user(session_dir).unwrap_or_default();
        self.host
            .cli_manager
            .watch_detached(session_id, terminal_id, name, location, &os_user)
            .await;
    }

    /// The watcher location of a target session whose resolver refused: the adapter itself,
    /// which resolves its target per call, so a later unlock/reach is picked up by its probes.
    fn unreachable_location(&self, session_dir: &Path) -> Option<SharedTmuxLocation> {
        let owner = read_session_metadata(session_dir).ok()?.owner_subject?;
        let owner = owner.trim();
        if owner.is_empty() {
            return None;
        }
        Some(Arc::new(TargetTmux::new(
            session_dir.to_path_buf(),
            owner.to_string(),
            Arc::clone(&self.shells),
        )))
    }

    /// Restart `session_id`'s hook-log follower on its target: a restarted daemon's followers
    /// died with it, unlike a dropped client's, which keep reading. The follower reads on from
    /// the offset persisted in the session's directory, so no hook is replayed or lost.
    fn restart_hook_follower(&self, session_id: &str, session_dir: &Path) {
        let Ok(metadata) = read_session_metadata(session_dir) else {
            return;
        };
        let (Some(token), Some(owner)) = (
            metadata.hook_token.as_deref().filter(|t| !t.is_empty()),
            metadata
                .owner_subject
                .as_deref()
                .filter(|o| !o.trim().is_empty()),
        ) else {
            return;
        };
        let Ok(target) = self.shells.target_of(session_dir, owner) else {
            return;
        };
        let os_user = self
            .owner_os_user(session_dir)
            .unwrap_or_else(|| owner.to_string());
        let answer_wait_secs = self
            .host
            .launch_coding_agents()
            .agents()
            .for_session_type(metadata.session_type.as_deref().unwrap_or_default())
            .map(|spec| spec.answer_wait_secs())
            .unwrap_or(tddy_coding_agent_specs::DEFAULT_ANSWER_WAIT_SECS);
        let delivery = Arc::new(
            crate::connection_service::svc_activity_ports::hook_delivery::ActivityHookDelivery {
                host: self.host.clone(),
            },
        )
            as Arc<dyn tddy_agent_launch::remote_hook_follower::HookReportDelivery>;
        let sink = tddy_agent_launch::remote_hook_follower::ActivityHookSink::new(
            delivery, session_id, token, &os_user,
        )
        .answering_through(
            Arc::clone(&target.shell),
            &target.worktree,
            answer_wait_secs,
        );
        let log = tddy_core::remote_hook_log_in(&target.worktree);
        self.host
            .remote_hook_followers
            .abort_hook_follower(session_id);
        let follower = tddy_agent_launch::remote_hook_follower::RemoteHookFollower::start_resuming(
            Arc::clone(&target.shell),
            &log,
            token,
            Arc::new(sink),
            tddy_agent_launch::transcript_follower::hook_log_offset(session_dir),
        );
        self.host
            .remote_hook_followers
            .insert(session_id, follower, target.shell, target.worktree);
    }

    /// Leave a target chain's terminal with `skip`'s reason, watching it when unreachable.
    async fn leave_target(
        &self,
        session_id: &str,
        terminal_id: &str,
        name: &str,
        session_dir: &Path,
        skip: &ReattachSkip,
        report: &mut ReattachReport,
    ) {
        match skip {
            ReattachSkip::NoOwnerSubject => {
                report.leave(session_id, terminal_id, "records no owner subject")
            }
            ReattachSkip::TargetUnreachable(reason) => {
                report.leave(
                    session_id,
                    terminal_id,
                    &format!("its target could not be reached: {reason}"),
                );
                if let Some(location) = self.unreachable_location(session_dir) {
                    self.watch_unreachable(session_id, terminal_id, name, location, session_dir)
                        .await;
                }
            }
            other => report.leave(session_id, terminal_id, &other.to_string()),
        }
    }

    /// Re-attach `session_id`'s `[target, tmux]` main on its target, through the recorded
    /// owner subject: the single-flight main re-attach the lazy path shares.
    pub(super) async fn target_main(
        &self,
        session_id: &str,
        session_dir: &Path,
        recorded_name: Option<&str>,
        recorded_os_user: Option<&str>,
        report: &mut ReattachReport,
    ) {
        let name = recorded_name
            .map(str::to_string)
            .or_else(|| {
                read_session_metadata(session_dir)
                    .ok()
                    .and_then(|metadata| metadata.tmux_session)
            })
            .unwrap_or_else(|| agent_tmux_session_name(session_id));
        if let Err(skip) = tmux_location_of(session_dir, "", &self.shells) {
            return self
                .leave_target(
                    session_id,
                    MAIN_TERMINAL_ID,
                    &name,
                    session_dir,
                    &skip,
                    report,
                )
                .await;
        }
        let os_user = recorded_os_user
            .map(str::to_string)
            .or_else(|| self.owner_os_user(session_dir))
            .unwrap_or_default();
        match self.main.attach(&os_user, session_id).await {
            ReattachOutcome::Reattached => {
                report.reattach(session_id, MAIN_TERMINAL_ID, &name);
                self.restart_hook_follower(session_id, session_dir);
            }
            ReattachOutcome::AlreadyAttached => {}
            ReattachOutcome::Skipped(skip) => {
                self.leave_target(
                    session_id,
                    MAIN_TERMINAL_ID,
                    &name,
                    session_dir,
                    &skip,
                    report,
                )
                .await
            }
        }
    }

    /// Re-attach the `[target, tmux, agent]` tab `tab` on its target, through the recorded
    /// owner subject. A tmux session gone there is left for Resume, which recreates it.
    pub(super) async fn target_tab(
        &self,
        session_id: &str,
        tab: &TabEntry,
        tmux: &tddy_core::tab_roster::LaunchNode,
        session_dir: &Path,
        report: &mut ReattachReport,
    ) {
        let terminal_id = tab.terminal_id.as_str();
        let Some(name) = tmux.property(SESSION_PROPERTY) else {
            return report.leave(session_id, terminal_id, "its tmux node records no session");
        };
        let name = name.to_string();
        let Some(harness) = tab
            .launch
            .iter()
            .find(|node| node.kind == AGENT_KIND)
            .and_then(|node| node.property(HARNESS_PROPERTY))
            .map(str::to_string)
        else {
            return report.leave(session_id, terminal_id, "its launch chain records no agent");
        };
        if self
            .host
            .cli_manager
            .get_terminal(session_id, terminal_id)
            .await
            .is_some()
        {
            return;
        }
        let location = match tmux_location_of(session_dir, "", &self.shells) {
            Ok(location) => location,
            Err(skip) => {
                return self
                    .leave_target(session_id, terminal_id, &name, session_dir, &skip, report)
                    .await
            }
        };
        match location.session_lives(&name).await {
            Ok(true) => {}
            Ok(false) => return report.leave(session_id, terminal_id, "its tmux session is gone"),
            Err(error) => {
                report.leave(
                    session_id,
                    terminal_id,
                    &format!("its target could not be reached: {error}"),
                );
                self.watch_unreachable(
                    session_id,
                    terminal_id,
                    &name,
                    Arc::clone(&location),
                    session_dir,
                )
                .await;
                return;
            }
        }
        let os_user = tmux
            .property(OS_USER_PROPERTY)
            .map(str::to_string)
            .or_else(|| self.owner_os_user(session_dir))
            .unwrap_or_default();
        self.main
            .serve_agent_tool_socket_of(&os_user, session_id)
            .await;
        match self
            .host
            .cli_manager
            .reattach_terminal_at_location(
                session_id,
                terminal_id,
                &harness,
                session_dir.to_path_buf(),
                location,
                &name,
            )
            .await
        {
            Ok(_) => {
                report.reattach(session_id, terminal_id, &name);
                self.restart_hook_follower(session_id, session_dir);
            }
            Err(error) => report.leave(
                session_id,
                terminal_id,
                &format!("the attach failed: {error:#}"),
            ),
        }
    }
}

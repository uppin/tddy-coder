//! Ending every agent tab of a session — its main, each tab beside it, each open conversation —
//! and then checking that none survived (docs/ft/daemon/session-liveness.md § Closing a session).
//!
//! The close streams each as a TERMINATE step; `DeleteSession` ends the tabs and conversations the
//! same way, through [`SessionTabEnding`].

use std::time::Duration;

use async_trait::async_trait;
use tddy_agent_launch::launch_ports::SessionTabEnding;
use tddy_rpc::{Code, Status};

use super::super::svc_terminal_ports::session_tabs::{local_tmux_survives, SessionTab};
use super::super::DaemonSessionHost;
use super::CloseContext;
use tddy_core::SessionMetadata;
use tddy_service::proto::session::CloseSessionTab;
use tddy_session_agents::ClosedTurnLoop;

/// How long the tabs a close ended are given to be gone before one still there fails its step.
pub(super) const ENDED_WITHIN: Duration = Duration::from_secs(10);

/// How often the ended tabs are checked for being gone.
const ENDED_POLL: Duration = Duration::from_millis(200);

/// What a close ends, and checks is gone once ended.
pub(super) enum EndedTab {
    Main,
    Tab(SessionTab),
    /// An open conversation, closed out of the conversation table: a local one's turn loop, to
    /// check that its interrupted turn stopped; a remote one, cancelled on its daemon, leaves
    /// nothing here to check.
    Conversation(Option<ClosedTurnLoop>),
}

/// How the close of one conversation open on a session went: its id, and its turn loop when it ran
/// here — or why it could not be closed.
pub(super) type ClosedConversation = (String, Result<Option<ClosedTurnLoop>, String>);

impl DaemonSessionHost {
    /// End the session's main agent as `SignalSession` SIGTERM does — a tmux-run main by killing
    /// its tmux session (here or on its target) and then its agent, any other by signalling the
    /// agent this daemon holds. A main with no agent running has already ended.
    pub(super) async fn end_main(&self, ctx: &CloseContext) -> Result<(), String> {
        let ended = self
            .launch_sessions()
            .terminate_session_dir(
                &ctx.os_user,
                &ctx.session_dir,
                &ctx.session_id,
                tddy_service::proto::session::Signal::Sigterm as i32,
                &ctx.session_token,
            )
            .await;
        match ended {
            Ok(_) => Ok(()),
            // Only a main that is not tmux-run answers "no running agent" this way: a tmux kill's
            // refusals are never taken for an ended agent.
            Err(refusal)
                if refusal.code() == Code::FailedPrecondition
                    && ctx.metadata.tmux_session.is_none() =>
            {
                log::debug!(
                    "CloseSession: the main agent of {} has already ended ({})",
                    ctx.session_id,
                    refusal.message()
                );
                Ok(())
            }
            Err(refusal) => Err(refusal.message().to_string()),
        }
    }

    /// Close every conversation open on `session_id`: interrupt the local ones and forget what
    /// was planned for them, and cancel each remote one on the daemon that runs it, with the
    /// caller's token. Each conversation's id, and how its close went.
    pub(super) async fn end_conversations_of(
        &self,
        session_id: &str,
        session_token: &str,
    ) -> Vec<ClosedConversation> {
        let (closed, mut loops) = self
            .agent_conversations
            .close_every_conversation_of(session_id)
            .await;
        let roster = self.agent_roster();
        let mut ended = Vec::new();
        for conversation_id in closed.local {
            roster.conversation_closed(&conversation_id);
            let turn_loop = loops
                .iter()
                .position(|(id, _)| *id == conversation_id)
                .map(|at| loops.swap_remove(at).1);
            ended.push((conversation_id, Ok(turn_loop)));
        }
        for (daemon_instance_id, conversation_id) in closed.remote {
            let cancelled = roster
                .forward_cancel_agent_conversation(
                    session_token,
                    session_id,
                    &daemon_instance_id,
                    &conversation_id,
                    false,
                )
                .await
                .map(|_| None)
                .map_err(|refusal| {
                    format!(
                        "could not cancel it on daemon {daemon_instance_id}, where its turn loop \
                         may still run: {}",
                        refusal.message()
                    )
                });
            ended.push((conversation_id, cancelled));
        }
        ended
    }

    /// Stop what holds the session's worktree open once its tabs ended: its terminals' tasks, its
    /// sandbox, its jailed tabs and workspace jail, the room polling its checkout, and the follower
    /// of a target session's hook log.
    pub(super) async fn stop_what_holds_the_worktree(&self, session_id: &str) {
        self.cli_manager.stop_session(session_id).await;
        if let Some(sandbox) = self.sandbox_manager.get(session_id).await {
            sandbox.stop();
        }
        self.sandbox_manager.remove(session_id).await;
        self.tab_jails.stop_session(session_id).await;
        if let Some(jail) = self.workspace_sandboxes.remove(session_id).await {
            jail.stop();
        }
        tddy_session_activity::session_deletion::close_session_room(
            &*self.session_rooms,
            session_id,
        );
        self.remote_hook_followers
            .stop_and_remove_log(session_id)
            .await;
    }

    /// Why `ended` survived its end, or `None` once nothing of it is left.
    pub(super) async fn survives(&self, ctx: &CloseContext, ended: &EndedTab) -> Option<String> {
        match ended {
            EndedTab::Main => self.main_survives(ctx).await,
            EndedTab::Tab(tab) => {
                self.tab_survives(&ctx.session_id, tab, ctx.subject.as_deref())
                    .await
            }
            EndedTab::Conversation(Some(turn_loop)) if turn_loop.turn_still_runs() => Some(
                "its interrupted turn is still running in the conversation's worktree".to_string(),
            ),
            EndedTab::Conversation(_) => None,
        }
    }

    /// Why the session's main survived — its terminal, sandbox or workspace jail still held, or the
    /// tmux session it ran in still alive here or on its target — or `None`.
    async fn main_survives(&self, ctx: &CloseContext) -> Option<String> {
        let session_id = &ctx.session_id;
        if self.cli_manager.get(session_id).await.is_some() {
            return Some("its terminal is still live".to_string());
        }
        // `stop` reaps the jail's runner and answers nothing; a jail registered again since (a tool
        // call that re-provisioned it) is what can still be told.
        if self.workspace_sandboxes.get(session_id).await.is_some() {
            return Some("its workspace jail is running again".to_string());
        }
        if let Some(name) = &ctx.metadata.tmux_session {
            let survived = match &ctx.metadata.target {
                None => local_tmux_survives(&ctx.os_user, name).await,
                Some(_) => self.target_main_tmux_survives(ctx).await,
            };
            if survived.is_some() {
                return survived;
            }
        }
        self.cli_manager
            .live_agent_sessions()
            .await
            .union(self.sandbox_manager.live_sessions().await)
            .contains(session_id)
            .then(|| "the session still holds a live agent".to_string())
    }

    /// Why the tmux session a target session's main ran in survived on the target, or `None`.
    async fn target_main_tmux_survives(&self, ctx: &CloseContext) -> Option<String> {
        let name =
            tddy_agent_launch::remote_agent_launch::target_tmux_session_name(&ctx.session_id);
        let shell = match self.target_shell_of(ctx) {
            Ok(shell) => shell,
            Err(reason) => {
                return Some(format!(
                    "whether its tmux session {name:?} on the target ended cannot be told: {reason}"
                ))
            }
        };
        match tddy_agent_launch::remote_agent_launch::target_tmux_session_lives(
            shell.as_ref(),
            &name,
        )
        .await
        {
            Ok(false) => None,
            Ok(true) => Some(format!(
                "its tmux session {name:?} on the target is still alive"
            )),
            Err(error) => Some(format!(
                "whether its tmux session {name:?} on the target ended cannot be told: {error}"
            )),
        }
    }

    /// Wait, up to [`ENDED_WITHIN`], for every tab in `ended` to be gone; each one still there then,
    /// with why. `ended` holds only tabs whose end itself succeeded.
    pub(super) async fn survivors_of<'a, K>(
        &self,
        ctx: &CloseContext,
        ended: &'a [(K, EndedTab)],
    ) -> Vec<(&'a K, String)> {
        let deadline = tokio::time::Instant::now() + ENDED_WITHIN;
        loop {
            let mut survivors = Vec::new();
            for (key, tab) in ended {
                if let Some(reason) = self.survives(ctx, tab).await {
                    survivors.push((key, reason));
                }
            }
            if survivors.is_empty() || tokio::time::Instant::now() >= deadline {
                return survivors;
            }
            tokio::time::sleep(ENDED_POLL).await;
        }
    }

    /// The tabs a close of `ctx`'s session ends: its main while an agent of it is live or it runs
    /// in tmux (whose agent outlives this daemon), each tab beside it, and each open conversation —
    /// and, when its tab roster cannot be read, why (the tabs beside the main are then not listed).
    pub(super) async fn tabs_a_close_ends(
        &self,
        ctx: &CloseContext,
    ) -> (Vec<CloseSessionTab>, Option<String>) {
        let mut tabs = Vec::new();
        let live = self
            .launch_sessions()
            .live_sessions()
            .await
            .contains(&ctx.session_id);
        if live || ctx.metadata.tmux_session.is_some() {
            tabs.push(CloseSessionTab {
                tab_id: MAIN_TAB.to_string(),
                label: main_label(&ctx.metadata),
            });
        }
        let unreadable = match self.tabs_beside_main(&ctx.session_id).await {
            Ok(beside) => {
                tabs.extend(beside.into_iter().map(|tab| CloseSessionTab {
                    tab_id: tab.terminal_id,
                    label: tab.label,
                }));
                None
            }
            Err(reason) => Some(reason),
        };
        tabs.extend(
            self.agent_conversations
                .conversations_of(&ctx.session_id)
                .await
                .into_iter()
                .map(|(conversation_id, agent_id)| CloseSessionTab {
                    tab_id: conversation_tab_id(&conversation_id),
                    label: agent_id,
                }),
        );
        (tabs, unreadable)
    }
}

#[async_trait]
impl SessionTabEnding for DaemonSessionHost {
    async fn end_tabs_and_conversations(
        &self,
        session_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        let subject = (self.user_resolver)(session_token);
        let tabs = self
            .tabs_beside_main(session_id)
            .await
            .map_err(|reason| Status::internal(format!("{reason}; it was left in place")))?;
        // A closed session's close ended every tab and checked each gone — on its target too —
        // before it recorded the removal, so a target that can no longer be reached must not keep
        // the record from going (docs/ft/daemon/session-liveness.md § Closing a session).
        let closed = self.closed_session(session_id);
        let mut failures = Vec::new();
        for tab in tabs {
            let tab = if closed {
                tab.without_its_target_tmux()
            } else {
                tab
            };
            if let Err(error) = self
                .end_tab_with_its_session(session_id, &tab, subject.as_deref())
                .await
            {
                failures.push(format!("tab {}: {error}", tab.terminal_id));
            }
        }
        for (conversation_id, ended) in self.end_conversations_of(session_id, session_token).await {
            if let Err(error) = ended {
                failures.push(format!("conversation {conversation_id}: {error}"));
            }
        }
        if failures.is_empty() {
            return Ok(());
        }
        Err(Status::unavailable(format!(
            "not every tab of session {session_id} could be ended, so it was left in place: {}",
            failures.join("; ")
        )))
    }
}

impl DaemonSessionHost {
    /// Whether `session_id` was closed: its directory holds the `worktree-removed` marker.
    fn closed_session(&self, session_id: &str) -> bool {
        self.session_dir_of(session_id)
            .map(|dir| {
                tddy_worktree_service::worktree_removed_marker::worktree_removal_recorded(&dir)
            })
            .unwrap_or(false)
    }
}

/// The main tab's id, as the terminal surface names it.
pub(super) const MAIN_TAB: &str = tddy_cli_sessions::cli_session_manager::MAIN_TERMINAL_ID;

/// A conversation's tab id: `conversation:<id>`.
pub(super) fn conversation_tab_id(conversation_id: &str) -> String {
    format!("conversation:{conversation_id}")
}

/// The main tab's label: the session's agent, and the tmux session it runs in.
pub(super) fn main_label(metadata: &SessionMetadata) -> String {
    let agent = metadata
        .session_type
        .clone()
        .unwrap_or_else(|| "agent".to_string());
    match &metadata.tmux_session {
        Some(name) => format!("{agent} in tmux {name}"),
        None => agent,
    }
}

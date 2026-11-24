//! A close, run as a task of the daemon's registry (kind `close_session`): check, terminate every
//! tab, check again, remove every worktree, record the removal
//! (docs/ft/daemon/session-liveness.md § Closing a session).
//!
//! The task names itself first (`started`), streams each step as it runs and as it ends — and
//! writes it as a line to its `progress` channel — and ends with exactly one terminal event:
//! `needs_confirmation` (nothing more was done) or `outcome`.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use tddy_rpc::{Request, Status};
use tddy_service::proto::session::{
    close_session_event, CloseSessionEvent, CloseSessionOutcome, CloseSessionPreview,
    CloseSessionRequest, CloseSessionStarted, CloseSessionStep, CloseSessionStepKind,
    CloseSessionStepState,
};
use tddy_session_agents::peer_session_answer::peer_has_no_such_session;
use tddy_task::{TaskBody, TaskChannel, TaskContext, TaskStatus};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use super::super::DaemonSessionHost;
use super::close_plan::{
    conversation_tab_id, host_of_target, ClosePlan, PlannedWorktree, Removal, MAIN_TAB,
};
use super::close_tabs::EndedTab;
use super::closes_in_flight::CloseReservation;
use super::CloseContext;

/// The `kind` a close task is registered under.
pub(super) const CLOSE_TASK_KIND: &str = "close_session";

/// One close: what it acts on, what the caller confirmed, and where its events go.
pub(super) struct CloseTask {
    pub host: DaemonSessionHost,
    pub ctx: CloseContext,
    pub confirmed: bool,
    pub confirmed_fingerprint: String,
    pub events: UnboundedSender<Result<CloseSessionEvent, Status>>,
    pub progress: Arc<TaskChannel>,
    pub daemon_instance_id: String,
    /// Holds the session for this close until the task ends.
    pub reservation: CloseReservation,
}

/// How a close ended.
enum Ended {
    /// Something unconfirmed (or unreadable) was at risk: nothing more was done.
    NeedsConfirmation(CloseSessionPreview),
    /// The close ran to its end.
    Outcome(CloseSessionOutcome),
}

#[async_trait]
impl TaskBody for CloseTask {
    async fn run(self: Box<Self>, task: TaskContext) -> TaskStatus {
        let CloseTask {
            host,
            ctx,
            confirmed,
            confirmed_fingerprint,
            events,
            progress,
            daemon_instance_id,
            reservation,
        } = *self;
        let task_id = task.task_id().0;
        // Named before `started` goes out: a second close the caller asks for once it has read
        // `started` is refused naming this task.
        reservation.runs_as(&task_id);
        let mut run = CloseRun {
            host,
            ctx,
            confirmed,
            confirmed_fingerprint,
            events,
            progress,
            failed: Vec::new(),
        };
        run.send(close_session_event::Event::Started(CloseSessionStarted {
            task_id,
            daemon_instance_id,
        }));
        let ended = run.close(&task).await;
        let status = match &ended {
            Ended::NeedsConfirmation(_) => {
                run.note("needs confirmation: nothing was done");
                TaskStatus::Completed { exit_code: Some(0) }
            }
            Ended::Outcome(_) if task.is_cancelled() => TaskStatus::Cancelled,
            Ended::Outcome(outcome) if outcome.ok => TaskStatus::Completed { exit_code: Some(0) },
            Ended::Outcome(outcome) => TaskStatus::Failed {
                message: format!(
                    "{} step(s) of the close failed or were skipped",
                    outcome.failed_steps.len()
                ),
            },
        };
        // The session is free for the next close before the caller learns this one ended.
        drop(reservation);
        run.send(match ended {
            Ended::NeedsConfirmation(preview) => {
                close_session_event::Event::NeedsConfirmation(preview)
            }
            Ended::Outcome(outcome) => close_session_event::Event::Outcome(outcome),
        });
        status
    }
}

/// A close as it runs: the steps it streamed that failed or were skipped, in order.
struct CloseRun {
    host: DaemonSessionHost,
    ctx: CloseContext,
    confirmed: bool,
    confirmed_fingerprint: String,
    events: UnboundedSender<Result<CloseSessionEvent, Status>>,
    progress: Arc<TaskChannel>,
    failed: Vec<CloseSessionStep>,
}

impl CloseRun {
    async fn close(&mut self, task: &TaskContext) -> Ended {
        let plan = self.host.plan_close(&self.ctx).await;
        if let Some(preview) = self.unconfirmed(&plan) {
            return Ended::NeedsConfirmation(preview);
        }
        self.done(
            CloseSessionStepKind::Check,
            &self.ctx.session_id.clone(),
            "",
            "nothing unconfirmed would be lost",
        );
        if !self.terminate_every_tab().await {
            self.skip_removing(
                &plan,
                "a tab survived its termination, so no worktree is removed",
            );
            return self.outcome();
        }
        if task.is_cancelled() {
            self.skip_removing(&plan, "the close was cancelled");
            return self.outcome();
        }
        // An agent may have written between the check and its end.
        let plan = self.host.plan_close(&self.ctx).await;
        if let Some(preview) = self.unconfirmed(&plan) {
            return Ended::NeedsConfirmation(preview);
        }
        self.done(
            CloseSessionStepKind::Check,
            &self.ctx.session_id.clone(),
            "",
            "every tab ended, and nothing more would be lost than was confirmed",
        );
        let removed_every_worktree = self.remove_every_worktree(&plan).await;
        let removed_every_clone = self.tear_down_every_clone().await;
        if removed_every_worktree && removed_every_clone {
            self.record_the_removal();
        } else {
            self.skipped(
                CloseSessionStepKind::Record,
                &self.ctx.session_id.clone(),
                "",
                "not every worktree was removed",
            );
        }
        self.outcome()
    }

    /// The fresh preview to ask with, when `plan` is not what the caller confirmed: a worktree that
    /// could not be read, a loss nobody confirmed, or a confirmation of another preview.
    fn unconfirmed(&self, plan: &ClosePlan) -> Option<CloseSessionPreview> {
        let unconfirmed = plan.has_an_unreadable_worktree()
            || if self.confirmed {
                plan.fingerprint != self.confirmed_fingerprint
            } else {
                plan.loses_something()
            };
        unconfirmed.then(|| plan.preview())
    }

    /// End the session's main, each tab beside it and each open conversation, one step each, then
    /// stop what holds its worktree and check that none survived. Whether every one ended.
    async fn terminate_every_tab(&mut self) -> bool {
        let session_id = self.ctx.session_id.clone();
        let main_host = self
            .ctx
            .metadata
            .target
            .as_ref()
            .map(host_of_target)
            .unwrap_or_default();
        let mut ended: Vec<((String, String), EndedTab)> = Vec::new();
        let mut all_ended = true;

        self.running(CloseSessionStepKind::Terminate, MAIN_TAB, &main_host);
        match self.host.end_main(&self.ctx).await {
            Ok(()) => ended.push(((MAIN_TAB.to_string(), main_host), EndedTab::Main)),
            Err(error) => {
                all_ended = false;
                self.failed(
                    CloseSessionStepKind::Terminate,
                    MAIN_TAB,
                    &main_host,
                    &error,
                );
            }
        }
        for tab in self.host.tabs_beside_main(&session_id).await {
            self.running(CloseSessionStepKind::Terminate, &tab.terminal_id, "");
            match self
                .host
                .end_tab_with_its_session(&session_id, &tab, self.ctx.subject.as_deref())
                .await
            {
                Ok(()) => {
                    ended.push(((tab.terminal_id.clone(), String::new()), EndedTab::Tab(tab)))
                }
                Err(error) => {
                    all_ended = false;
                    self.failed(
                        CloseSessionStepKind::Terminate,
                        &tab.terminal_id,
                        "",
                        &error,
                    );
                }
            }
        }
        for (conversation_id, closed) in self
            .host
            .end_conversations_of(&session_id, &self.ctx.session_token)
            .await
        {
            let subject = conversation_tab_id(&conversation_id);
            match closed {
                Ok(()) => ended.push(((subject, String::new()), EndedTab::Conversation)),
                Err(error) => {
                    all_ended = false;
                    self.failed(CloseSessionStepKind::Terminate, &subject, "", &error);
                }
            }
        }

        self.host.stop_what_holds_the_worktree(&session_id).await;
        let survivors = self.host.survivors_of(&self.ctx, &ended).await;
        for ((subject, host), _) in &ended {
            match survivors.iter().find(|(key, _)| key.0 == *subject) {
                Some((_, reason)) => {
                    all_ended = false;
                    self.failed(
                        CloseSessionStepKind::Terminate,
                        subject,
                        host,
                        &format!("it survived its termination: {reason}"),
                    );
                }
                None => self.done(CloseSessionStepKind::Terminate, subject, host, "ended"),
            }
        }
        all_ended
    }

    /// Remove every worktree of `plan`, one step each, every one attempted. Whether all went.
    async fn remove_every_worktree(&mut self, plan: &ClosePlan) -> bool {
        let mut removed_all = true;
        for worktree in &plan.worktrees {
            if matches!(worktree.removal, Removal::WithAnother) {
                continue;
            }
            let (subject, host) = removal_subject(worktree);
            self.running(CloseSessionStepKind::Remove, &subject, &host);
            match self.remove(worktree).await {
                Ok(note) => self.done(CloseSessionStepKind::Remove, &subject, &host, &note),
                Err(error) => {
                    removed_all = false;
                    self.failed(CloseSessionStepKind::Remove, &subject, &host, &error);
                }
            }
        }
        removed_all
    }

    async fn remove(&self, worktree: &PlannedWorktree) -> Result<String, String> {
        match &worktree.removal {
            Removal::Local { path } => self.host.remove_local_worktree(&self.ctx, path).await,
            Removal::Target { worktree } => {
                let shell = self.host.target_shell_of(&self.ctx)?;
                tddy_worktree_service::target_worktrees::remove_target_worktree(
                    shell.as_ref(),
                    worktree,
                )
                .await
                .map(|()| "removed on the target".to_string())
                .map_err(|refusal| refusal.message().to_string())
            }
            Removal::Conversation(conversation) => conversation
                .clone()
                .remove()
                .await
                .map(|()| "removed with its branch and base ref".to_string())
                .map_err(|error| error.to_string()),
            Removal::ConversationWithoutBase {
                session_worktree,
                root,
            } => {
                let (repo, root) = (session_worktree.clone(), root.clone());
                tokio::task::spawn_blocking(move || {
                    tddy_worktree_service::worktrees::force_remove_worktree_under_repo(&repo, &root)
                })
                .await
                .map_err(|e| format!("the removal did not finish: {e}"))?
                .map(|()| "removed; it recorded no base ref".to_string())
                .map_err(|error| error.to_string())
            }
            Removal::Codebase {
                daemon_instance_id,
                session_id,
                fingerprint,
                here,
            } => {
                self.close_codebase(daemon_instance_id, session_id, fingerprint, *here)
                    .await
            }
            Removal::WithAnother => Ok(String::new()),
        }
    }

    /// Close the workspace session holding a split session's checkout on its codebase daemon,
    /// confirming the preview that daemon gave. A daemon that no longer has the session has no
    /// checkout left to remove.
    async fn close_codebase(
        &self,
        daemon_instance_id: &str,
        session_id: &str,
        fingerprint: &str,
        here: bool,
    ) -> Result<String, String> {
        let request = CloseSessionRequest {
            session_token: self.ctx.session_token.clone(),
            session_id: session_id.to_string(),
            confirmed: true,
            confirmed_fingerprint: fingerprint.to_string(),
        };
        let events = if here {
            Box::pin(self.host.close_session_at_host(Request::direct(request)))
                .await
                .map(|stream| CodebaseEvents::Here(stream.into_inner()))
        } else {
            self.host
                .forward_close(daemon_instance_id, &request)
                .await
                .map(CodebaseEvents::Peer)
        };
        let ended = match events {
            Ok(events) => events.terminal().await,
            Err(refusal) => Err(refusal),
        };
        match ended {
            Ok(close_session_event::Event::Outcome(outcome)) if outcome.ok => {
                Ok(format!("closed on daemon {daemon_instance_id}"))
            }
            Ok(close_session_event::Event::Outcome(outcome)) => Err(format!(
                "the close on daemon {daemon_instance_id} did not finish: {}",
                outcome
                    .failed_steps
                    .iter()
                    .map(|step| format!("{} — {}", step.subject, step.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            )),
            Ok(_) => Err(format!(
                "the checkout on daemon {daemon_instance_id} changed since it was checked; \
                 close the session again"
            )),
            Err(refusal) if peer_has_no_such_session(&refusal) => Ok(format!(
                "daemon {daemon_instance_id} no longer has workspace session {session_id}"
            )),
            Err(refusal) => Err(refusal.message().to_string()),
        }
    }

    /// Tear down every agent clone the session's roster created — from the clone store and the
    /// record — one step each. Whether all went.
    async fn tear_down_every_clone(&mut self) -> bool {
        let roster = self.host.agent_roster();
        let session_id = self.ctx.session_id.clone();
        let mut torn_down_all = true;
        for (daemon_instance_id, codebase_session_id) in roster.every_agent_clone_of(&session_id) {
            self.running(
                CloseSessionStepKind::Remove,
                &codebase_session_id,
                &daemon_instance_id,
            );
            match roster
                .tear_down_agent_clone(
                    &session_id,
                    &daemon_instance_id,
                    &codebase_session_id,
                    &self.ctx.session_token,
                )
                .await
            {
                Ok(()) => self.done(
                    CloseSessionStepKind::Remove,
                    &codebase_session_id,
                    &daemon_instance_id,
                    "agent clone torn down",
                ),
                Err(refusal) => {
                    torn_down_all = false;
                    self.failed(
                        CloseSessionStepKind::Remove,
                        &codebase_session_id,
                        &daemon_instance_id,
                        refusal.message(),
                    );
                }
            }
        }
        torn_down_all
    }

    /// Skip every removal `plan` names, every clone teardown and the record: `why`.
    fn skip_removing(&mut self, plan: &ClosePlan, why: &str) {
        for worktree in &plan.worktrees {
            if matches!(worktree.removal, Removal::WithAnother) {
                continue;
            }
            let (subject, host) = removal_subject(worktree);
            self.skipped(CloseSessionStepKind::Remove, &subject, &host, why);
        }
        let session_id = self.ctx.session_id.clone();
        for (daemon_instance_id, codebase_session_id) in
            self.host.agent_roster().every_agent_clone_of(&session_id)
        {
            self.skipped(
                CloseSessionStepKind::Remove,
                &codebase_session_id,
                &daemon_instance_id,
                why,
            );
        }
        self.skipped(CloseSessionStepKind::Record, &session_id, "", why);
    }

    /// Record in the session directory that its worktrees were removed.
    fn record_the_removal(&mut self) {
        let session_id = self.ctx.session_id.clone();
        match tddy_worktree_service::worktree_removed_marker::record_worktree_removed(
            &self.ctx.session_dir,
        ) {
            Ok(()) => self.done(
                CloseSessionStepKind::Record,
                &session_id,
                "",
                "the session keeps its record and branch, without a worktree",
            ),
            Err(error) => self.failed(
                CloseSessionStepKind::Record,
                &session_id,
                "",
                &format!("the removal could not be recorded: {error}"),
            ),
        }
    }

    fn outcome(&self) -> Ended {
        Ended::Outcome(CloseSessionOutcome {
            ok: self.failed.is_empty(),
            failed_steps: self.failed.clone(),
        })
    }

    fn running(&mut self, kind: CloseSessionStepKind, subject: &str, host: &str) {
        self.step(kind, subject, host, CloseSessionStepState::Running, "");
    }

    fn done(&mut self, kind: CloseSessionStepKind, subject: &str, host: &str, message: &str) {
        self.step(kind, subject, host, CloseSessionStepState::Done, message);
    }

    fn failed(&mut self, kind: CloseSessionStepKind, subject: &str, host: &str, message: &str) {
        self.step(kind, subject, host, CloseSessionStepState::Failed, message);
    }

    fn skipped(&mut self, kind: CloseSessionStepKind, subject: &str, host: &str, message: &str) {
        self.step(kind, subject, host, CloseSessionStepState::Skipped, message);
    }

    /// Stream one step, write it to the progress channel, and keep it when it failed or was skipped.
    fn step(
        &mut self,
        kind: CloseSessionStepKind,
        subject: &str,
        host: &str,
        state: CloseSessionStepState,
        message: &str,
    ) {
        let step = CloseSessionStep {
            kind: kind as i32,
            subject: subject.to_string(),
            state: state as i32,
            message: message.to_string(),
            host: host.to_string(),
        };
        self.note(&progress_line(&step));
        if matches!(
            state,
            CloseSessionStepState::Failed | CloseSessionStepState::Skipped
        ) {
            self.failed.push(step.clone());
        }
        self.send(close_session_event::Event::Step(step));
    }

    /// Write `line` to the task's progress channel.
    fn note(&self, line: &str) {
        self.progress.write(Bytes::from(format!("{line}\n")));
    }

    fn send(&self, event: close_session_event::Event) {
        // A caller that went away does not stop the close: it runs to its end, and its task
        // records how it went.
        let _ = self
            .events
            .send(Ok(CloseSessionEvent { event: Some(event) }));
    }
}

/// What a REMOVE step of `worktree` names: its path as recorded (the paired workspace session for
/// a codebase checkout that listed none), and where it is.
fn removal_subject(worktree: &PlannedWorktree) -> (String, String) {
    let subject = if worktree.view.path.is_empty() {
        worktree.view.owner_id.clone()
    } else {
        worktree.view.path.clone()
    };
    (subject, worktree.view.host.clone())
}

/// One step as a line of the task's progress output.
fn progress_line(step: &CloseSessionStep) -> String {
    let kind = match step.kind() {
        CloseSessionStepKind::Check => "check",
        CloseSessionStepKind::Terminate => "terminate",
        CloseSessionStepKind::Remove => "remove",
        CloseSessionStepKind::Record => "record",
        CloseSessionStepKind::Unspecified => "step",
    };
    let state = match step.state() {
        CloseSessionStepState::Running => "running",
        CloseSessionStepState::Done => "done",
        CloseSessionStepState::Failed => "failed",
        CloseSessionStepState::Skipped => "skipped",
        CloseSessionStepState::Unspecified => "unknown",
    };
    let host = if step.host.is_empty() {
        String::new()
    } else {
        format!(" on {}", step.host)
    };
    let message = if step.message.is_empty() {
        String::new()
    } else {
        format!(" — {}", step.message)
    };
    format!("{kind} {}{host}: {state}{message}", step.subject)
}

/// The events of a codebase daemon's close: this daemon's own stream, or the peer's relay.
enum CodebaseEvents {
    Here(crate::CloseSessionEventStream),
    Peer(UnboundedReceiver<Result<CloseSessionEvent, Status>>),
}

impl CodebaseEvents {
    /// Read to the close's terminal event. A stream that ends without one, or with an error, is
    /// the error.
    async fn terminal(self) -> Result<close_session_event::Event, Status> {
        let mut last = None;
        match self {
            CodebaseEvents::Here(mut stream) => {
                while let Some(item) = futures_util::StreamExt::next(&mut stream).await {
                    last = item?.event;
                }
            }
            CodebaseEvents::Peer(mut events) => {
                while let Some(item) = events.recv().await {
                    last = item?.event;
                }
            }
        }
        match last {
            Some(
                event @ (close_session_event::Event::Outcome(_)
                | close_session_event::Event::NeedsConfirmation(_)),
            ) => Ok(event),
            _ => Err(Status::internal(
                "the codebase daemon's close ended without saying how",
            )),
        }
    }
}

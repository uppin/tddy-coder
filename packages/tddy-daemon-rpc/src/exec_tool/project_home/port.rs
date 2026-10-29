use crate::exec_tool::project_home::registered_project;
use crate::exec_tool::project_home::status_error;
use crate::exec_tool::project_home::DaemonProjectHome;
use crate::exec_tool::project_home_session_follow;
use crate::exec_tool::project_home_start_options::{describe_start_options, StartOptionsSources};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tddy_agent_launch::svc_signal_delete_session::signal_session_dir;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Request;
use tddy_service::proto::session::{ListSessionsRequest, Signal};
use tddy_tool_engine::project_home_tools::{
    ListProjectSessionsResult, ListedSession, ProjectHomePort,
};
use tddy_tool_engine::project_home_worktree_tools::WorktreeEntry;
use tddy_tool_engine::session_follow::{SessionActivity, ToolCallBodies, TranscriptItem};
use tddy_tool_engine::start_ledger::{StartHandle, StartLedgerRecord};
use tddy_tool_engine::start_options::{StartOptions, StartOptionsQuery};
use tddy_tool_engine::start_session_args::{StartContext, StartSessionArgs};
use tddy_worktree_service::service::project_main_repo_on_host;
use tddy_worktree_service::session_git_ops::PushIdentity;
use tddy_worktree_service::session_target::{SessionTarget, TargetRecord};
use tddy_worktree_service::worktrees::{
    list_worktree_diff_rows, project_worktree_snapshot, worktree_path_is_listed,
};

#[async_trait]
impl ProjectHomePort for DaemonProjectHome {
    /// The host's choices for a start, answered from this host's own RPCs and sessions. A remote
    /// host's answer is the one it gives for itself: the tool forwards a call that names it.
    async fn start_options(
        &self,
        project_id: &str,
        query: StartOptionsQuery,
    ) -> anyhow::Result<StartOptions> {
        describe_start_options(
            StartOptionsSources {
                session_token: &self.session_token,
                daemon_instance_id: &self.daemon_instance_id,
                launch: &self.launch,
                catalog: &self.catalog,
                projects: &self.projects,
                host_service: &self.host_service,
                provider_accounts: self.provider_accounts.as_deref(),
            },
            project_id,
            query,
        )
        .await
    }

    async fn list_sessions(
        &self,
        project_id: &str,
        home_session_id: &str,
    ) -> anyhow::Result<Vec<ListedSession>> {
        let listed = self
            .launch
            .list_sessions_at_session_coordinate(Request::direct(ListSessionsRequest {
                session_token: self.session_token.clone(),
            }))
            .await
            .map_err(|status| anyhow::anyhow!(status.message().to_string()))?
            .into_inner();
        Ok(listed
            .sessions
            .into_iter()
            .filter(|s| s.project_id == project_id && s.session_id != home_session_id)
            .map(|s| ListedSession {
                agent_status: if s.activity_status.is_empty() {
                    s.status
                } else {
                    s.activity_status
                },
                session_id: s.session_id,
                branch: s.branch,
                worktree: s.repo_path,
                is_active: s.is_active,
                pending_elicitation: s.pending_elicitation,
                daemon_instance_id: self.daemon_instance_id.clone(),
            })
            .collect())
    }

    async fn list_peer_sessions(
        &self,
        _project_id: &str,
        _home_session_id: &str,
    ) -> ListProjectSessionsResult {
        match &self.peers {
            Some(peers) => peers.list().await,
            None => ListProjectSessionsResult {
                sessions: Vec::new(),
                unreachable_hosts: Vec::new(),
            },
        }
    }

    /// The start is begun on this daemon's streamed start path, which forwards it to the host the
    /// arguments name, and recorded in the start ledger of `home_session_id`, the orchestrator the
    /// session is linked to. Answers with the handle once the start has named its task.
    async fn start_session(
        &self,
        project_id: &str,
        home_session_id: &str,
        args: StartSessionArgs,
    ) -> anyhow::Result<StartHandle> {
        let request = args.into_request(StartContext {
            session_token: self.session_token.clone(),
            project_id: project_id.to_string(),
            orchestrator_session_id: home_session_id.to_string(),
            orchestrator_daemon_instance_id: self.daemon_instance_id.clone(),
        });
        let ledger = self.start_ledger_of(home_session_id)?;
        self.launch
            .begin_recorded_start(request, &self.os_user, ledger)
            .await
    }

    async fn get_session_start(
        &self,
        home_session_id: &str,
        start_id: &str,
    ) -> anyhow::Result<StartLedgerRecord> {
        self.start_ledger_of(home_session_id)?.read(start_id)
    }

    /// Stop the start on the host its task runs on: this host's task registry, or the peer's
    /// `CancelTask`. The record reads cancelled once the start's stream has ended.
    async fn cancel_session_start(
        &self,
        home_session_id: &str,
        start_id: &str,
    ) -> anyhow::Result<()> {
        let ledger = self.start_ledger_of(home_session_id)?;
        self.launch
            .cancel_recorded_start(&ledger, start_id, &self.session_token)
            .await
    }

    async fn push_identity(&self, target: &SessionTarget) -> anyhow::Result<PushIdentity> {
        let identity = self.identity.clone();
        let os_user = self.os_user.clone();
        let sessions_base = self.sessions_base.clone();
        let tddy_data_dir = self.tddy_data_dir.clone();
        let session_id = target.session_id.clone();
        let project_id = target.project_id.clone();
        let env = tokio::task::spawn_blocking(move || {
            let project = match registered_project(&tddy_data_dir, &os_user, &project_id) {
                Ok(project) => Some(project),
                Err(reason) => {
                    log::warn!(
                        "project-home git: target session {session_id} has no git identity: its project could not be read: {reason}"
                    );
                    None
                }
            };
            let session_dir = unified_session_dir_path(&sessions_base, &session_id);
            identity
                .session_identity_in(&session_id, &os_user, &session_dir, project.as_ref())
                .git_environment
        })
        .await
        .map_err(|e| anyhow::anyhow!("reading the target's git identity failed: {e}"))?;
        Ok(PushIdentity { env })
    }

    /// The worktrees of `project_id` on this host, from the snapshot the Worktrees screen's stream
    /// opens with, so the tool and the screen cannot disagree (`#start-parity` 3/9).
    async fn list_worktrees(&self, project_id: &str) -> anyhow::Result<Vec<WorktreeEntry>> {
        let main_repo = project_main_repo_on_host(
            &self.tddy_data_dir,
            &self.os_user,
            &self.daemon_instance_id,
            project_id,
        )
        .map_err(|status| anyhow::anyhow!(status.message().to_string()))?;
        let calculator = Arc::clone(&self.worktree_sizes);
        let project_id = project_id.to_string();
        let rows = tokio::task::spawn_blocking(move || {
            project_worktree_snapshot(&main_repo, &calculator, &project_id)
        })
        .await
        .map_err(|e| anyhow::anyhow!("reading the worktrees failed: {e}"))?;
        Ok(rows.into_iter().map(WorktreeEntry::from).collect())
    }

    async fn session_activity(&self, target: &SessionTarget) -> anyhow::Result<SessionActivity> {
        project_home_session_follow::session_activity(&self.sessions_base, target).await
    }

    async fn session_transcript(
        &self,
        target: &SessionTarget,
    ) -> anyhow::Result<Vec<TranscriptItem>> {
        project_home_session_follow::session_transcript(&self.sessions_base, target).await
    }

    async fn session_tool_call(
        &self,
        target: &SessionTarget,
        tool_call_id: &str,
    ) -> anyhow::Result<Option<ToolCallBodies>> {
        project_home_session_follow::session_tool_call(&self.sessions_base, target, tool_call_id)
            .await
    }

    /// SIGTERM to `target`'s process, delivered as `SignalSession` delivers it, on the sessions
    /// directory this port reads the target from (`#start-parity` 7/9). The record, worktree and
    /// branch are left as they are.
    async fn terminate_session(&self, target: &SessionTarget) -> anyhow::Result<()> {
        let session_dir = unified_session_dir_path(&self.sessions_base, &target.session_id);
        let session_id = target.session_id.clone();
        tokio::task::spawn_blocking(move || {
            signal_session_dir(&session_dir, &session_id, Signal::Sigterm as i32)
        })
        .await
        .map_err(|e| anyhow::anyhow!("terminating the session failed: {e}"))?
        .map_err(status_error)?;
        Ok(())
    }

    /// Enqueue the Worktrees screen's size walk: one worktree (membership-checked on git's own
    /// worktree list, as `CalculateWorktreeSize` checks it), or every worktree of the project
    /// (`#start-parity` 8/9). Answers once the walks are queued; the calculator reports the rest.
    async fn refresh_worktree_disk_usage(
        &self,
        project_id: &str,
        path: Option<&str>,
    ) -> anyhow::Result<tddy_tool_engine::project_home_disk_refresh::RefreshQueued> {
        let main_repo = project_main_repo_on_host(
            &self.tddy_data_dir,
            &self.os_user,
            &self.daemon_instance_id,
            project_id,
        )
        .map_err(|status| anyhow::anyhow!(status.message().to_string()))?;
        let wanted = path.map(PathBuf::from);
        let targets = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<PathBuf>> {
            match wanted {
                None => Ok(list_worktree_diff_rows(&main_repo)
                    .into_iter()
                    .map(|row| row.path)
                    .collect()),
                Some(path) if worktree_path_is_listed(&main_repo, &path) => Ok(vec![path]),
                Some(path) => anyhow::bail!(
                    "worktree path {} is not in git worktree list",
                    path.display()
                ),
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("reading the worktrees failed: {e}"))??;
        for target in &targets {
            self.worktree_sizes.enqueue(project_id, target).await;
        }
        Ok(tddy_tool_engine::project_home_disk_refresh::RefreshQueued {
            queued: targets.len(),
        })
    }

    async fn target_records(&self) -> anyhow::Result<Vec<TargetRecord>> {
        Ok(self
            .rows()
            .await?
            .into_iter()
            .map(|row| TargetRecord {
                worktree: Some(row.entry.repo_path)
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from),
                session_id: row.entry.session_id,
                project_id: row.entry.project_id,
                branch: row.branch,
                base_ref: row.base_ref,
                is_active: row.entry.is_active,
            })
            .collect())
    }
}

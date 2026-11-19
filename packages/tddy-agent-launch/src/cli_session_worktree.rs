//! The worktree and semantic index a daemon-hosted CLI harness session (`cursor-cli`,
//! `opencode-cli`) starts in: cut for the session from a registered project, or the client's own
//! checkout named by `repo_path`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_projects::project_storage::{self, ProjectData};
use tddy_rpc::Status;
use tddy_service::proto::session::start_phase::Step as StartStep;
use tddy_session_files::attachment_progress::AttachmentProgressSink;

use crate::hooks_and_urls::{effective_spawn_branch, spawned_branch_of_session};
use crate::stack_parent::SpawnStackParent;
use crate::worktree_source::{session_worktree_source, WorktreeSource};

/// What a start needs to provision its worktree.
pub(crate) struct CliWorktreeStart<'a> {
    /// The session type being started, named in refusals and logs.
    pub session_type: &'static str,
    /// Names the worktree cut in the error a failure of it reports.
    pub cut_label: &'static str,
    pub session_id: &'a str,
    pub session_dir: &'a Path,
    pub sessions_base: &'a Path,
    pub tddy_data_dir: &'a Path,
    pub os_user: &'a str,
    pub project_id: &'a str,
    /// Client-supplied local checkout; when non-empty it wins over `project_id`.
    pub repo_path: &'a str,
    pub branch_worktree_intent: &'a str,
    pub new_branch_name: &'a str,
    pub selected_integration_base_ref: &'a str,
    pub selected_branch_to_work_on: &'a str,
    pub create_remote_branch: bool,
    /// The intent the session's changeset recorded.
    pub intent: tddy_core::BranchWorktreeIntent,
    pub timeout: Duration,
    /// The ssh `Host` alias the checkout lives on (`StartSessionRequest.ssh_config_host`); empty cuts
    /// a worktree on this host. The remote checkout is cloned from the project's git url.
    pub ssh_alias: &'a str,
    /// The shell that reaches the session's target, when it named one: the checkout is cut on
    /// the target through it, whatever transport reaches it. Takes precedence over `ssh_alias`.
    pub shell: Option<std::sync::Arc<dyn tddy_remote_shell::RemoteShell>>,
}

/// The worktree a CLI harness session runs in, and the project it belongs to.
pub(crate) struct CliWorktree {
    pub path: PathBuf,
    /// `None` for a client-supplied checkout, which belongs to no project.
    pub project: Option<ProjectData>,
}

/// Cut the session's worktree from its registered project and link the branch it created to the
/// stack node it materializes, or adopt the client-supplied `repo_path` directly (not
/// daemon-managed; never removed on session end).
pub(crate) async fn provision_cli_worktree(
    start: &CliWorktreeStart<'_>,
    stack_parent: &SpawnStackParent<'_>,
    progress: &AttachmentProgressSink,
) -> Result<CliWorktree, Status> {
    let session_type = start.session_type;
    match session_worktree_source(start.repo_path, start.project_id) {
        WorktreeSource::Project(pid) => {
            if pid.is_empty() {
                return Err(Status::invalid_argument(format!(
                    "project_id is required for {session_type} sessions"
                )));
            }
            let (projects_dir, project) =
                tddy_session_split::service_util::find_registered_project(
                    start.tddy_data_dir,
                    start.os_user,
                    &pid,
                )?;
            let repo_root = tddy_session_split::service_util::project_repo_root(&project)?;
            let chain_base_ref = stack_parent
                .chain_base_ref(
                    &pid,
                    start.sessions_base,
                    &repo_root,
                    start.new_branch_name,
                    start.selected_integration_base_ref,
                )
                .await?;
            progress.begin_phase(StartStep::Worktree);
            let path = cut_worktree(
                start,
                &repo_root,
                &project.git_url,
                chain_base_ref,
                progress,
            )
            .await?;
            progress.end_phase(StartStep::Worktree);
            // The child's branch now exists, so the planned node it materializes can record it —
            // which is what makes that node's descendants spawnable at all, since they base onto
            // `<remote>/<branch>` (`Stack::base_ref_for_spawn`).
            //
            // Keyed on the branch the session's changeset records rather than on the requested name
            // — it may carry a collision suffix — and a failed link is logged, not raised (D36).
            let spawned_branch = spawned_branch(start, &pid, &projects_dir, &repo_root)?;
            stack_parent
                .link_spawned_branch_without_failing_the_spawn(
                    start.sessions_base,
                    &spawned_branch,
                    start.session_id,
                )
                .await;
            Ok(CliWorktree {
                path,
                project: Some(project),
            })
        }
        WorktreeSource::RepoPath(path) => {
            let canonical = std::fs::canonicalize(&path).map_err(|e| {
                Status::invalid_argument(format!(
                    "repo_path {} is not accessible: {e}",
                    path.display()
                ))
            })?;
            if !canonical.is_dir() {
                return Err(Status::invalid_argument(format!(
                    "repo_path {} is not a directory",
                    canonical.display()
                )));
            }
            log::info!(
                target: "tddy_daemon::cli_session_worktree",
                "{session_type} {}: using client-supplied repo_path {} directly as worktree (not daemon-managed; not removed on session end)",
                start.session_id,
                canonical.display()
            );
            Ok(CliWorktree {
                path: canonical,
                project: None,
            })
        }
    }
}

async fn cut_worktree(
    start: &CliWorktreeStart<'_>,
    repo_root: &Path,
    git_url: &str,
    chain_base_ref: Option<String>,
    progress: &AttachmentProgressSink,
) -> Result<PathBuf, Status> {
    let worktree_base_ref =
        tddy_core::select_worktree_base_ref(start.selected_integration_base_ref, chain_base_ref);
    let wt = if let Some(shell) = &start.shell {
        let recorded = recorded_by(shell.clone(), progress);
        remote_worktree_through(start, git_url, &recorded, worktree_base_ref, progress).await?
    } else if start.ssh_alias.is_empty() {
        tddy_session_split::service_util::create_session_worktree(
            start.timeout,
            start.cut_label,
            repo_root,
            start.session_dir,
            worktree_base_ref,
        )
        .await?
    } else {
        remote_worktree(start, git_url, worktree_base_ref, progress).await?
    };
    tddy_session_split::service_util::push_new_branch_to_origin_if_requested(
        start.create_remote_branch,
        start.intent,
        start.session_dir,
        &wt,
        start.timeout,
    )
    .await?;
    Ok(wt)
}

/// The session's checkout on the ssh target `start.ssh_alias`, cloned from `git_url`.
async fn remote_worktree(
    start: &CliWorktreeStart<'_>,
    git_url: &str,
    base_ref: Option<String>,
    progress: &AttachmentProgressSink,
) -> Result<PathBuf, Status> {
    let shell = recorded_by(
        std::sync::Arc::new(tddy_remote_shell::SshShell::new(start.ssh_alias)),
        progress,
    );
    remote_worktree_through(start, git_url, &shell, base_ref, progress).await
}

/// `shell`, reporting each command it runs to the start's progress, which is how the start's record
/// comes to list them.
///
/// The reports hold the progress stream only weakly: a recorded shell a session keeps past its
/// start (its pointer's removal on exit, its hook follower) neither keeps the start's stream open
/// nor reports anything once the start is over.
pub(crate) fn recorded_by(
    shell: std::sync::Arc<dyn tddy_remote_shell::RemoteShell>,
    progress: &AttachmentProgressSink,
) -> tddy_remote_shell::RecordedShell {
    tddy_remote_shell::RecordedShell::new(
        shell,
        std::sync::Arc::new(ProgressCalls(
            progress
                .tx
                .as_ref()
                .map(tokio::sync::mpsc::UnboundedSender::downgrade),
        )),
    )
}

/// A [`tddy_remote_shell::RemoteCallSink`] that turns a call's report into the start's
/// `remote_call` event, while the start's progress stream is still open.
struct ProgressCalls(
    Option<
        tokio::sync::mpsc::WeakUnboundedSender<
            Result<tddy_service::proto::session::StartSessionEvent, Status>,
        >,
    >,
);

impl tddy_remote_shell::RemoteCallSink for ProgressCalls {
    fn report(&self, call: tddy_remote_shell::RemoteCallReport) {
        use tddy_remote_shell::RemoteCallOutcome as Ended;
        use tddy_service::proto::session::start_remote_call::Outcome;
        let (outcome, exit_code) = match call.outcome {
            Ended::Exited { code } => (Outcome::Exited, code),
            Ended::TimedOut => (Outcome::TimedOut, 0),
            Ended::Unreachable => (Outcome::Unreachable, 0),
        };
        let Some(tx) = self.0.as_ref().and_then(|weak| weak.upgrade()) else {
            return;
        };
        AttachmentProgressSink::streaming(tx).remote_call(
            tddy_service::proto::session::StartRemoteCall {
                host: call.address.to_string(),
                command: call.command,
                duration_ms: u64::try_from(call.duration.as_millis()).unwrap_or(u64::MAX),
                outcome: outcome as i32,
                exit_code,
                stderr_tail: call.stderr_tail,
            },
        );
    }
}

/// The session's checkout on its target, cloned from `git_url` through `shell`: under the target
/// user's home, on the branch the start asked for — a new branch from its base, or the existing
/// branch it named — as the local cut honours both. A new branch is cut from `base_ref` when the
/// start or its stack parent named one, and otherwise from the clone's own default integration
/// base, resolved on the target after its fetch (`tddy_core::worktree::DEFAULT_BASE_REFS`) — never
/// from the clone's `HEAD`, which nothing moves after the clone.
async fn remote_worktree_through(
    start: &CliWorktreeStart<'_>,
    git_url: &str,
    shell: &dyn tddy_remote_shell::RemoteShell,
    base_ref: Option<String>,
    progress: &AttachmentProgressSink,
) -> Result<PathBuf, Status> {
    // The branch the start resolved, read from its changeset as the local cut reads it: an
    // omitted intent generated a name there, while the request fields this start carries may
    // name none. The requested field answers only when the changeset names none.
    let workflow = tddy_core::read_changeset(start.session_dir)
        .ok()
        .and_then(|cs| cs.workflow);
    let resolved_name = |from_changeset: Option<String>, requested: &str| {
        from_changeset
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| requested.trim().to_string())
    };
    let branch = match start.intent {
        tddy_core::BranchWorktreeIntent::WorkOnSelectedBranch => {
            let name = resolved_name(
                workflow.and_then(|wf| wf.selected_branch_to_work_on),
                start.selected_branch_to_work_on,
            );
            if name.is_empty() {
                return Err(Status::invalid_argument(
                    "selected_branch_to_work_on is required when branch_worktree_intent is work_on_selected_branch",
                ));
            }
            tddy_core::worktree::RemoteBranch::Existing { name }
        }
        tddy_core::BranchWorktreeIntent::NewBranchFromBase => {
            let name = resolved_name(
                workflow.and_then(|wf| wf.new_branch_name),
                start.new_branch_name,
            );
            if name.is_empty() {
                return Err(Status::invalid_argument(
                    "new_branch_name is required when branch_worktree_intent is new_branch_from_base",
                ));
            }
            tddy_core::worktree::RemoteBranch::NewFrom {
                name,
                base_ref: base_ref.filter(|base| !base.trim().is_empty()),
            }
        }
    };
    let request = tddy_core::worktree::RemoteWorktreeRequest {
        git_url: git_url.to_string(),
        session_id: start.session_id.to_string(),
        branch,
    };
    let checkout = tokio::time::timeout(
        start.timeout,
        tddy_core::setup_remote_worktree(shell, &request),
    )
    .await
    .map_err(|_| {
        Status::deadline_exceeded(format!(
            "remote worktree setup on {} did not finish within {:?}",
            shell.address(),
            start.timeout
        ))
    })?
    .map_err(|error| {
        // The clone's error names the project's URL, which may carry a credential.
        Status::internal(format!(
            "remote worktree setup failed: {}",
            tddy_remote_shell::scrub_credentials(&error.to_string())
        ))
    })?;
    progress.worktree_ready(&checkout.branch, &checkout.worktree);
    Ok(PathBuf::from(checkout.worktree))
}

fn spawned_branch(
    start: &CliWorktreeStart<'_>,
    project_id: &str,
    projects_dir: &Path,
    repo_root: &Path,
) -> Result<String, Status> {
    let remote =
        project_storage::effective_remote_name_for_project(projects_dir, project_id, repo_root)
            .map_err(|e| Status::internal(e.to_string()))?;
    Ok(spawned_branch_of_session(
        start.session_dir,
        effective_spawn_branch(
            start.branch_worktree_intent,
            start.new_branch_name,
            start.selected_branch_to_work_on,
            &remote,
        ),
    ))
}

/// Index the worktree into the session dir before launch (blocking; a missing embedder or a failed
/// index aborts the start — no unindexed fallback), and name the per-session index DB in the env the
/// agent is launched with. Empty when `semantic_index` is off.
pub(crate) async fn semantic_index_env(
    tddy_data_dir: &Path,
    session_id: &str,
    semantic_index: bool,
    task_registry: &tddy_task::TaskRegistry,
    session_dir: &Path,
    worktree_path: &Path,
    progress: &AttachmentProgressSink,
) -> Result<Vec<(String, String)>, Status> {
    let mut session_env: Vec<(String, String)> = Vec::new();
    if semantic_index {
        progress.begin_phase(StartStep::SemanticIndex);
        tddy_session_split::service_util::index_session_worktree(
            tddy_data_dir,
            task_registry,
            session_id,
            worktree_path,
            session_dir,
        )
        .await?;
        progress.end_phase(StartStep::SemanticIndex);
        session_env.push(tddy_semantic_index::semantic_index::semantic_index_env(
            session_dir,
        ));
    }
    Ok(session_env)
}

#[cfg(test)]
mod remote_cut_tests {
    //! `#remote-shell` 10/11: a target session's checkout is cut under the target user's home,
    //! on the branch the start asked for.

    use std::sync::Arc;
    use std::time::Duration;

    use tddy_remote_shell::{ExecOutput, InMemoryShell};

    use super::{cut_worktree, remote_worktree_through, CliWorktreeStart};

    const QAPE_HQ: &str = "git@github.com:wix-private/qape-hq.git";
    const SESSION: &str = "01a1b2c3-remote-dev";
    const WORKSPACE_HOME: &str = "/home/coder";

    /// A workspace whose user's home is /home/coder, whose clone of the project holds
    /// `origin/master` once fetched, and whose git commands all succeed.
    /// The home probe is answered as a pty-merging transport delivers it: the shell's
    /// start marker, then the probe's sentinels around the home (they spell exactly what
    /// `remote_checkout_root` prints around `$HOME`). The cut's base probe is answered with
    /// `origin/master` between the sentinels `tddy-git`'s `remote_checkout` prints around it.
    fn a_workspace() -> InMemoryShell {
        let shell = InMemoryShell::new("coder.remote-dev");
        shell.answers_when(|request| {
            request.command.contains("TDDY_REMOTE_BASE_BEGIN").then(|| {
                ExecOutput::succeeded(
                    b"TDDY_REMOTE_BASE_BEGIN:origin/master:TDDY_REMOTE_BASE_END\n".to_vec(),
                )
            })
        });
        shell.answers_when(|request| {
            request.command.contains("$HOME").then(|| {
                ExecOutput::succeeded(
                    format!(
                        "tddy-coder-shell: the command is running\r\n\
                         TDDY_REMOTE_HOME_BEGIN:{WORKSPACE_HOME}:TDDY_REMOTE_HOME_END\r\n"
                    )
                    .into_bytes(),
                )
            })
        });
        shell.answers_when(|request| {
            request
                .command
                .contains("git ")
                .then(|| ExecOutput::succeeded(Vec::new()))
        });
        shell
    }

    #[tokio::test]
    async fn a_target_checkout_is_cut_under_the_targets_home_from_the_requested_branch_and_base() {
        // Given a start on a Coder target asking for feature/hooks off origin/main
        let session_dir = tempfile::tempdir().expect("session dir");
        let sessions_base = tempfile::tempdir().expect("sessions base");
        let tddy_data = tempfile::tempdir().expect("tddy data");
        let workspace = a_workspace();
        let start = CliWorktreeStart {
            session_type: "claude-cli",
            cut_label: "cut the session worktree",
            session_id: SESSION,
            session_dir: session_dir.path(),
            sessions_base: sessions_base.path(),
            tddy_data_dir: tddy_data.path(),
            os_user: "testdev",
            project_id: "proj-1",
            repo_path: "",
            branch_worktree_intent: "new_branch_from_base",
            new_branch_name: "feature/hooks",
            selected_integration_base_ref: "origin/main",
            selected_branch_to_work_on: "",
            create_remote_branch: false,
            intent: tddy_core::BranchWorktreeIntent::NewBranchFromBase,
            timeout: Duration::from_secs(5),
            ssh_alias: "",
            shell: Some(Arc::new(workspace.clone())),
        };

        // When the checkout is cut on the target
        let worktree = remote_worktree_through(
            &start,
            QAPE_HQ,
            &workspace,
            Some("origin/main".to_string()),
            &tddy_session_files::attachment_progress::AttachmentProgressSink::discarding(),
        )
        .await
        .expect("the cut succeeds");

        // Then it is under the workspace user's home, on the requested branch from its base
        let root = format!("{WORKSPACE_HOME}/.tddy/repos/qape-hq");
        assert_eq!(
            worktree.to_string_lossy(),
            format!("{root}/.worktrees/{SESSION}")
        );
        let run = workspace.commands_run().join("\n");
        assert!(
            run.contains("worktree add -b 'feature/hooks'")
                && run.contains("'origin/main'")
                && !run.contains("/home/dev")
                && !run.contains("tddy-"),
            "the requested branch, from its base, under the target's home:\n{run}"
        );
    }

    #[tokio::test]
    async fn a_start_that_named_no_branch_is_cut_on_the_branch_its_changeset_generated() {
        // Given a start that named no branch, whose changeset generated one for its session
        let session_dir = tempfile::tempdir().expect("session dir");
        let sessions_base = tempfile::tempdir().expect("sessions base");
        let tddy_data = tempfile::tempdir().expect("tddy data");
        tddy_core::write_changeset(
            session_dir.path(),
            &tddy_core::Changeset {
                workflow: Some(tddy_core::ChangesetWorkflow {
                    branch_worktree_intent: Some(
                        tddy_core::BranchWorktreeIntent::NewBranchFromBase,
                    ),
                    new_branch_name: Some("claude-cli/01a1b2c3".to_string()),
                    ..tddy_core::ChangesetWorkflow::default()
                }),
                ..tddy_core::Changeset::default()
            },
        )
        .expect("the changeset the start resolved is written");
        let workspace = a_workspace();
        let start = CliWorktreeStart {
            session_type: "claude-cli",
            cut_label: "cut the session worktree",
            session_id: SESSION,
            session_dir: session_dir.path(),
            sessions_base: sessions_base.path(),
            tddy_data_dir: tddy_data.path(),
            os_user: "testdev",
            project_id: "proj-1",
            repo_path: "",
            branch_worktree_intent: "",
            new_branch_name: "",
            selected_integration_base_ref: "",
            selected_branch_to_work_on: "",
            create_remote_branch: false,
            intent: tddy_core::BranchWorktreeIntent::NewBranchFromBase,
            timeout: Duration::from_secs(5),
            ssh_alias: "",
            shell: Some(Arc::new(workspace.clone())),
        };

        // When the checkout is cut on the target
        remote_worktree_through(
            &start,
            QAPE_HQ,
            &workspace,
            Some("origin/master".to_string()),
            &tddy_session_files::attachment_progress::AttachmentProgressSink::discarding(),
        )
        .await
        .expect("the cut succeeds");

        // Then the branch cut is the generated one the changeset names, never an empty name
        let run = workspace.commands_run().join("\n");
        assert!(
            run.contains("worktree add -b 'claude-cli/01a1b2c3'"),
            "the generated branch from the changeset is cut:\n{run}"
        );
    }
    /// The base the target's `git worktree add -b` was given: its last argument, unquoted.
    fn the_base_the_target_cut_from(workspace: &InMemoryShell) -> Option<String> {
        workspace
            .commands_run()
            .iter()
            .find(|command| command.contains("worktree add -b"))
            .and_then(|command| command.rsplit(' ').next().map(str::to_string))
            .map(|base| base.trim_matches('\'').to_string())
    }

    /// A `new_branch_from_base` start on a target that names no base and has no stack parent is
    /// cut from the literal `HEAD` of the target's clone: the clone's local `master`, which the
    /// cut's `git fetch --all` never moves. A long-lived clone (a Coder workspace's
    /// `~/.tddy/repos/qape-hq`) then branches every new session from a stale commit, where a cut
    /// on this host branches from the freshly fetched `origin/master`.
    #[tokio::test]
    async fn a_target_start_naming_no_base_is_cut_from_the_projects_fetched_main_branch() {
        // Given a workspace whose clone holds origin/master, and a start naming no base
        let session_dir = tempfile::tempdir().expect("session dir");
        let sessions_base = tempfile::tempdir().expect("sessions base");
        let tddy_data = tempfile::tempdir().expect("tddy data");
        let project_checkout = tempfile::tempdir().expect("this host's checkout");
        let workspace = a_workspace();
        let start = CliWorktreeStart {
            session_type: "claude-cli",
            cut_label: "cut the session worktree",
            session_id: SESSION,
            session_dir: session_dir.path(),
            sessions_base: sessions_base.path(),
            tddy_data_dir: tddy_data.path(),
            os_user: "testdev",
            project_id: "proj-1",
            repo_path: "",
            branch_worktree_intent: "new_branch_from_base",
            new_branch_name: "test-coder-remote-dev",
            selected_integration_base_ref: "",
            selected_branch_to_work_on: "",
            create_remote_branch: false,
            intent: tddy_core::BranchWorktreeIntent::NewBranchFromBase,
            timeout: Duration::from_secs(30),
            ssh_alias: "",
            shell: Some(Arc::new(workspace.clone())),
        };

        // When the session's worktree is cut, with no stack parent to base it on
        cut_worktree(
            &start,
            project_checkout.path(),
            QAPE_HQ,
            None,
            &tddy_session_files::attachment_progress::AttachmentProgressSink::discarding(),
        )
        .await
        .expect("the cut succeeds");

        // Then the target branched from its clone's fetched main branch, not the clone's HEAD
        assert_eq!(
            the_base_the_target_cut_from(&workspace),
            Some("origin/master".to_string())
        );
    }
}

#[cfg(test)]
mod provision_tests {
    //! Where a CLI harness session's worktree comes from: the client's own checkout, used as it
    //! is, or a registered project — and the starts that name neither usably.

    use std::path::Path;
    use std::time::Duration;

    use tddy_session_files::attachment_progress::AttachmentProgressSink;

    use super::{provision_cli_worktree, CliWorktree, CliWorktreeStart};
    use crate::stack_parent::SpawnStackParent;

    /// The directories a start is made in: its session directory, the sessions base and the
    /// daemon's data directory.
    struct ADaemonsDirectories {
        session_dir: tempfile::TempDir,
        sessions_base: tempfile::TempDir,
        tddy_data: tempfile::TempDir,
    }

    fn a_daemons_directories() -> ADaemonsDirectories {
        ADaemonsDirectories {
            session_dir: tempfile::tempdir().expect("session dir"),
            sessions_base: tempfile::tempdir().expect("sessions base"),
            tddy_data: tempfile::tempdir().expect("tddy data"),
        }
    }

    impl ADaemonsDirectories {
        /// A claude-cli start on this host naming `project_id` and the client's `repo_path`.
        fn a_start<'a>(&'a self, project_id: &'a str, repo_path: &'a str) -> CliWorktreeStart<'a> {
            CliWorktreeStart {
                session_type: "claude-cli",
                cut_label: "cut the session worktree",
                session_id: "01a1f24f-7302-7521-b951-e380ae501dd5",
                session_dir: self.session_dir.path(),
                sessions_base: self.sessions_base.path(),
                tddy_data_dir: self.tddy_data.path(),
                os_user: "testdev",
                project_id,
                repo_path,
                branch_worktree_intent: "new_branch_from_base",
                new_branch_name: "feature/hooks",
                selected_integration_base_ref: "",
                selected_branch_to_work_on: "",
                create_remote_branch: false,
                intent: tddy_core::BranchWorktreeIntent::NewBranchFromBase,
                timeout: Duration::from_secs(5),
                ssh_alias: "",
                shell: None,
            }
        }
    }

    async fn provisioned(start: &CliWorktreeStart<'_>) -> Result<CliWorktree, tddy_rpc::Status> {
        provision_cli_worktree(
            start,
            &SpawnStackParent::NoParent,
            &AttachmentProgressSink::discarding(),
        )
        .await
    }

    /// How a refused provision reads: its code and message.
    fn refusal(result: Result<CliWorktree, tddy_rpc::Status>) -> (tddy_rpc::Code, String) {
        match result {
            Ok(worktree) => panic!("provisioned {}", worktree.path.display()),
            Err(status) => (status.code(), status.message().to_string()),
        }
    }

    #[tokio::test]
    async fn a_client_checkout_is_the_worktree_as_it_is_and_belongs_to_no_project() {
        // Given a start naming the client's own checkout
        let directories = a_daemons_directories();
        let checkout = tempfile::tempdir().expect("the client's checkout");
        let checkout_path = checkout.path().to_string_lossy().into_owned();
        let start = directories.a_start("proj-1", &checkout_path);

        // When
        let worktree = provisioned(&start).await.expect("the checkout is used");

        // Then the checkout itself is the worktree, canonical, and no project owns it
        assert_eq!(
            (worktree.path, worktree.project.is_none()),
            (
                std::fs::canonicalize(checkout.path()).expect("the checkout resolves"),
                true
            )
        );
    }

    #[tokio::test]
    async fn a_client_checkout_that_is_a_file_is_refused() {
        // Given a start naming a file as the client's checkout
        let directories = a_daemons_directories();
        let dir = tempfile::tempdir().expect("a directory holding the file");
        let file = dir.path().join("README.md");
        std::fs::write(&file, "not a checkout").expect("the file is written");
        let file_path = file.to_string_lossy().into_owned();
        let start = directories.a_start("proj-1", &file_path);

        // When
        let refused = refusal(provisioned(&start).await);

        // Then
        assert_eq!(
            refused,
            (
                tddy_rpc::Code::InvalidArgument,
                format!(
                    "repo_path {} is not a directory",
                    std::fs::canonicalize(&file)
                        .expect("the file resolves")
                        .display()
                )
            )
        );
    }

    #[tokio::test]
    async fn a_client_checkout_that_does_not_exist_is_refused() {
        // Given a start naming a checkout that is not there
        let directories = a_daemons_directories();
        let missing = Path::new("/nonexistent/tddy/checkout");
        let start = directories.a_start("proj-1", "/nonexistent/tddy/checkout");

        // When
        let (code, message) = refusal(provisioned(&start).await);

        // Then the refusal names the path
        assert_eq!(
            (
                code,
                message.starts_with(&format!(
                    "repo_path {} is not accessible",
                    missing.display()
                ))
            ),
            (tddy_rpc::Code::InvalidArgument, true),
            "{message}"
        );
    }

    #[tokio::test]
    async fn a_start_naming_neither_a_project_nor_a_checkout_is_refused() {
        // Given
        let directories = a_daemons_directories();
        let start = directories.a_start("", "");

        // When
        let refused = refusal(provisioned(&start).await);

        // Then
        assert_eq!(
            refused,
            (
                tddy_rpc::Code::InvalidArgument,
                "project_id is required for claude-cli sessions".to_string()
            )
        );
    }
}

#[cfg(test)]
mod recorded_start_shell_tests {
    //! `#coder-start` 8/10: a start's recorded shell lists its calls while the start runs, and a
    //! session that keeps it afterwards neither holds the start's stream open nor reports past it.

    use std::sync::Arc;

    use tddy_remote_shell::{ExecOutput, ExecRequest, InMemoryShell, RemoteShell};
    use tddy_service::proto::session::start_session_event::Event;
    use tddy_session_files::attachment_progress::AttachmentProgressSink;

    use super::recorded_by;

    fn a_target() -> Arc<dyn RemoteShell> {
        let shell = InMemoryShell::new("coder.remote-dev");
        shell.answers_when(|_| Some(ExecOutput::succeeded(Vec::new())));
        Arc::new(shell)
    }

    #[tokio::test]
    async fn a_call_made_during_the_start_is_reported_on_its_stream() {
        // Given a start's progress stream and its target's shell, recorded
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let progress = AttachmentProgressSink::streaming(tx);
        let shell = recorded_by(a_target(), &progress);

        // When the start runs a command there
        shell
            .exec(ExecRequest::new("mkdir -p .tddy"))
            .await
            .unwrap();

        // Then the stream carries the call
        let event = rx.try_recv().expect("the call is reported").unwrap();
        assert!(
            matches!(&event.event, Some(Event::RemoteCall(call)) if call.command == "mkdir -p .tddy"),
            "{event:?}"
        );
    }

    #[tokio::test]
    async fn a_shell_kept_past_its_start_neither_holds_the_stream_open_nor_reports() {
        // Given a recorded shell whose start's own sender has been dropped
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let shell = recorded_by(a_target(), &AttachmentProgressSink::streaming(tx));

        // When the session still runs a command through it
        shell
            .exec(ExecRequest::new("rm -f hooks.log"))
            .await
            .unwrap();

        // Then the start's stream is closed, with nothing reported after the start
        assert!(rx.recv().await.is_none(), "the stream ended with the start");
    }
}

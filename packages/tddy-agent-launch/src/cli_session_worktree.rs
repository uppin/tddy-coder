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
            let path = cut_worktree(start, &repo_root, chain_base_ref).await?;
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
    chain_base_ref: Option<String>,
) -> Result<PathBuf, Status> {
    let worktree_base_ref =
        tddy_core::select_worktree_base_ref(start.selected_integration_base_ref, chain_base_ref);
    let wt = tddy_session_split::service_util::create_session_worktree(
        start.timeout,
        start.cut_label,
        repo_root,
        start.session_dir,
        worktree_base_ref,
    )
    .await?;
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

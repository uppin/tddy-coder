use super::DaemonSessionHost;

use tddy_projects::project_storage;
use tddy_rpc::Status;

use std::path::PathBuf;

use std::path::Path;

use super::JailBranch;

use crate::{
    connection_service::{hooks_and_urls, service_util, stack_parent},
    user_sessions_path::projects_path_for_user,
};

impl DaemonSessionHost {
    pub(super) fn project_default_branch_ref(
        &self,
        os_user: &str,
        project_id: &str,
        repo_path: &str,
    ) -> Option<String> {
        let project_default_branch_ref: Option<String> =
            if repo_path.is_empty() && !project_id.is_empty() {
                projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                    .and_then(|dir| {
                        project_storage::find_project(&dir, project_id)
                            .ok()
                            .flatten()
                    })
                    .and_then(|p| p.main_branch_ref)
            } else {
                None
            };
        project_default_branch_ref
    }

    pub(super) async fn create_jail_project_worktree(
        &self,
        branch: &JailBranch<'_>,
        session_dir: &Path,
        intent: tddy_core::BranchWorktreeIntent,
        pid: &str,
        repo_root: &Path,
    ) -> Result<PathBuf, Status> {
        let chain_base_ref = self
            .resolve_chain_base_ref_status(&stack_parent::StackBaseLookup {
                session_token: branch.session_token,
                stack_parent: branch.stack_parent,
                stack_parent_daemon_instance_id: branch.stack_parent_daemon_instance_id,
                project_id: pid,
                sessions_base: branch.sessions_base,
                repo_root,
                new_branch_name: branch.new_branch_name,
                stack_node_id: branch.stack_node_id,
                selected_integration_base_ref: branch.selected_integration_base_ref,
            })
            .await?;
        let worktree_base_ref = tddy_core::select_worktree_base_ref(
            branch.selected_integration_base_ref,
            chain_base_ref,
        );
        let timeout = self.config.spawn_worker_request_timeout();
        let wt = service_util::create_session_worktree(
            timeout,
            "start_sandboxed_claude_cli_session: create worktree",
            repo_root,
            session_dir,
            worktree_base_ref,
        )
        .await?;
        service_util::push_new_branch_to_origin_if_requested(
            branch.create_remote_branch,
            intent,
            session_dir,
            &wt,
            timeout,
        )
        .await?;
        Ok(wt)
    }

    pub(super) async fn link_jail_branch_to_stack_node(
        &self,
        branch: &JailBranch<'_>,
        session_dir: &Path,
        pid: &str,
        projects_dir: &Path,
        repo_root: &Path,
    ) -> Result<(), Status> {
        let JailBranch {
            session_id,
            stack_parent_daemon_instance_id,
            stack_node_id,
            ..
        } = *branch;
        let remote =
            project_storage::effective_remote_name_for_project(projects_dir, pid, repo_root)
                .map_err(|e| Status::internal(e.to_string()))?;
        let spawned_branch = hooks_and_urls::spawned_branch_of_session(
            session_dir,
            hooks_and_urls::effective_spawn_branch(
                branch.branch_worktree_intent,
                branch.new_branch_name,
                branch.selected_branch_to_work_on,
                &remote,
            ),
        );
        // A failed link never fails the spawn (D36) — see the same call in
        // `spawn_claude_cli_session_inner`.
        if let Some(orchestrator) = branch.stack_parent {
            if let Err(status) = self
                .record_spawn_on_stack_node(&stack_parent::StackNodeLink {
                    session_token: branch.session_token,
                    orchestrator_session_id: orchestrator,
                    orchestrator_daemon_instance_id: stack_parent_daemon_instance_id,
                    node_id: stack_node_id,
                    child_session_id: session_id,
                    // The branch as the session recorded it, suffix and all — see
                    // `spawned_branch_of_session`.
                    branch: &spawned_branch,
                    sessions_base: branch.sessions_base,
                })
                .await
            {
                log::error!(
                    target: "tddy_daemon::connection_service",
                    "session {session_id}: could not record its branch on pr-stack orchestrator {orchestrator} (node {stack_node_id:?}, daemon {stack_parent_daemon_instance_id:?}): {}; the node keeps no branch and its descendants stay unspawnable until it is re-linked",
                    status.message()
                );
            }
        }
        Ok(())
    }
}

//! Bodies of the `WorkQueueHandler` methods that `ports.rs` delegates here, as inherent methods
//! of [`WorkQueueRpcHandler`].
//!
//! TODO: these are a verbatim hand move out of the trait impl in `ports.rs` — the restructure
//! engine cannot cut a trait impl (no extract-into-function after a `forward_if_addressed` guard).

use super::guards::{token_for_repoint, validate_repoint_target};
use super::pr_status::owner_repo_from_repo_root;
use super::project_stacks::{require_a_stack, work_queue_target};
use super::WorkQueueRpcHandler;
use std::sync::Arc;
use tddy_projects::project_storage;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{RepointWorkItemRequest, RepointWorkItemResponse};
use tddy_session_lifecycle::connection_service::{local_gh_github_token, project_github_token};
use tddy_session_lifecycle::session_list_enrichment;
use tddy_session_lifecycle::user_sessions_path::{projects_path_for_user, sessions_base_for_user};

impl WorkQueueRpcHandler {
    pub(super) async fn repoint_work_item_rpc(
        &self,
        request: Request<RepointWorkItemRequest>,
    ) -> Result<Response<RepointWorkItemResponse>, Status> {
        let req = request.into_inner();
        let (github_user, os_user_owned) = self.authenticated_user(&req.session_token)?;
        let target = work_queue_target(req.stack.as_ref(), &req.session_id)?;
        if let Some(answered) = self
            .forward_if_addressed::<_, RepointWorkItemResponse>("RepointWorkItem", &target, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }
        let _ = &github_user;
        let os_user = &os_user_owned;
        if req.item_id.trim().is_empty() {
            return Err(Status::invalid_argument("node_id is required"));
        }
        let sessions_base = sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = self.session_dir_for_target(&sessions_base, &target)?;
        require_a_stack(&session_dir)?;

        // A repoint rebases a local branch and re-targets a PR base, so it needs the real checkout:
        // an unknown one is a precondition failure, not a repoint attempted in the session directory.
        let repo_root = tddy_core::repo_root_for_session(&session_dir).ok_or_else(|| {
            Status::failed_precondition(
                "no checkout is recorded for this session, so its repository could not be resolved",
            )
        })?;
        let owner_repo = owner_repo_from_repo_root(&repo_root).unwrap_or_default();
        let default_branch =
            tddy_core::resolve_default_integration_base_ref(&repo_root).map_err(|e| {
                Status::failed_precondition(format!("could not resolve default branch: {e}"))
            })?;

        let node_id = req.item_id.trim().to_string();

        // On the wire, an unnamed target means "the project's default branch" — the daemon's own
        // resolved ref, substituted here.
        //
        // A client cannot always name it: `ProjectEntry.main_branch_ref` is empty for a project that
        // stores no default, and the web then renders "Repoint to default branch". Forwarding that
        // empty string would instead select the recipe's drop-merged-parents rule, which in the very
        // case this feature exists for — a predecessor whose PR merged but whose plan still records
        // `open` — drops nothing at all and returns success against an unchanged plan. The operator
        // would see no error and no change.
        //
        // The recipe's `None` mode is therefore in-process only; it is never reachable from here.
        let requested_target = if req.target_base_branch.trim().is_empty() {
            default_branch.as_str()
        } else {
            req.target_base_branch.as_str()
        };

        // The named target must be a branch this node can be based onto: the repoint retains only
        // the parents that own it, so an unvalidated target is a silent plan rewrite.
        let stack = tddy_core::changeset::read_session_stack(&session_dir)
            .map_err(|e| Status::internal(e.to_string()))?
            .unwrap_or_default();
        let parent_branches: Vec<String> = stack
            .node(&node_id)
            .map(|node| {
                node.parents
                    .iter()
                    .filter_map(|parent_id| stack.node(parent_id)?.branch.clone())
                    .collect()
            })
            .unwrap_or_default();
        let target_base_branch = validate_repoint_target(
            requested_target,
            &default_branch,
            &parent_branches
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>(),
        )
        .map_err(Status::invalid_argument)?;

        // Only a node that owns a branch has a PR to re-target, so only it needs GitHub. Its token is
        // the account the orchestrator's project is assigned, from the caller's own vault — resolved
        // here, before the plan is rewritten, so a refusal changes nothing. The login-keyed
        // `retained_github_token` is not used: it is the credential this stack retires.
        let node_owns_branch = stack
            .node(&node_id)
            .is_some_and(|node| node.branch.is_some());
        let token = token_for_repoint(node_owns_branch, || {
            let project_id = tddy_core::read_session_metadata(&session_dir)
                .map_err(|e| format!("the orchestrator's session metadata could not be read: {e}"))?
                .project_id;
            let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| "could not resolve the projects path".to_string())?;
            let project = project_storage::find_project(&projects_dir, project_id.trim())
                .map_err(|e| format!("the orchestrator's project could not be read: {e}"))?
                .ok_or_else(|| {
                    format!("the orchestrator's project '{project_id}' is not registered")
                })?;
            // The session's own source (its sidecar), else this host's row of its project.
            let local_gh = match tddy_core::read_session_github_source(&session_dir)
                .map_err(|e| format!("the orchestrator's GitHub source could not be read: {e}"))?
            {
                Some(source) => source == tddy_core::SessionGithubSource::LocalGh,
                None => project.local_gh,
            };
            if local_gh {
                return local_gh_github_token(&self.github_cli, os_user, &project.git_url);
            }
            project_github_token(
                self.credential_vaults.clone(),
                Arc::clone(&self.user_resolver),
                &req.session_token,
                &project.accounts,
            )
        })?;

        let session_dir_for_op = session_dir.clone();
        tokio::task::spawn_blocking(move || {
            // A plan-only repoint reaches no GitHub call, so it holds no credential.
            let gh = match token {
                Some(token) => tddy_github::pr_api::RealGithubPrApi::with_token(owner_repo, token),
                None => tddy_github::pr_api::RealGithubPrApi::without_credential(owner_repo),
            };
            tddy_work_queue::item_ops::repoint_work_item(
                &session_dir_for_op,
                &repo_root,
                &node_id,
                &default_branch,
                target_base_branch.as_deref(),
                &gh,
            )
        })
        .await
        .map_err(|e| Status::internal(format!("repoint_work_item join error: {e}")))?
        .map_err(Status::failed_precondition)?;

        // Re-read the just-updated changeset and reuse the same serializer as `ListSessions`
        // enrichment so the response's `work_queue_json` is the exact wire shape `PrStackScreen`
        // already knows how to parse.
        let work_queue_json = session_list_enrichment::work_queue_json_of(&session_dir);
        Ok(Response::new(RepointWorkItemResponse { work_queue_json }))
    }
}

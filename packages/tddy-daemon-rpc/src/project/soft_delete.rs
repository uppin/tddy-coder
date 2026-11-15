//! `DeleteProject` and `RestoreProject`: soft-delete a project, and bring it back, on every host
//! owning it ([`super::fan_out`]).
//!
//! A delete only tombstones the `projects.yaml` row (`deleted_at`, `deleted_by`): the main
//! checkout, worktrees, session directories, stacks and settings all stay on disk. It runs in two
//! phases so no host is tombstoned while another refuses: a `preflight_only` round asks every host
//! for the project's running ordinary sessions, and any answer refuses the whole delete, naming
//! them; then the tombstone — stamped once, here — is written everywhere, and each host stops the
//! project's project-home sessions (process only; their records are kept for a restore).

use std::path::PathBuf;

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::user_paths::{projects_path_for_user, sessions_base_for_user};
use tddy_projects::project_storage::{self, ProjectData};
use tddy_rpc::{Code, Request, Response, Status};
use tddy_service::proto::project::{
    DeleteProjectRequest, DeleteProjectResponse, ProjectEntry, RestoreProjectRequest,
    RestoreProjectResponse,
};

use super::entries;
use super::project_sessions::{sessions_blocking_a_delete, stop_project_home_sessions};
use super::ProjectRpcHandler;

/// Who is calling, and where their projects and sessions live on this host.
pub(super) struct ProjectCaller {
    pub github_user: String,
    pub projects_dir: PathBuf,
    pub sessions_base: PathBuf,
    /// This daemon's instance id: the host a local row is reported under.
    pub local_id: String,
}

impl ProjectRpcHandler {
    pub(super) fn project_caller(&self, session_token: &str) -> Result<ProjectCaller, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let projects_dir = projects_path_for_user(&os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let sessions_base = sessions_base_for_user(&os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        Ok(ProjectCaller {
            github_user,
            projects_dir,
            sessions_base,
            local_id: local_instance_id_for_config(&self.config),
        })
    }

    /// `row` as this host lists it.
    pub(super) fn local_entry(&self, caller: &ProjectCaller, row: &ProjectData) -> ProjectEntry {
        let default_remote = entries::resolve_default_remote_or_empty(
            &caller.projects_dir,
            &row.project_id,
            &PathBuf::from(&row.main_repo_path),
        );
        entries::project_entry_from(row, caller.local_id.clone(), default_remote)
    }

    pub(crate) async fn delete_project_at_project_coordinate(
        &self,
        request: Request<DeleteProjectRequest>,
    ) -> Result<Response<DeleteProjectResponse>, Status> {
        let req = request.into_inner();
        let caller = self.project_caller(&req.session_token)?;
        let project_id = required_project_id(&req.project_id)?;
        if req.local_only {
            return self.delete_here(&caller, &req, &project_id).map(Response::new);
        }

        // Phase one: every reachable host's running sessions of the project refuse the delete.
        let mut blocking = match project_storage::find_project(&caller.projects_dir, &project_id)
            .map_err(|e| Status::internal(e.to_string()))?
        {
            Some(_) => {
                sessions_blocking_a_delete(&caller.sessions_base, &project_id, &caller.local_id)?
            }
            None => Vec::new(),
        };
        let preflight = DeleteProjectRequest {
            local_only: true,
            preflight_only: true,
            ..req.clone()
        };
        let checked = self
            .fan_out_to_peers("DeleteProject", preflight.clone(), |room, peer, r| async move {
                tddy_daemon_kernel::peer_calls::forward_delete_project(&room, &peer, &r).await
            })
            .await;
        blocking.extend(
            checked
                .failed
                .iter()
                .filter(|(_, s)| s.code == Code::FailedPrecondition)
                .map(|(_, s)| s.message.clone()),
        );
        if !blocking.is_empty() {
            return Err(Status::failed_precondition(format!(
                "project {project_id} has running sessions: {}; stop them before deleting the project",
                blocking.join(", ")
            )));
        }

        // Phase two: one tombstone, stamped here, written on every host owning the project.
        let stamped = DeleteProjectRequest {
            local_only: true,
            preflight_only: false,
            deleted_at: project_storage::lifecycle_timestamp_now(),
            deleted_by: caller.github_user.clone(),
            ..req
        };
        let here = match self.delete_here(&caller, &stamped, &project_id) {
            Ok(answer) => Some(answer),
            Err(status) if status.code == Code::NotFound => None,
            Err(status) => return Err(status),
        };
        let peers = self
            .fan_out_to_peers("DeleteProject", stamped.clone(), |room, peer, r| async move {
                tddy_daemon_kernel::peer_calls::forward_delete_project(&room, &peer, &r).await
            })
            .await;
        let unconfirmed_hosts = peers.unconfirmed();
        let answers = here
            .into_iter()
            .chain(peers.applied.into_iter().map(|(_, a)| a))
            .collect::<Vec<DeleteProjectResponse>>();
        let Some(first) = answers.first() else {
            return Err(not_on_any_reachable_host(&project_id));
        };
        log::info!(
            "DeleteProject: project_id={project_id} by={} applied_hosts={}",
            caller.github_user,
            answers.len()
        );
        Ok(Response::new(DeleteProjectResponse {
            project: first.project.clone(),
            applied_hosts: answers.iter().flat_map(|a| a.applied_hosts.clone()).collect(),
            unconfirmed_hosts,
            stopped_session_ids: answers
                .iter()
                .flat_map(|a| a.stopped_session_ids.clone())
                .collect(),
        }))
    }

    /// One host's part of a delete: its preflight answer, or its tombstone and stopped main.
    fn delete_here(
        &self,
        caller: &ProjectCaller,
        req: &DeleteProjectRequest,
        project_id: &str,
    ) -> Result<DeleteProjectResponse, Status> {
        if project_storage::find_project(&caller.projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .is_none()
        {
            return Err(Status::not_found("project not found"));
        }
        if req.preflight_only {
            let blocking =
                sessions_blocking_a_delete(&caller.sessions_base, project_id, &caller.local_id)?;
            if !blocking.is_empty() {
                return Err(Status::failed_precondition(blocking.join(", ")));
            }
            return Ok(DeleteProjectResponse::default());
        }
        let deleted_at = non_empty_or(&req.deleted_at, project_storage::lifecycle_timestamp_now);
        let deleted_by = non_empty_or(&req.deleted_by, || caller.github_user.clone());
        let row = project_storage::mark_project_deleted(
            &caller.projects_dir,
            project_id,
            &deleted_at,
            &deleted_by,
        )
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("project not found"))?;
        let stopped_session_ids = stop_project_home_sessions(&caller.sessions_base, project_id)?;
        Ok(DeleteProjectResponse {
            project: Some(self.local_entry(caller, &row)),
            applied_hosts: vec![caller.local_id.clone()],
            unconfirmed_hosts: Vec::new(),
            stopped_session_ids,
        })
    }

    pub(crate) async fn restore_project_at_project_coordinate(
        &self,
        request: Request<RestoreProjectRequest>,
    ) -> Result<Response<RestoreProjectResponse>, Status> {
        let req = request.into_inner();
        let caller = self.project_caller(&req.session_token)?;
        let project_id = required_project_id(&req.project_id)?;
        if req.local_only {
            return self.restore_here(&caller, &req, &project_id).map(Response::new);
        }
        self.restore_everywhere(&caller, &req.session_token, &project_id)
            .await
            .map(Response::new)
    }

    /// Clear `project_id`'s tombstone here and on every peer owning it, stamped once, here. Also
    /// the path a `CreateProject` or `AddProjectToHost` that meets a deleted project takes.
    pub(super) async fn restore_everywhere(
        &self,
        caller: &ProjectCaller,
        session_token: &str,
        project_id: &str,
    ) -> Result<RestoreProjectResponse, Status> {
        let stamped = RestoreProjectRequest {
            session_token: session_token.to_string(),
            project_id: project_id.to_string(),
            local_only: true,
            restored_at: project_storage::lifecycle_timestamp_now(),
        };
        let here = match self.restore_here(caller, &stamped, project_id) {
            Ok(answer) => Some(answer),
            Err(status) if status.code == Code::NotFound => None,
            Err(status) => return Err(status),
        };
        let peers = self
            .fan_out_to_peers("RestoreProject", stamped.clone(), |room, peer, r| async move {
                tddy_daemon_kernel::peer_calls::forward_restore_project(&room, &peer, &r).await
            })
            .await;
        let unconfirmed_hosts = peers.unconfirmed();
        let answers = here
            .into_iter()
            .chain(peers.applied.into_iter().map(|(_, a)| a))
            .collect::<Vec<RestoreProjectResponse>>();
        let Some(first) = answers.first() else {
            return Err(not_on_any_reachable_host(project_id));
        };
        log::info!(
            "RestoreProject: project_id={project_id} applied_hosts={}",
            answers.len()
        );
        Ok(RestoreProjectResponse {
            project: first.project.clone(),
            applied_hosts: answers.iter().flat_map(|a| a.applied_hosts.clone()).collect(),
            unconfirmed_hosts,
        })
    }

    fn restore_here(
        &self,
        caller: &ProjectCaller,
        req: &RestoreProjectRequest,
        project_id: &str,
    ) -> Result<RestoreProjectResponse, Status> {
        let restored_at = non_empty_or(&req.restored_at, project_storage::lifecycle_timestamp_now);
        let row = project_storage::mark_project_restored(&caller.projects_dir, project_id, &restored_at)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;
        Ok(RestoreProjectResponse {
            project: Some(self.local_entry(caller, &row)),
            applied_hosts: vec![caller.local_id.clone()],
            unconfirmed_hosts: Vec::new(),
        })
    }
}

pub(super) fn required_project_id(project_id: &str) -> Result<String, Status> {
    let project_id = project_id.trim();
    if project_id.is_empty() {
        return Err(Status::invalid_argument("project_id is required"));
    }
    Ok(project_id.to_string())
}

pub(super) fn not_on_any_reachable_host(project_id: &str) -> Status {
    Status::not_found(format!(
        "project {project_id} is not registered on any reachable host"
    ))
}

fn non_empty_or(value: &str, otherwise: impl FnOnce() -> String) -> String {
    if value.trim().is_empty() {
        otherwise()
    } else {
        value.to_string()
    }
}

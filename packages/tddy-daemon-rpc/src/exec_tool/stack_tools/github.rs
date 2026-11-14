//! How a stack tool reaches GitHub: as the **project's account**, for the project's repository.
//!
//! Everything is read from the project's row when a tool asks, never at construction: the plan
//! writers reach no GitHub and must not fail for want of a token.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_agent_launch::session_acting_identity::{local_gh_github_token, SessionAccountAccess};
use tddy_github::pr_api::owner_repo_from_remote_url;
use tddy_host_service::GithubCli;
use tddy_projects::project_storage::{
    effective_integration_base_ref_for_project, find_project, ProjectData,
};

use super::{StackGithub, StackGithubClients};

/// The project, its repository and its account, for one stack-tool call.
pub(super) struct GithubSource {
    /// The host's `projects.yaml` directory (also the stack store's).
    pub(super) projects_dir: PathBuf,
    pub(super) project_id: String,
    pub(super) os_user: String,
    /// The signed-in person's vault, which a project's assigned GitHub account is read from.
    pub(super) identity: SessionAccountAccess,
    /// The `gh` a Local gh project's token is asked of.
    pub(super) github_cli: GithubCli,
    pub(super) clients: Arc<dyn StackGithubClients>,
}

impl GithubSource {
    fn project(&self) -> Result<ProjectData, String> {
        find_project(&self.projects_dir, &self.project_id)
            .map_err(|e| format!("project {} could not be read: {e:#}", self.project_id))?
            .ok_or_else(|| format!("project {} is not registered on this host", self.project_id))
    }

    /// `owner/repo` of the project's repository.
    pub(super) fn repo(&self) -> Result<String, String> {
        repo_of(&self.project()?)
    }

    /// A client for the project's repository, authenticated as the project's account: the host's
    /// own `gh` login for a Local gh project, else the account the row assigns, from the vault.
    pub(super) fn client(&self) -> Result<Box<dyn StackGithub>, String> {
        let project = self.project()?;
        let repo = repo_of(&project)?;
        let token = if project.local_gh {
            local_gh_github_token(&self.github_cli, &self.os_user, &project.git_url)?
        } else {
            self.identity
                .acting_identity(&project.accounts)
                .map(|acting| acting.token)
                .map_err(|refusal| refusal.to_string())?
        };
        Ok(self.clients.client_for(&repo, &token))
    }

    /// The project's integration base ref, as `origin/main`.
    pub(super) fn default_branch(&self) -> Result<String, String> {
        effective_integration_base_ref_for_project(&self.projects_dir, &self.project_id)
            .map_err(|e| format!("could not resolve the project's default branch: {e:#}"))
    }
}

fn repo_of(project: &ProjectData) -> Result<String, String> {
    owner_repo_from_remote_url(&project.git_url).ok_or_else(|| {
        format!(
            "project {} names no GitHub repository: its git url is {:?}",
            project.project_id, project.git_url
        )
    })
}

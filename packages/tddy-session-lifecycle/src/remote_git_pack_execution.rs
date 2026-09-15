//! Resolve [`PackExecution`] for `RemoteGitService.Serve` from session metadata n2 persists.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::default_remote_repo_root;
use tddy_projects::project_storage;
use tddy_worktree_service::remote_git_service::{
    pack_execution_for_session, PackExecution, PackExecutionResolver,
    ProjectsDirResolver, SessionsBaseResolver,
};

use crate::session_reader::{list_sessions_in_dir, SessionEntry};

/// Production resolver: when a project has an SSH-backed session, pack verbs run on that target.
pub fn pack_execution_resolver_from_sessions(
    sessions_base_resolver: SessionsBaseResolver,
    projects_dir_resolver: ProjectsDirResolver,
) -> PackExecutionResolver {
    Arc::new(move |ctx| {
        resolve_pack_execution(
            &sessions_base_resolver,
            &projects_dir_resolver,
            &ctx.os_user,
            &ctx.project_ref,
            ctx.local_repo_path,
        )
    })
}

fn resolve_pack_execution(
    sessions_base_resolver: &SessionsBaseResolver,
    projects_dir_resolver: &ProjectsDirResolver,
    os_user: &str,
    project_ref: &str,
    local_repo_path: PathBuf,
) -> PackExecution {
    let Some(sessions_base) = sessions_base_resolver(os_user) else {
        return pack_execution_for_session(local_repo_path, "", "");
    };
    let Some(projects_dir) = projects_dir_resolver(os_user) else {
        return pack_execution_for_session(local_repo_path, "", "");
    };
    let project = match project_storage::find_project_by_ref(&projects_dir, project_ref) {
        Ok(Some(p)) => p,
        _ => return pack_execution_for_session(local_repo_path, "", ""),
    };
    let sessions = list_sessions_in_dir(&sessions_base).unwrap_or_default();
    let Some(session) = best_ssh_session_for_project(&sessions, &project.project_id) else {
        return pack_execution_for_session(local_repo_path, "", "");
    };
    let remote_repo_path = remote_repo_path_for_session(session, &project.git_url);
    pack_execution_for_session(
        local_repo_path,
        &session.ssh_config_host,
        &remote_repo_path,
    )
}

fn best_ssh_session_for_project<'a>(
    sessions: &'a [SessionEntry],
    project_id: &str,
) -> Option<&'a SessionEntry> {
    let mut ssh_sessions: Vec<&SessionEntry> = sessions
        .iter()
        .filter(|s| s.project_id == project_id && !s.ssh_config_host.trim().is_empty())
        .collect();
    if ssh_sessions.is_empty() {
        return None;
    }
    ssh_sessions.sort_by(|a, b| {
        b.is_active
            .cmp(&a.is_active)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
    ssh_sessions.first().copied()
}

fn remote_repo_path_for_session(session: &SessionEntry, git_url: &str) -> String {
    let trimmed = session.repo_path.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    format!(
        "{}/.worktrees/{}",
        default_remote_repo_root(git_url).trim_end_matches('/'),
        session.session_id
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use std::path::Path;

    use tddy_core::{write_session_metadata, SessionMetadata};
    use tddy_projects::project_storage::{write_projects, ProjectData};
    use tddy_worktree_service::remote_git_service::{PackExecution, PackExecutionContext};

    use super::*;

    fn write_session(
        sessions_base: &Path,
        session_id: &str,
        project_id: &str,
        ssh_host: &str,
        repo_path: &str,
        updated_at: &str,
    ) {
        let session_dir = sessions_base.join("sessions").join(session_id);
        std::fs::create_dir_all(&session_dir).expect("session dir");
        write_session_metadata(
            &session_dir,
            &SessionMetadata {
                session_id: session_id.to_string(),
                project_id: project_id.to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: updated_at.to_string(),
                status: "running".to_string(),
                repo_path: Some(repo_path.to_string()),
                pid: None,
                tool: None,
                livekit_room: None,
                pending_elicitation: false,
                previous_session_id: None,
                session_type: None,
                model: None,
                cursor_chat_id: None,
                activity_status: None,
                hook_token: None,
                sandbox: None,
                agent: None,
                recipe: None,
                agents: Vec::new(),
                agents_rev: 0,
                legacy_specialized_agents: Vec::new(),
                codebase_daemon_instance_id: None,
                codebase_session_id: None,
                agent_daemon_instance_id: None,
                agent_session_id: None,
                ssh_config_host: Some(ssh_host.to_string()),
            },
        )
        .expect("write session metadata");
    }

    #[test]
    fn resolver_without_ssh_sessions_packs_locally() {
        // Given
        let home = tempfile::tempdir().expect("tempdir");
        let sessions_base = home.path().join(".tddy");
        let projects_dir = sessions_base.join("projects");
        let local_repo = home.path().join("repos").join("app");
        std::fs::create_dir_all(&local_repo).expect("repo");
        write_projects(
            &projects_dir,
            &[ProjectData {
                project_id: "proj-1".to_string(),
                name: "my-app".to_string(),
                git_url: "https://github.com/example/my-app.git".to_string(),
                main_repo_path: local_repo.to_string_lossy().to_string(),
                main_branch_ref: None,
                remote_name: None,
                host_repo_paths: HashMap::new(),
            }],
        )
        .expect("projects");

        let sessions_base_owned = sessions_base.clone();
        let projects_dir_owned = projects_dir.clone();
        let sessions_base_resolver: SessionsBaseResolver =
            Arc::new(move |_| Some(sessions_base_owned.clone()));
        let projects_dir_resolver: ProjectsDirResolver =
            Arc::new(move |_| Some(projects_dir_owned.clone()));
        let resolver =
            pack_execution_resolver_from_sessions(sessions_base_resolver, projects_dir_resolver);

        // When
        let execution = resolver(PackExecutionContext {
            os_user: "dev".to_string(),
            project_ref: "my-app".to_string(),
            local_repo_path: local_repo.clone(),
        });

        // Then
        assert_eq!(
            execution,
            PackExecution::Local {
                repo_path: local_repo
            }
        );
    }

    #[test]
    fn resolver_picks_an_ssh_backed_session_for_the_project() {
        // Given
        let home = tempfile::tempdir().expect("tempdir");
        let sessions_base = home.path().join(".tddy");
        let projects_dir = sessions_base.join("projects");
        let local_repo = home.path().join("repos").join("app");
        std::fs::create_dir_all(&local_repo).expect("repo");
        write_projects(
            &projects_dir,
            &[ProjectData {
                project_id: "proj-1".to_string(),
                name: "my-app".to_string(),
                git_url: "https://github.com/example/my-app.git".to_string(),
                main_repo_path: local_repo.to_string_lossy().to_string(),
                main_branch_ref: None,
                remote_name: None,
                host_repo_paths: HashMap::new(),
            }],
        )
        .expect("projects");
        write_session(
            &sessions_base,
            "sess-a",
            "proj-1",
            "buildbox",
            "/home/dev/repo/.worktrees/sess-a",
            "2026-01-02T00:00:00Z",
        );
        write_session(
            &sessions_base,
            "sess-b",
            "proj-1",
            "buildbox",
            "/home/dev/repo/.worktrees/sess-b",
            "2026-01-03T00:00:00Z",
        );

        let sessions_base_owned = sessions_base.clone();
        let projects_dir_owned = projects_dir.clone();
        let sessions_base_resolver: SessionsBaseResolver =
            Arc::new(move |_| Some(sessions_base_owned.clone()));
        let projects_dir_resolver: ProjectsDirResolver =
            Arc::new(move |_| Some(projects_dir_owned.clone()));
        let resolver =
            pack_execution_resolver_from_sessions(sessions_base_resolver, projects_dir_resolver);

        // When
        let execution = resolver(PackExecutionContext {
            os_user: "dev".to_string(),
            project_ref: "my-app".to_string(),
            local_repo_path: local_repo.clone(),
        });

        // Then — newest `updated_at` wins when no session has a live pid
        assert_eq!(
            execution,
            PackExecution::Remote {
                ssh_config_host: "buildbox".to_string(),
                remote_repo_path: "/home/dev/repo/.worktrees/sess-b".to_string(),
            }
        );
    }
}

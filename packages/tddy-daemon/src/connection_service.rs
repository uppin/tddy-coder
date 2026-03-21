//! ConnectionService implementation for daemon session/tool management.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::read_session_metadata;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::connection::{
    ConnectSessionRequest, ConnectSessionResponse, ConnectionService as ConnectionServiceTrait,
    CreateProjectRequest, CreateProjectResponse, ListProjectsRequest, ListProjectsResponse,
    ListSessionsRequest, ListSessionsResponse, ListToolsRequest, ListToolsResponse,
    ProjectEntry as ProtoProjectEntry, ResumeSessionRequest, ResumeSessionResponse,
    SessionEntry as ProtoSessionEntry, Signal, SignalSessionRequest, SignalSessionResponse,
    StartSessionRequest, StartSessionResponse, ToolInfo,
};
use uuid::Uuid;

use crate::config::DaemonConfig;
use crate::project_storage::{self, ProjectData};
use crate::session_reader;
use crate::spawn_worker;
use crate::spawner::{self, SpawnOptions};
use crate::user_sessions_path::{
    project_path_under_home_from_user_relative, projects_path_for_user, repos_base_for_user,
};

/// Resolves session token to GitHub user login.
pub type SessionUserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Resolves OS user to sessions base path.
pub type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// ConnectionService implementation.
pub struct ConnectionServiceImpl {
    config: DaemonConfig,
    sessions_base_for_user: SessionsBaseResolver,
    user_resolver: SessionUserResolver,
    spawn_client: Option<Arc<spawn_worker::SpawnClient>>,
}

impl ConnectionServiceImpl {
    pub fn new(
        config: DaemonConfig,
        sessions_base_for_user: SessionsBaseResolver,
        user_resolver: SessionUserResolver,
        spawn_client: Option<(spawn_worker::SpawnClient, i32)>,
    ) -> Self {
        let spawn_client = spawn_client.map(|(c, _pid)| Arc::new(c));
        Self {
            config,
            sessions_base_for_user,
            user_resolver,
            spawn_client,
        }
    }
}

#[async_trait::async_trait]
impl ConnectionServiceTrait for ConnectionServiceImpl {
    async fn list_tools(
        &self,
        _request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        let tools: Vec<ToolInfo> = self
            .config
            .allowed_tools()
            .iter()
            .map(|t| ToolInfo {
                path: t.path.clone(),
                label: t.label.clone().unwrap_or_else(|| t.path.clone()),
            })
            .collect();
        Ok(Response::new(ListToolsResponse { tools }))
    }

    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base = (self.sessions_base_for_user)(os_user)
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let sessions = session_reader::list_sessions_in_dir(&sessions_base)
            .map_err(|e| Status::internal(e.to_string()))?;
        let entries: Vec<ProtoSessionEntry> = sessions
            .into_iter()
            .map(|s| ProtoSessionEntry {
                session_id: s.session_id,
                created_at: s.created_at,
                status: s.status,
                repo_path: s.repo_path,
                pid: s.pid.unwrap_or(0),
                is_active: s.is_active,
                project_id: s.project_id,
            })
            .collect();
        Ok(Response::new(ListSessionsResponse { sessions: entries }))
    }

    async fn list_projects(
        &self,
        request: Request<ListProjectsRequest>,
    ) -> Result<Response<ListProjectsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let projects_dir = projects_path_for_user(os_user)
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let projects = project_storage::read_projects(&projects_dir)
            .map_err(|e| Status::internal(e.to_string()))?;
        let entries: Vec<ProtoProjectEntry> = projects
            .into_iter()
            .map(|p| ProtoProjectEntry {
                project_id: p.project_id,
                name: p.name,
                git_url: p.git_url,
                main_repo_path: p.main_repo_path,
            })
            .collect();
        Ok(Response::new(ListProjectsResponse { projects: entries }))
    }

    async fn create_project(
        &self,
        request: Request<CreateProjectRequest>,
    ) -> Result<Response<CreateProjectResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;

        let name = req.name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("project name is required"));
        }
        if name.contains('/') || name.contains("..") {
            return Err(Status::invalid_argument("invalid project name"));
        }
        let git_url = req.git_url.trim();
        if git_url.is_empty() {
            return Err(Status::invalid_argument("git_url is required"));
        }

        let projects_dir = projects_path_for_user(os_user)
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;

        let user_rel = req.user_relative_path.trim();
        let destination = if !user_rel.is_empty() {
            project_path_under_home_from_user_relative(os_user, user_rel)
                .map_err(Status::invalid_argument)?
        } else {
            let base = repos_base_for_user(os_user, self.config.repos_base_path_or_default())
                .ok_or_else(|| Status::internal("could not resolve repos base path"))?;
            base.join(name)
        };
        let spawn_client = self.spawn_client.clone();
        let os_user_owned = os_user.to_string();
        let git_url_owned = git_url.to_string();
        let dest_path = destination.clone();

        tokio::task::spawn_blocking(move || {
            if let Some(ref client) = spawn_client {
                client.clone_repo(spawn_worker::CloneRequest {
                    os_user: os_user_owned,
                    git_url: git_url_owned,
                    destination: dest_path.display().to_string(),
                })
            } else {
                spawner::clone_as_user(&os_user_owned, &git_url_owned, &dest_path)
            }
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .map_err(|e| {
            log::error!("clone project failed: {}", e);
            Status::internal(e.to_string())
        })?;

        let main_repo_path = destination
            .canonicalize()
            .unwrap_or(destination)
            .display()
            .to_string();

        let project = ProjectData {
            project_id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            git_url: git_url.to_string(),
            main_repo_path,
        };
        let entry = ProtoProjectEntry {
            project_id: project.project_id.clone(),
            name: project.name.clone(),
            git_url: project.git_url.clone(),
            main_repo_path: project.main_repo_path.clone(),
        };
        project_storage::add_project(&projects_dir, project)
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateProjectResponse {
            project: Some(entry),
        }))
    }

    async fn start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let livekit = spawner::livekit_creds_from_config(&self.config)
            .ok_or_else(|| Status::failed_precondition("LiveKit not configured"))?;

        let project_id_req = req.project_id.trim();
        if project_id_req.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }

        let projects_dir = projects_path_for_user(os_user)
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let project = project_storage::find_project(&projects_dir, project_id_req)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;

        let repo_path = Path::new(&project.main_repo_path);
        if !repo_path.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }

        log::debug!("StartSession: entering spawn_blocking session_id=new");
        let spawn_client = self.spawn_client.clone();
        let spawn_mouse = self.config.spawn_mouse;
        let os_user = os_user.to_string();
        let tool_path = req.tool_path.clone();
        let repo_path = repo_path.to_path_buf();
        let livekit = livekit.clone();
        let pid_for_spawn = project.project_id.clone();
        let agent_for_spawn: Option<String> = {
            let t = req.agent.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        let result = tokio::task::spawn_blocking(move || {
            log::debug!(
                "StartSession: spawn_blocking running, using_spawn_worker={}",
                spawn_client.is_some()
            );
            let pid = Some(pid_for_spawn.as_str());
            let agent = agent_for_spawn.as_deref();
            if let Some(ref client) = spawn_client {
                let spawn_req = spawn_worker::build_spawn_request(
                    &os_user,
                    &tool_path,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: None,
                        project_id: pid,
                        agent,
                        mouse: spawn_mouse,
                    },
                );
                client.spawn(spawn_req)
            } else {
                spawner::spawn_as_user(
                    &os_user,
                    &tool_path,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: None,
                        project_id: pid,
                        agent,
                        mouse: spawn_mouse,
                    },
                )
            }
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .map_err(|e| {
            log::error!("spawn failed: {}", e);
            Status::internal(e.to_string())
        })?;
        log::debug!(
            "StartSession: spawn_blocking returned, session_id={}",
            result.session_id
        );
        Ok(Response::new(StartSessionResponse {
            session_id: result.session_id,
            livekit_room: result.livekit_room,
            livekit_url: result.livekit_url,
            livekit_server_identity: result.livekit_server_identity,
        }))
    }

    async fn connect_session(
        &self,
        request: Request<ConnectSessionRequest>,
    ) -> Result<Response<ConnectSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base = (self.sessions_base_for_user)(os_user)
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = sessions_base.join(&req.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;
        let livekit_url = self
            .config
            .livekit
            .as_ref()
            .and_then(|l| l.public_url.clone())
            .or_else(|| self.config.livekit.as_ref().and_then(|l| l.url.clone()))
            .ok_or_else(|| Status::internal("LiveKit URL not configured"))?;
        let livekit_room = metadata
            .livekit_room
            .ok_or_else(|| Status::failed_precondition("session has no LiveKit room"))?;
        let livekit_server_identity = format!("daemon-{}", req.session_id);
        Ok(Response::new(ConnectSessionResponse {
            livekit_room,
            livekit_url,
            livekit_server_identity,
        }))
    }

    async fn resume_session(
        &self,
        request: Request<ResumeSessionRequest>,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base = (self.sessions_base_for_user)(os_user)
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = sessions_base.join(&req.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;
        let repo_path = metadata
            .repo_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| session_dir.clone());
        let repo_path = if repo_path.exists() {
            repo_path
        } else {
            session_dir.clone()
        };
        let tool_path = metadata.tool.as_deref().unwrap_or("tddy-coder").to_string();
        let livekit = spawner::livekit_creds_from_config(&self.config)
            .ok_or_else(|| Status::failed_precondition("LiveKit not configured"))?;
        let spawn_client = self.spawn_client.clone();
        let spawn_mouse = self.config.spawn_mouse;
        let os_user = os_user.to_string();
        let session_id = req.session_id.clone();
        let livekit = livekit.clone();
        let project_id_resume = metadata.project_id.clone();
        let result = tokio::task::spawn_blocking(move || {
            let pid = if project_id_resume.is_empty() {
                None
            } else {
                Some(project_id_resume.as_str())
            };
            if let Some(ref client) = spawn_client {
                let spawn_req = spawn_worker::build_spawn_request(
                    &os_user,
                    &tool_path,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: Some(session_id.as_str()),
                        project_id: pid,
                        agent: None,
                        mouse: spawn_mouse,
                    },
                );
                client.spawn(spawn_req)
            } else {
                spawner::spawn_as_user(
                    &os_user,
                    &tool_path,
                    &repo_path,
                    &livekit,
                    SpawnOptions {
                        resume_session_id: Some(session_id.as_str()),
                        project_id: pid,
                        agent: None,
                        mouse: spawn_mouse,
                    },
                )
            }
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))?
        .map_err(|e| {
            log::error!("spawn (resume) failed: {}", e);
            Status::internal(e.to_string())
        })?;
        Ok(Response::new(ResumeSessionResponse {
            session_id: result.session_id,
            livekit_room: result.livekit_room,
            livekit_url: result.livekit_url,
            livekit_server_identity: result.livekit_server_identity,
        }))
    }

    async fn signal_session(
        &self,
        request: Request<SignalSessionRequest>,
    ) -> Result<Response<SignalSessionResponse>, Status> {
        log::debug!(
            "{}",
            r#"{"tddy":{"marker_id":"M001","scope":"connection_service::signal_session","data":{}}}"#
        );

        let req = request.into_inner();
        log::debug!(
            "SignalSession: session_id={}, signal={}",
            req.session_id,
            req.signal
        );

        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base = (self.sessions_base_for_user)(os_user)
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;

        let session_dir = sessions_base.join(&req.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|_| Status::not_found("session not found"))?;

        let pid = metadata
            .pid
            .ok_or_else(|| Status::failed_precondition("session has no PID"))?;

        log::debug!(
            "SignalSession: resolved pid={} for session={}",
            pid,
            req.session_id
        );

        #[cfg(unix)]
        {
            let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
            if !alive {
                log::debug!("SignalSession: pid={} is not alive", pid);
                return Err(Status::failed_precondition("process is not alive"));
            }

            let os_signal = match Signal::try_from(req.signal) {
                Ok(Signal::Sigint) => libc::SIGINT,
                Ok(Signal::Sigterm) => libc::SIGTERM,
                Ok(Signal::Sigkill) => libc::SIGKILL,
                Err(_) => return Err(Status::invalid_argument("invalid signal value")),
            };

            log::info!(
                "SignalSession: sending signal {} to pid={} session={}",
                os_signal,
                pid,
                req.session_id
            );

            let ret = unsafe { libc::kill(pid as i32, os_signal) };
            if ret != 0 {
                let err = std::io::Error::last_os_error();
                log::error!(
                    "SignalSession: kill({}, {}) failed: {}",
                    pid,
                    os_signal,
                    err
                );
                return Err(Status::internal(format!("failed to send signal: {}", err)));
            }

            Ok(Response::new(SignalSessionResponse {
                ok: true,
                message: format!("signal {} sent to pid {}", os_signal, pid),
            }))
        }

        #[cfg(not(unix))]
        {
            let _ = pid;
            Err(Status::unimplemented(
                "signal delivery is only supported on Unix",
            ))
        }
    }
}

#[cfg(test)]
mod signal_session_unit_tests {
    use super::*;
    use tddy_core::SessionMetadata;

    fn make_unit_config() -> crate::config::DaemonConfig {
        let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        crate::config::DaemonConfig::load(&path).unwrap()
    }

    fn make_unit_service(sessions_base: std::path::PathBuf) -> ConnectionServiceImpl {
        let config = make_unit_config();
        let base = sessions_base.clone();
        let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
        let user_resolver: SessionUserResolver = Arc::new(|token| {
            if token == "valid" {
                Some("u".to_string())
            } else {
                None
            }
        });
        ConnectionServiceImpl::new(config, sessions_base_resolver, user_resolver, None)
    }

    fn write_unit_session(session_dir: &std::path::Path, pid: u32) {
        let session_id = session_dir
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let metadata = SessionMetadata {
            session_id,
            project_id: "proj-unit".to_string(),
            created_at: "2026-03-21T00:00:00Z".to_string(),
            updated_at: "2026-03-21T00:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: Some("/tmp".to_string()),
            pid: Some(pid),
            tool: None,
            livekit_room: None,
        };
        tddy_core::write_session_metadata(session_dir, &metadata).unwrap();
    }

    /// Unit: signal_session rejects an invalid (empty) session token.
    #[tokio::test]
    async fn signal_session_unit_rejects_invalid_token() {
        eprintln!(
            "{}",
            r#"{"tddy":{"marker_id":"M002","scope":"unit::signal_session_unit_rejects_invalid_token","data":{}}}"#
        );
        let temp = tempfile::tempdir().unwrap();
        let service = make_unit_service(temp.path().join("sessions"));
        let request = Request::new(SignalSessionRequest {
            session_token: "bad-token".to_string(),
            session_id: "any".to_string(),
            signal: Signal::Sigint as i32,
        });
        let result = service.signal_session(request).await;
        assert!(result.is_err(), "invalid token should return error");
        assert_eq!(result.unwrap_err().code, tddy_rpc::Code::Unauthenticated);
    }

    /// Unit: signal_session returns not-found for a session that has no yaml file.
    #[tokio::test]
    async fn signal_session_unit_returns_error_for_missing_session() {
        eprintln!(
            "{}",
            r#"{"tddy":{"marker_id":"M003","scope":"unit::signal_session_unit_returns_error_for_missing_session","data":{}}}"#
        );
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("sessions")).unwrap();
        let service = make_unit_service(temp.path().join("sessions"));
        let request = Request::new(SignalSessionRequest {
            session_token: "valid".to_string(),
            session_id: "no-such-session".to_string(),
            signal: Signal::Sigterm as i32,
        });
        let result = service.signal_session(request).await;
        assert!(result.is_err(), "missing session should return error");
        assert_eq!(result.unwrap_err().code, tddy_rpc::Code::NotFound);
    }

    /// Unit: signal_session with SIGKILL sends correct signal to a live process.
    #[tokio::test]
    async fn signal_session_unit_sigkill_reaches_live_process() {
        eprintln!(
            "{}",
            r#"{"tddy":{"marker_id":"M004","scope":"unit::signal_session_unit_sigkill_reaches_live_process","data":{}}}"#
        );
        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();

        let temp = tempfile::tempdir().unwrap();
        let sessions_base = temp.path().join("sessions");
        let session_dir = sessions_base.join("sigkill-session");
        std::fs::create_dir_all(&session_dir).unwrap();
        write_unit_session(&session_dir, pid);

        let service = make_unit_service(sessions_base);
        let request = Request::new(SignalSessionRequest {
            session_token: "valid".to_string(),
            session_id: "sigkill-session".to_string(),
            signal: Signal::Sigkill as i32,
        });
        let response = service.signal_session(request).await.unwrap();
        assert!(response.into_inner().ok);

        let status = child.wait().unwrap();
        assert!(!status.success(), "process should have been killed");
    }
}

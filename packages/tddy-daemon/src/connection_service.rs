//! ConnectionService implementation for daemon session/tool management.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::read_session_metadata;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::connection::{
    ConnectSessionRequest, ConnectSessionResponse, ConnectionService as ConnectionServiceTrait,
    ListSessionsRequest, ListSessionsResponse, ListToolsRequest, ListToolsResponse,
    ResumeSessionRequest, ResumeSessionResponse, SessionEntry as ProtoSessionEntry,
    StartSessionRequest, StartSessionResponse, ToolInfo,
};

use crate::config::DaemonConfig;
use crate::session_reader;
use crate::spawner;

/// Resolves session token to GitHub user login.
pub type SessionUserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Resolves OS user to sessions base path.
pub type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// ConnectionService implementation.
pub struct ConnectionServiceImpl {
    config: DaemonConfig,
    sessions_base_for_user: SessionsBaseResolver,
    user_resolver: SessionUserResolver,
}

impl ConnectionServiceImpl {
    pub fn new(
        config: DaemonConfig,
        sessions_base_for_user: SessionsBaseResolver,
        user_resolver: SessionUserResolver,
    ) -> Self {
        Self {
            config,
            sessions_base_for_user,
            user_resolver,
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
            })
            .collect();
        Ok(Response::new(ListSessionsResponse { sessions: entries }))
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
        let repo_path = Path::new(&req.repo_path);
        if !repo_path.exists() {
            return Err(Status::invalid_argument("repo path does not exist"));
        }
        let result = spawner::spawn_as_user(os_user, &req.tool_path, repo_path, &livekit, None)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(StartSessionResponse {
            session_id: result.session_id,
            livekit_room: result.livekit_room,
            livekit_url: result.livekit_url,
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
        Ok(Response::new(ConnectSessionResponse {
            livekit_room,
            livekit_url,
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
        let tool_path = metadata.tool.as_deref().unwrap_or("tddy-coder");
        let livekit = spawner::livekit_creds_from_config(&self.config)
            .ok_or_else(|| Status::failed_precondition("LiveKit not configured"))?;
        let result = spawner::spawn_as_user(
            os_user,
            tool_path,
            &repo_path,
            &livekit,
            Some(&req.session_id),
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ResumeSessionResponse {
            session_id: result.session_id,
            livekit_room: result.livekit_room,
            livekit_url: result.livekit_url,
        }))
    }
}

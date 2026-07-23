//! Session-addressed BSP service for daemon-managed sessions.
//!
//! The per-session [`tddy_bsp::BspServiceImpl`] is bound to one `(session_dir, repo_root)` at
//! construction — fine on the coder participant, which is single-session. Daemon-managed
//! claude-cli/cursor sessions instead share the daemon's one RPC surface, so every request must say
//! *which* session it targets. This service reads the `session_token`/`session_id` on each request,
//! resolves them to the session's worktree + catalog directory via the injected [`SessionPathsResolver`]
//! (built in `main.rs` from the daemon's token/user/sessions-base machinery), and delegates to a
//! `BspServiceImpl` scoped to that session.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_bsp::BspServiceImpl;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::bsp::{
    BspService, BuildTargetActionRequest, BuildTargetActionResponse, BuildTargetOutputPathsRequest,
    BuildTargetOutputPathsResponse, BuildTargetSourcesRequest, BuildTargetSourcesResponse,
    WorkspaceBuildTargetsRequest, WorkspaceBuildTargetsResponse, WorkspaceReloadRequest,
    WorkspaceReloadResponse,
};

/// Resolves a `(session_token, session_id)` to that session's `(session_dir, repo_root)`, or a
/// `Status` error (unauthenticated / permission-denied / not-found). Injected so the resolution
/// policy (token verification, os-user mapping, sessions-base + `.session.yaml` lookup) lives in the
/// daemon wiring and this service stays testable with a fake.
pub type SessionPathsResolver =
    Arc<dyn Fn(&str, &str) -> Result<(PathBuf, PathBuf), Status> + Send + Sync>;

/// A `bsp.BspService` that dispatches each request to the session it names.
pub struct DaemonBspService {
    resolver: SessionPathsResolver,
    tddy_data_dir: PathBuf,
}

impl DaemonBspService {
    pub fn new(resolver: SessionPathsResolver, tddy_data_dir: PathBuf) -> Self {
        Self {
            resolver,
            tddy_data_dir,
        }
    }

    /// Resolve the request's session and build a `BspServiceImpl` scoped to it.
    fn session_impl(&self, token: &str, session_id: &str) -> Result<BspServiceImpl, Status> {
        let (session_dir, repo_root) = (self.resolver)(token, session_id)?;
        Ok(BspServiceImpl::new(
            session_dir,
            repo_root,
            self.tddy_data_dir.clone(),
        ))
    }
}

#[async_trait]
impl BspService for DaemonBspService {
    async fn workspace_build_targets(
        &self,
        request: Request<WorkspaceBuildTargetsRequest>,
    ) -> Result<Response<WorkspaceBuildTargetsResponse>, Status> {
        let req = request.into_inner();
        let svc = self.session_impl(&req.session_token, &req.session_id)?;
        svc.workspace_build_targets(Request::new(req)).await
    }

    async fn workspace_reload(
        &self,
        request: Request<WorkspaceReloadRequest>,
    ) -> Result<Response<WorkspaceReloadResponse>, Status> {
        let req = request.into_inner();
        let svc = self.session_impl(&req.session_token, &req.session_id)?;
        svc.workspace_reload(Request::new(req)).await
    }

    async fn build_target_sources(
        &self,
        request: Request<BuildTargetSourcesRequest>,
    ) -> Result<Response<BuildTargetSourcesResponse>, Status> {
        let req = request.into_inner();
        let svc = self.session_impl(&req.session_token, &req.session_id)?;
        svc.build_target_sources(Request::new(req)).await
    }

    async fn build_target_output_paths(
        &self,
        request: Request<BuildTargetOutputPathsRequest>,
    ) -> Result<Response<BuildTargetOutputPathsResponse>, Status> {
        let req = request.into_inner();
        let svc = self.session_impl(&req.session_token, &req.session_id)?;
        svc.build_target_output_paths(Request::new(req)).await
    }

    async fn build_target_compile(
        &self,
        request: Request<BuildTargetActionRequest>,
    ) -> Result<Response<BuildTargetActionResponse>, Status> {
        let req = request.into_inner();
        let svc = self.session_impl(&req.session_token, &req.session_id)?;
        svc.build_target_compile(Request::new(req)).await
    }

    async fn build_target_test(
        &self,
        request: Request<BuildTargetActionRequest>,
    ) -> Result<Response<BuildTargetActionResponse>, Status> {
        let req = request.into_inner();
        let svc = self.session_impl(&req.session_token, &req.session_id)?;
        svc.build_target_test(Request::new(req)).await
    }

    async fn build_target_run(
        &self,
        request: Request<BuildTargetActionRequest>,
    ) -> Result<Response<BuildTargetActionResponse>, Status> {
        let req = request.into_inner();
        let svc = self.session_impl(&req.session_token, &req.session_id)?;
        svc.build_target_run(Request::new(req)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_RUST_LIBRARY: &str = "\
schema_version: 1
targets:
  - id: \"packages/foo:lib\"
    name: Foo library
    config: { type: rust_library, package: foo }
";

    /// A resolver that maps only the token `\"good\"` to the given paths.
    fn a_resolver_for(session_dir: PathBuf, repo_root: PathBuf) -> SessionPathsResolver {
        Arc::new(move |token: &str, _session_id: &str| {
            if token == "good" {
                Ok((session_dir.clone(), repo_root.clone()))
            } else {
                Err(Status::unauthenticated("invalid or expired session"))
            }
        })
    }

    #[tokio::test]
    async fn a_valid_session_lists_that_sessions_build_targets() {
        // Given — a repo with one target, reachable via the resolver for token "good".
        tddy_bsp::register_catalog_provider();
        let repo = tempfile::tempdir().expect("repo tempdir");
        std::fs::create_dir_all(repo.path().join("packages/foo")).expect("mkdir");
        std::fs::write(repo.path().join("packages/foo/BUILD.yaml"), A_RUST_LIBRARY)
            .expect("write BUILD.yaml");
        let session = tempfile::tempdir().expect("session tempdir");
        let data = tempfile::tempdir().expect("data tempdir");
        let svc = DaemonBspService::new(
            a_resolver_for(session.path().to_path_buf(), repo.path().to_path_buf()),
            data.path().to_path_buf(),
        );

        // When
        let resp = svc
            .workspace_build_targets(Request::new(WorkspaceBuildTargetsRequest {
                session_token: "good".to_string(),
                session_id: "s1".to_string(),
            }))
            .await
            .expect("ok");

        // Then
        let ids: Vec<String> = resp
            .into_inner()
            .targets
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(ids, vec!["packages/foo:lib".to_string()]);
    }

    #[tokio::test]
    async fn an_invalid_session_token_is_rejected_as_unauthenticated() {
        // Given
        let svc = DaemonBspService::new(
            a_resolver_for(PathBuf::from("/unused"), PathBuf::from("/unused")),
            PathBuf::from("/unused"),
        );

        // When
        let err = svc
            .workspace_build_targets(Request::new(WorkspaceBuildTargetsRequest {
                session_token: "bad".to_string(),
                session_id: "s1".to_string(),
            }))
            .await
            .expect_err("must reject");

        // Then
        assert_eq!(err.code(), tddy_rpc::Code::Unauthenticated);
    }
}

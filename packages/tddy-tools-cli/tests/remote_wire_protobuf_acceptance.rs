//! Acceptance tests: `tddy-tools remote start-session` reaches the daemon's `session.SessionService`
//! as a protobuf request, and the generated service decodes it.
//!
//! The CLI runs as a real process against a real loopback port. That port serves the same mount the
//! daemon builds (`MultiRpcService` behind `tddy_connectrpc::connect_router`), with a generated
//! `session.SessionService` implementation registered in it. The implementation is reached only
//! through the generated handler, which decodes the body with the prost type for
//! `StartSessionRequest`. A body that type cannot decode never reaches the implementation, so the
//! CLI call fails and nothing is recorded.

use std::sync::{Arc, Mutex};

use assert_cmd::cargo::cargo_bin_cmd;
use async_trait::async_trait;
use tddy_connectrpc::connect_router;
use tddy_rpc::{
    MultiRpcService, Request, Response, RpcBridge, RpcService, ServiceEntry, Status, Streaming,
};
use tddy_service::proto::session::{
    ConnectSessionRequest, ConnectSessionResponse, DeleteSessionRequest, DeleteSessionResponse,
    EnsureProjectHomeSessionRequest, EnsureProjectHomeSessionResponse, GetWorktreeSnapshotRequest,
    GetWorktreeSnapshotResponse, ListSessionsRequest, ListSessionsResponse, ResumeSessionRequest,
    ResumeSessionResponse, SessionService, SessionServiceServer, SignalSessionRequest,
    SignalSessionResponse, StartSessionEvent, StartSessionRequest, StartSessionResponse,
};

/// The session id the fixture answers every start with, so the CLI's printed output can be checked.
const STARTED_SESSION_ID: &str = "sess-wire-1";

/// The `session.SessionService` the daemon serves, reduced to what a start needs. It records every
/// start request the generated handler decoded and answers with a session id. Every other method
/// refuses, because no test in this file reaches it.
#[derive(Default)]
struct StartRecordingSessionService {
    started: Mutex<Vec<StartSessionRequest>>,
}

#[async_trait]
impl SessionService for StartRecordingSessionService {
    type StreamStartSessionStream = Streaming<StartSessionEvent>;

    async fn list_sessions(
        &self,
        _request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        self.started
            .lock()
            .expect("recorded start requests")
            .push(request.into_inner());
        Ok(Response::new(StartSessionResponse {
            session_id: STARTED_SESSION_ID.to_string(),
            ..Default::default()
        }))
    }

    async fn stream_start_session(
        &self,
        _request: Request<StartSessionRequest>,
    ) -> Result<Response<Self::StreamStartSessionStream>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn connect_session(
        &self,
        _request: Request<ConnectSessionRequest>,
    ) -> Result<Response<ConnectSessionResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn resume_session(
        &self,
        _request: Request<ResumeSessionRequest>,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn ensure_project_home_session(
        &self,
        _request: Request<EnsureProjectHomeSessionRequest>,
    ) -> Result<Response<EnsureProjectHomeSessionResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn signal_session(
        &self,
        _request: Request<SignalSessionRequest>,
    ) -> Result<Response<SignalSessionResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn delete_session(
        &self,
        _request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }

    async fn get_worktree_snapshot(
        &self,
        _request: Request<GetWorktreeSnapshotRequest>,
    ) -> Result<Response<GetWorktreeSnapshotResponse>, Status> {
        Err(Status::unimplemented("not served by this fixture"))
    }
}

/// The daemon's `/rpc` surface on a real loopback port, mounted the way the daemon mounts it:
/// `connect_router` over an `RpcBridge` over a `MultiRpcService`. Only `session.SessionService` is
/// registered, so a request addressed to any other service is refused.
struct ASessionEndpoint {
    port: u16,
    service: Arc<StartRecordingSessionService>,
    _serving: tokio::task::JoinHandle<()>,
}

impl ASessionEndpoint {
    async fn start() -> Self {
        let service = Arc::new(StartRecordingSessionService::default());
        let router = connect_router(RpcBridge::new(MultiRpcService::new(vec![ServiceEntry {
            name: "session.SessionService",
            service: Arc::new(SessionServiceServer::from_arc(Arc::clone(&service)))
                as Arc<dyn RpcService>,
        }])));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the session endpoint");
        let port = listener
            .local_addr()
            .expect("session endpoint local addr")
            .port();
        let serving = tokio::spawn(async move {
            // Not swallowed: a harness that failed to serve records no start request, which would
            // be reported as a wire bug.
            axum::serve(listener, router)
                .await
                .expect("the session endpoint served");
        });
        Self {
            port,
            service,
            _serving: serving,
        }
    }

    /// The start requests the generated handler decoded and handed to the service, in arrival order.
    fn decoded_starts(&self) -> Vec<StartSessionRequest> {
        self.service
            .started
            .lock()
            .expect("recorded start requests")
            .clone()
    }
}

/// A relay discovery directory whose `daemon.json` points the CLI at `port`.
fn relay_discovery_at(port: u16) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("relay discovery dir");
    std::fs::write(
        dir.path().join("daemon.json"),
        serde_json::json!({ "port": port, "pid": 0, "started_at": 0 }).to_string(),
    )
    .expect("write daemon.json");
    dir
}

/// Runs `tddy-tools remote start-session` against the relay in `relay`, with one `--agent` per
/// entry of `agents`. Blocking, so it runs off the reactor: the endpoint under test is served by
/// this same runtime.
async fn run_start_session(relay: &tempfile::TempDir, agents: &[&str]) -> std::process::Output {
    let relay_dir = relay.path().to_path_buf();
    let agents: Vec<String> = agents.iter().map(|agent| agent.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        let mut cmd = cargo_bin_cmd!("tddy-tools");
        cmd.args(["remote", "start-session", "--base-dir"])
            .arg(&relay_dir)
            .args(["--session-token", "test-token"]);
        for agent in &agents {
            cmd.args(["--agent", agent]);
        }
        cmd.output().expect("tddy-tools remote start-session runs")
    })
    .await
    .expect("the start-session subprocess ran to completion")
}

#[tokio::test]
async fn remote_start_session_exits_zero_when_the_daemon_service_accepts_it() {
    // Given a daemon serving session.SessionService on a real port
    let endpoint = ASessionEndpoint::start().await;
    let relay = relay_discovery_at(endpoint.port);

    // When the CLI starts a workspace session with one agent
    let output = run_start_session(&relay, &["codex-fast@codebase-host"]).await;

    // Then the call succeeds
    assert!(
        output.status.success(),
        "remote start-session must exit 0 against the daemon; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn remote_start_session_reaches_the_service_as_one_decoded_start_request() {
    // Given a daemon serving session.SessionService on a real port
    let endpoint = ASessionEndpoint::start().await;
    let relay = relay_discovery_at(endpoint.port);

    // When the CLI starts a workspace session with one agent
    let output = run_start_session(&relay, &["codex-fast@codebase-host"]).await;

    // Then the generated handler decoded exactly one start request for the service
    let starts = endpoint.decoded_starts();
    assert_eq!(
        starts.len(),
        1,
        "the daemon service must decode one start request; cli stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn remote_start_session_sends_each_agent_as_a_specialized_agent() {
    // Given a daemon serving session.SessionService on a real port
    let endpoint = ASessionEndpoint::start().await;
    let relay = relay_discovery_at(endpoint.port);

    // When the CLI starts a session with two agents
    let output = run_start_session(
        &relay,
        &["codex-fast@codebase-host", "claude-coder@codebase-host"],
    )
    .await;

    // Then the decoded request carries both agents as specialized_agents, in the order given
    let starts = endpoint.decoded_starts();
    let start = starts.first().unwrap_or_else(|| {
        panic!(
            "the daemon service must decode a start request; cli stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(
        start.specialized_agents,
        vec![
            "codex-fast@codebase-host".to_string(),
            "claude-coder@codebase-host".to_string(),
        ]
    );
}

#[tokio::test]
async fn remote_start_session_requests_a_workspace_session() {
    // Given a daemon serving session.SessionService on a real port
    let endpoint = ASessionEndpoint::start().await;
    let relay = relay_discovery_at(endpoint.port);

    // When the CLI starts a session
    let output = run_start_session(&relay, &["codex-fast@codebase-host"]).await;

    // Then the decoded request is a workspace session
    let starts = endpoint.decoded_starts();
    let start = starts.first().unwrap_or_else(|| {
        panic!(
            "the daemon service must decode a start request; cli stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(start.session_type, "workspace");
}

#[tokio::test]
async fn remote_start_session_prints_the_session_id_the_daemon_returned() {
    // Given a daemon that answers a start with a session id
    let endpoint = ASessionEndpoint::start().await;
    let relay = relay_discovery_at(endpoint.port);

    // When the CLI starts a session
    let output = run_start_session(&relay, &["codex-fast@codebase-host"]).await;

    // Then stdout carries the session id the daemon returned
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(STARTED_SESSION_ID),
        "remote start-session must print the returned session id {STARTED_SESSION_ID}; \
         stdout: {stdout}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

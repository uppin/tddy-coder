//! Every RPC family, reached over the local socket **`runtime::build` itself assembled**.
//!
//! `local_socket_reachability_acceptance.rs` reads `local_socket_server.rs` as text, and
//! `local_token_uds.rs` mounts services it builds for itself. Neither can see what `runtime.rs`
//! actually hands the socket, so swapping which type serves a family there — the whole of
//! `#carve` 11's rewiring — could drop a family from the socket with every other gate green. This
//! suite dials the real thing: the binary runtime, built and started, on a socket in a temp dir.
//!
//! One call per family, answered by that family's handler rather than by the transport's
//! `Unimplemented`. The calls carry a token no daemon issued, so each handler's own first check is
//! what answers — which is exactly what proves the handler is mounted, without depending on any
//! session or project existing.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hyper_util::rt::TokioIo;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, RuntimeOptions, RuntimeTaskHandles};
use tddy_service::proto::tonic_catalog::catalog_service_client::CatalogServiceClient;
use tddy_service::proto::tonic_exec_tools::exec_tool_service_client::ExecToolServiceClient;
use tddy_service::proto::tonic_pr_stack::pr_stack_service_client::PrStackServiceClient;
use tddy_service::proto::tonic_project::project_service_client::ProjectServiceClient;
use tddy_service::proto::tonic_session::session_service_client::SessionServiceClient;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tonic::transport::{Channel, Endpoint};
use tonic::Code;

const A_TOKEN_NO_DAEMON_ISSUED: &str = "a-token-no-daemon-issued";

/// How long the runtime is given to bind its socket before the test fails rather than hangs.
const THE_SOCKET_IS_BOUND_WITHIN: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------------------------
// Given: the binary daemon, built and started
// ---------------------------------------------------------------------------------------------

/// A running binary daemon, and the temp home its socket and state live in.
struct ARunningDaemon {
    socket_path: PathBuf,
    tasks: RuntimeTaskHandles,
    _home: TempDir,
}

impl Drop for ARunningDaemon {
    fn drop(&mut self) {
        self.tasks.abort_all();
    }
}

async fn a_free_tcp_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("no loopback port was available");
    listener
        .local_addr()
        .expect("the bound listener has no address")
        .port()
}

/// GitHub auth is stubbed because every token-authenticated family is registered only when the
/// runtime has a user resolver; without one this suite would find nothing mounted and prove nothing.
fn a_binary_daemon_config(web_port: u16, home: &Path, socket_path: &Path) -> DaemonConfig {
    let yaml = format!(
        r#"
listen:
  web_port: {web_port}
  web_host: 127.0.0.1
tddy_data_dir: {data_dir}
github:
  stub: true
local:
  socket_path: {socket_path}
"#,
        data_dir = home.display(),
        socket_path = socket_path.display(),
    );
    serde_yaml::from_str(&yaml).expect("the config fixture did not parse")
}

async fn a_running_binary_daemon() -> ARunningDaemon {
    let home = tempfile::tempdir().expect("a temp tddy home");
    let socket_path = home.path().join("tddy-daemon.sock");
    let config = a_binary_daemon_config(a_free_tcp_port().await, home.path(), &socket_path);

    let runtime = runtime::build(config, RuntimeOptions::for_binary())
        .await
        .expect("the binary runtime did not build");
    let tasks = runtime.tasks.spawn();

    ARunningDaemon {
        socket_path,
        tasks,
        _home: home,
    }
}

async fn a_channel_to(socket_path: &Path) -> Channel {
    let deadline = Instant::now() + THE_SOCKET_IS_BOUND_WITHIN;
    while !socket_path.exists() {
        assert!(
            Instant::now() < deadline,
            "the runtime never bound its local socket at {}",
            socket_path.display()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let path = socket_path.to_path_buf();
    Endpoint::try_from("http://127.0.0.1:50051")
        .expect("an endpoint")
        .connect_with_connector(tower::service_fn(move |_| {
            let path = path.clone();
            async move {
                let stream = tokio::net::UnixStream::connect(&path).await?;
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await
        .expect("connect over the local socket")
}

// ---------------------------------------------------------------------------------------------
// Then: what each family answered
// ---------------------------------------------------------------------------------------------

/// The code a family answered with — `Ok` included — so one table can say what each should be.
fn answered<T>(result: Result<tonic::Response<T>, tonic::Status>) -> Code {
    match result {
        Ok(_) => Code::Ok,
        Err(status) => status.code(),
    }
}

#[tokio::test]
async fn the_assembled_daemon_answers_every_family_on_its_local_socket() {
    // Given the binary daemon, as `runtime::build` assembles it
    let daemon = a_running_binary_daemon().await;
    let channel = a_channel_to(&daemon.socket_path).await;

    // When one method of each family is called
    let session = answered(
        SessionServiceClient::new(channel.clone())
            .list_sessions(tddy_service::proto::session::ListSessionsRequest {
                session_token: A_TOKEN_NO_DAEMON_ISSUED.to_string(),
            })
            .await,
    );
    let project = answered(
        ProjectServiceClient::new(channel.clone())
            .list_projects(tddy_service::proto::project::ListProjectsRequest {
                session_token: A_TOKEN_NO_DAEMON_ISSUED.to_string(),
                local_only: true,
            })
            .await,
    );
    let catalog = answered(
        CatalogServiceClient::new(channel.clone())
            .list_tools(tddy_service::proto::catalog::ListToolsRequest {})
            .await,
    );
    let exec_tools = answered(
        ExecToolServiceClient::new(channel.clone())
            .execute_tool(tddy_service::proto::exec_tools::ExecuteToolRequest {
                session_token: A_TOKEN_NO_DAEMON_ISSUED.to_string(),
                session_id: "a-session".to_string(),
                tool_name: "Read".to_string(),
                args_json: "{}".to_string(),
                ..Default::default()
            })
            .await,
    );
    let pr_stack = answered(
        PrStackServiceClient::new(channel.clone())
            .query_branch(tddy_service::proto::pr_stack::QueryBranchRequest {
                session_token: A_TOKEN_NO_DAEMON_ISSUED.to_string(),
                branch: "feature/widgets".to_string(),
                ..Default::default()
            })
            .await,
    );

    // Then each was answered by its own handler: the catalogue needs no caller, and every other
    // family refuses a caller it cannot verify — none falls through to `Unimplemented`
    assert_eq!(
        [
            ("session", session),
            ("project", project),
            ("catalog", catalog),
            ("exec_tools", exec_tools),
            ("pr_stack", pr_stack),
        ],
        [
            ("session", Code::Unauthenticated),
            ("project", Code::Unauthenticated),
            ("catalog", Code::Ok),
            ("exec_tools", Code::Unauthenticated),
            ("pr_stack", Code::Unauthenticated),
        ]
    );
}

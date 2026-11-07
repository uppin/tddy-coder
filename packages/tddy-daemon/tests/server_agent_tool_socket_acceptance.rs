//! A **server** daemon — the `tddy-daemon` binary — serves an agent-tool socket **per OS user**
//! (`#agent-tools` 26/28, design A).
//!
//! A host Cursor session's `.cursor/hooks.json` names a `--daemon-socket` and has no HTTP
//! fallback, and a co-located Claude session's hooks, notify and tool calls go over the same
//! socket. Before this change only an embedded daemon served one, so on a server every such report
//! went to a path nobody served. One socket for the daemon's own user would not do either: under
//! `tddy-supervisor`, sessions run as other OS users and would get EACCES. So each session's user
//! gets `<data dir>/run-agent-tools/<user>/agent.sock`, owned by them, `0600` in a `0700` directory.
//!
//! Re-applies the three tests held on `wip/agent-tools-cursor-cli-server-socket` (`07128a63e`),
//! moved from the one digest-named socket to the session user's.
//!
//! Every test dials the real thing: the binary runtime as `runtime::build` assembles it, started,
//! with its state in a temp dir, asked for the socket of the account running the test — the one OS
//! user a test can be served as without privilege. That a *second* user cannot connect to it is a
//! file-mode property checked here by mode and owner; connecting as that user needs root and a real
//! account, which only the Linux privileged suites have.
//!
//! Changeset: docs/dev/changesets/2026-11-07-agent-tool-socket-per-os-user.md

use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use prost::Message as _;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, DaemonRuntime, RuntimeOptions, RuntimeTaskHandles};
use tddy_rpc::{Code, MultiRpcService, RequestTransport, RpcClientTransport};
use tddy_service::proto::activity::{ReportSessionStatusRequest, ReportSessionStatusResponse};
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_testing_commons::wait::eventually_awaiting;
use tempfile::TempDir;
use tokio::net::TcpListener;

const A_CURSOR_SESSION: &str = "a-host-cursor-session";
const ITS_HOOK_TOKEN: &str = "the-hook-token-baked-into-hooks-json";
const A_TOKEN_NO_DAEMON_ISSUED: &str = "a-token-no-daemon-issued";
const EXEC_TOOL_SERVICE: &str = "exec_tools.ExecToolService";

/// How long the daemon is given to answer on a user's socket before the test fails rather than
/// hangs.
const THE_SOCKET_ANSWERS_WITHIN: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------------------------
// Given: the binary daemon, built and started
// ---------------------------------------------------------------------------------------------

/// A running binary daemon, and the temp dir its state lives in.
struct ARunningServerDaemon {
    data_dir: PathBuf,
    session_host: Arc<tddy_session_lifecycle::connection_service::DaemonSessionHost>,
    tasks: RuntimeTaskHandles,
    _home: TempDir,
}

impl Drop for ARunningServerDaemon {
    fn drop(&mut self) {
        self.tasks.abort_all();
    }
}

fn this_os_user() -> String {
    // SAFETY: `getpwuid` points into static storage that is read at once.
    unsafe {
        let entry = libc::getpwuid(libc::geteuid());
        assert!(!entry.is_null(), "the test account has a passwd entry");
        std::ffi::CStr::from_ptr((*entry).pw_name)
            .to_string_lossy()
            .into_owned()
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
/// runtime has a user resolver. The local socket is pinned inside the temp dir so the server binds
/// nothing outside it.
fn a_server_daemon_config(web_port: u16, home: &Path) -> DaemonConfig {
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
        socket_path = home.join("tddy-daemon.sock").display(),
    );
    serde_yaml::from_str(&yaml).expect("the config fixture did not parse")
}

async fn a_running_server_daemon() -> ARunningServerDaemon {
    let home = tempfile::tempdir().expect("a temp tddy home");
    let config = a_server_daemon_config(a_free_tcp_port().await, home.path());

    let runtime: DaemonRuntime = runtime::build(
        config,
        RuntimeOptions::for_binary(tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker),
    )
    .await
    .expect("the binary runtime did not build");
    let session_host = runtime
        .session_host
        .clone()
        .expect("an authenticated server hosts sessions");
    let tasks = runtime.tasks.spawn();

    ARunningServerDaemon {
        data_dir: home.path().to_path_buf(),
        session_host,
        tasks,
        _home: home,
    }
}

impl ARunningServerDaemon {
    /// Where a session of `os_user` is told this daemon listens — as its start asks.
    async fn agent_tool_socket_of(&self, os_user: &str) -> Result<PathBuf, String> {
        self.session_host
            .agent_tool_sockets()
            .ensure_bound(os_user, &self.data_dir)
            .await
            .map_err(|e| format!("{e:#}"))
    }

    /// A host `cursor-cli` session's `.session.yaml`, holding the hook token its `hooks.json` was
    /// rendered with — what a host Cursor start leaves on disk.
    fn with_a_host_cursor_session(&self) {
        let session_dir = self.data_dir.join("sessions").join(A_CURSOR_SESSION);
        std::fs::create_dir_all(&session_dir).expect("the session dir is created");
        std::fs::write(
            session_dir.join(".session.yaml"),
            format!(
                "session_id: {A_CURSOR_SESSION}\n\
                 project_id: a-project\n\
                 created_at: 2026-11-03T00:00:00Z\n\
                 updated_at: 2026-11-03T00:00:00Z\n\
                 status: active\n\
                 session_type: cursor-cli\n\
                 hook_token: {ITS_HOOK_TOKEN}\n"
            ),
        )
        .expect("the session metadata is written");
    }

    /// The activity status the session's metadata records.
    fn activity_status_of_the_cursor_session(&self) -> Option<String> {
        tddy_core::read_session_metadata(&self.data_dir.join("sessions").join(A_CURSOR_SESSION))
            .expect("the session metadata reads back")
            .activity_status
    }
}

/// Connect to `socket` as a co-located agent process does — once the daemon is accepting on it.
async fn an_agent_on(socket: &Path) -> Arc<dyn RpcClientTransport> {
    eventually_awaiting(
        "the user's agent-tool socket accepts",
        THE_SOCKET_ANSWERS_WITHIN,
        || {
            let socket = socket.to_path_buf();
            async move {
                let stream = tokio::net::UnixStream::connect(&socket)
                    .await
                    .map_err(|e| format!("connect {}: {e}", socket.display()))?;
                let (reader, writer) = tokio::io::split(stream);
                // The agent hosts nothing for the daemon to call back.
                let (client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                    reader,
                    writer,
                    MultiRpcService::new(Vec::new()),
                    RequestTransport::UnixSocket,
                );
                tokio::spawn(endpoint.run());
                Ok(client as Arc<dyn RpcClientTransport>)
            }
        },
    )
    .await
}

// ---------------------------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------------------------

/// The `beforeSubmitPrompt` report `tddy-tools session-hook` sends for a Cursor session.
async fn the_cursor_hook_reports_running(
    agent: &dyn RpcClientTransport,
) -> Result<ReportSessionStatusResponse, tddy_rpc::Status> {
    let request = ReportSessionStatusRequest {
        session_id: A_CURSOR_SESSION.to_string(),
        status: "Running".to_string(),
        os_user: this_os_user(),
        hook_token: ITS_HOOK_TOKEN.to_string(),
        ..Default::default()
    };
    let answer = tokio::time::timeout(
        THE_SOCKET_ANSWERS_WITHIN,
        agent.call_unary(
            tddy_service::session_activity::ACTIVITY_SERVICE,
            "ReportSessionStatus",
            request.encode_to_vec(),
        ),
    )
    .await
    .map_err(|_| tddy_rpc::Status::deadline_exceeded("the hook report was not answered"))??;
    Ok(ReportSessionStatusResponse::decode(&answer[..]).expect("the response decodes"))
}

// ---------------------------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_server_daemon_tells_a_session_its_users_own_agent_tool_socket() {
    // Given a server daemon, serving
    let daemon = a_running_server_daemon().await;
    let user = this_os_user();

    // When a session running as this user asks where its hooks and tools should reach it
    let socket = daemon.agent_tool_socket_of(&user).await;

    // Then it is told its user's socket under the data dir, not one shared by every user
    assert_eq!(
        socket,
        Ok(daemon
            .data_dir
            .join("run-agent-tools")
            .join(&user)
            .join("agent.sock"))
    );
}

#[tokio::test]
async fn a_session_running_as_x_is_served_on_xs_owner_only_agent_tool_socket() {
    // Given a server daemon, and the socket of the user a session runs as
    let daemon = a_running_server_daemon().await;
    let socket = daemon
        .agent_tool_socket_of(&this_os_user())
        .await
        .expect("this user's socket is bound");

    // When the session's agent connects
    let _agent = an_agent_on(&socket).await;

    // Then it was served — and the socket is closed to every other user: 0600, in a 0700
    // directory, both owned by this user
    let file = std::fs::symlink_metadata(&socket).expect("stat the socket");
    let directory = std::fs::metadata(socket.parent().expect("a directory")).expect("stat");
    // SAFETY: reads this process's own credentials.
    let me = unsafe { libc::geteuid() };
    assert_eq!(
        (
            file.file_type().is_socket(),
            file.permissions().mode() & 0o7777,
            directory.permissions().mode() & 0o7777,
            file.uid(),
            directory.uid(),
        ),
        (true, 0o600, 0o700, me, me)
    );
}

#[tokio::test]
async fn a_host_cursor_sessions_hook_report_reaches_a_server_daemon_over_its_users_agent_tool_socket(
) {
    // Given a server daemon holding a host Cursor session
    let daemon = a_running_server_daemon().await;
    daemon.with_a_host_cursor_session();
    let socket = daemon
        .agent_tool_socket_of(&this_os_user())
        .await
        .expect("this user's socket is bound");

    // When the session's hook reports over the socket its `hooks.json` names
    let agent = an_agent_on(&socket).await;
    let answered = the_cursor_hook_reports_running(agent.as_ref())
        .await
        .map(|response| response.ok)
        .map_err(|status| format!("{:?}: {}", status.code, status.message));

    // Then the daemon accepted it and recorded the status
    assert_eq!(
        (answered, daemon.activity_status_of_the_cursor_session()),
        (Ok(true), Some("Running".to_string()))
    );
}

#[tokio::test]
async fn a_tool_call_over_a_users_agent_tool_socket_reaches_the_server_daemons_exec_tool_handler() {
    // Given a server daemon, serving this user's socket
    let daemon = a_running_server_daemon().await;
    let socket = daemon
        .agent_tool_socket_of(&this_os_user())
        .await
        .expect("this user's socket is bound");
    let agent = an_agent_on(&socket).await;

    // When a co-located agent calls a tool over it, with a token no daemon issued
    let request = ExecuteToolRequest {
        session_token: A_TOKEN_NO_DAEMON_ISSUED.to_string(),
        session_id: "a-session".to_string(),
        tool_name: "Read".to_string(),
        args_json: "{}".to_string(),
        ..Default::default()
    };
    let answered = tokio::time::timeout(
        THE_SOCKET_ANSWERS_WITHIN,
        agent.call_unary(EXEC_TOOL_SERVICE, "ExecuteTool", request.encode_to_vec()),
    )
    .await
    .expect("the tool call was answered in time")
    .map(|bytes| ExecuteToolResponse::decode(&bytes[..]).expect("the response decodes"))
    .map_err(|status| status.code);

    // Then the exec-tool handler itself refused the caller — not the transport, with
    // `Unimplemented`
    assert_eq!(answered.err(), Some(Code::Unauthenticated));
}

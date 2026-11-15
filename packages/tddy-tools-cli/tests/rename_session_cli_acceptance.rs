//! `tddy-tools rename-session`: an agent naming the session it runs in, run as the agent runs it —
//! a process whose environment names its session, against a daemon's agent-tool socket.
//!
//! Feature doc: docs/ft/web/session-drawer.md § Session names. The exit code and what the command
//! prints are the agent's whole interface, so this suite runs the real binary and asserts on
//! exactly those, and on the request the daemon was sent.

use std::sync::{Arc, Mutex};

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use prost::Message as _;
use tddy_rpc::{MultiRpcService, RpcMessage, RpcResult, RpcService, ServiceEntry, Status};
use tddy_service::proto::activity::{RenameOwnSessionRequest, RenameOwnSessionResponse};
use tddy_service::session_activity::notify_env;

const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000501";
const OS_USER: &str = "ada";
const HOOK_TOKEN: &str = "tok-rename-cli";
const NAME: &str = "Fix the login flow";

/// What the daemon says when it refuses a name — the reason the agent must be shown.
const DAEMONS_REASON: &str = "a session name is at most 80 characters; this one has 81";

type RecordedCalls = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// How the fake daemon answers `RenameOwnSession`.
#[derive(Clone)]
enum Answer {
    /// Records the name, answering it trimmed as the daemon does.
    Records,
    Refuses,
}

struct AnActivityServiceThat {
    answer: Answer,
    calls: RecordedCalls,
}

#[async_trait::async_trait]
impl RpcService for AnActivityServiceThat {
    async fn handle_rpc(&self, _service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        self.calls
            .lock()
            .expect("recorded calls")
            .push((method.to_string(), message.payload.clone()));
        RpcResult::Unary(match self.answer {
            Answer::Records => {
                let request = RenameOwnSessionRequest::decode(message.payload.as_slice())
                    .map_err(|e| Status::invalid_argument(e.to_string()));
                request.map(|request| {
                    RenameOwnSessionResponse {
                        display_name: request.display_name.trim().to_string(),
                    }
                    .encode_to_vec()
                })
            }
            Answer::Refuses => Err(Status::invalid_argument(DAEMONS_REASON)),
        })
    }
}

/// A daemon's agent-tool socket serving only `activity.ActivityService`.
struct AnAgentToolSocket {
    path: std::path::PathBuf,
    calls: RecordedCalls,
    serving: tokio::task::JoinHandle<()>,
}

impl AnAgentToolSocket {
    fn that(answer: Answer, tag: &str) -> Self {
        let calls: RecordedCalls = Arc::new(Mutex::new(Vec::new()));
        let path = tddy_sandbox::SandboxSpec::short_ipc_socket_path(tag);
        let _ = std::fs::remove_file(&path);
        let listener = tokio::net::UnixListener::bind(&path).expect("bind the agent-tool socket");
        let served = Arc::clone(&calls);
        let serving = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let calls = Arc::clone(&served);
                let answer = answer.clone();
                tokio::spawn(async move {
                    let (read_half, write_half) = tokio::io::split(stream);
                    let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                        read_half,
                        write_half,
                        MultiRpcService::new(vec![ServiceEntry {
                            name: tddy_service::session_activity::ACTIVITY_SERVICE,
                            service: Arc::new(AnActivityServiceThat { answer, calls })
                                as Arc<dyn RpcService>,
                        }]),
                        tddy_rpc::RequestTransport::UnixSocket,
                    );
                    endpoint.run().await;
                });
            }
        });
        Self {
            path,
            calls,
            serving,
        }
    }

    /// The `RenameOwnSession` the socket was sent, decoded; `None` when it was sent none.
    fn rename_received(&self) -> Option<RenameOwnSessionRequest> {
        self.calls
            .lock()
            .expect("recorded calls")
            .iter()
            .find(|(method, _)| method == "RenameOwnSession")
            .map(|(_, body)| {
                RenameOwnSessionRequest::decode(body.as_slice()).expect("the request decodes")
            })
    }
}

impl Drop for AnAgentToolSocket {
    fn drop(&mut self) {
        self.serving.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// `tddy-tools` with none of the session environment inherited from whoever runs this suite, run
/// from the system temp directory so no session pointer (`.tddy/session.json`) above the checkout
/// is adopted in place of the environment each test sets — a checkout that is itself a tddy
/// session's worktree has one, and the command would reach that session's real daemon.
fn tddy_tools_without_a_session_environment() -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.current_dir(std::env::temp_dir())
        .env_remove("TDDY_SOCKET")
        .env_remove(notify_env::SESSION_ID)
        .env_remove(notify_env::OS_USER)
        .env_remove(notify_env::HOOK_TOKEN)
        .env_remove(notify_env::DAEMON_URL)
        .env_remove(notify_env::DAEMON_SOCKET);
    cmd
}

fn tddy_tools_in_a_session_reaching(socket: &AnAgentToolSocket) -> Command {
    let mut cmd = tddy_tools_without_a_session_environment();
    cmd.env(notify_env::SESSION_ID, SESSION_ID)
        .env(notify_env::OS_USER, OS_USER)
        .env(notify_env::HOOK_TOKEN, HOOK_TOKEN)
        .env(notify_env::DAEMON_SOCKET, &socket.path);
    cmd
}

struct Ran {
    succeeded: bool,
    stdout: String,
    stderr: String,
}

async fn run(mut cmd: Command, args: &[&str]) -> Ran {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let output = tokio::task::spawn_blocking(move || {
        cmd.args(&args)
            .timeout(std::time::Duration::from_secs(10))
            .output()
            .expect("the rename-session subprocess ran")
    })
    .await
    .expect("the rename-session subprocess ran to completion");
    Ran {
        succeeded: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

impl Ran {
    fn assert_succeeded_saying(&self, line: &str) {
        assert!(
            self.succeeded,
            "expected rename-session to succeed; stderr: {}",
            self.stderr
        );
        assert_eq!(self.stdout.trim_end(), line);
    }

    fn assert_failed_naming(&self, reason: &str) {
        assert!(
            !self.succeeded,
            "expected rename-session to exit non-zero; stdout: {}",
            self.stdout
        );
        assert!(
            self.stderr.contains(reason),
            "expected the failure to name {reason:?}; stderr: {}",
            self.stderr
        );
    }
}

#[tokio::test]
async fn an_agent_names_its_session_under_the_credentials_its_environment_holds() {
    // Given a daemon's agent-tool socket that records names
    let socket = AnAgentToolSocket::that(Answer::Records, "renameok");

    // When the agent names its session
    let ran = run(
        tddy_tools_in_a_session_reaching(&socket),
        &["rename-session", &format!(" {NAME} ")],
    )
    .await;

    // Then it prints the name as recorded, and the daemon was asked under the session's credentials
    ran.assert_succeeded_saying(&format!("Session renamed: {NAME}"));
    assert_eq!(
        socket.rename_received(),
        Some(RenameOwnSessionRequest {
            session_id: SESSION_ID.to_string(),
            hook_token: HOOK_TOKEN.to_string(),
            os_user: OS_USER.to_string(),
            display_name: format!(" {NAME} "),
        })
    );
}

#[tokio::test]
async fn an_empty_name_clears_the_sessions_name() {
    // Given
    let socket = AnAgentToolSocket::that(Answer::Records, "renameclr");

    // When the agent names its session with nothing
    let ran = run(
        tddy_tools_in_a_session_reaching(&socket),
        &["rename-session", ""],
    )
    .await;

    // Then it says the name was cleared
    ran.assert_succeeded_saying("Session name cleared");
    assert_eq!(
        socket.rename_received().map(|request| request.display_name),
        Some(String::new())
    );
}

#[tokio::test]
async fn a_refused_name_exits_non_zero_naming_the_daemons_reason() {
    // Given a daemon that refuses the name
    let socket = AnAgentToolSocket::that(Answer::Refuses, "renameno");

    // When the agent names its session
    let ran = run(
        tddy_tools_in_a_session_reaching(&socket),
        &["rename-session", &"x".repeat(81)],
    )
    .await;

    // Then it fails, saying why
    ran.assert_failed_naming(DAEMONS_REASON);
}

#[tokio::test]
async fn outside_a_daemon_launched_session_it_exits_non_zero_naming_what_is_missing() {
    // When the command runs with no session environment and no session pointer
    let cwd = tempfile::tempdir().unwrap();
    let mut cmd = tddy_tools_without_a_session_environment();
    cmd.current_dir(cwd.path());
    let ran = run(cmd, &["rename-session", NAME]).await;

    // Then it fails naming the variable it needs
    ran.assert_failed_naming(notify_env::SESSION_ID);
}

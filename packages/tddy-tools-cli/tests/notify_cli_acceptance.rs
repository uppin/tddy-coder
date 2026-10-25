//! `tddy-tools notify`: an agent notifying its operator, run as the agent runs it — a process whose
//! environment names its session, against a daemon's agent-tool socket.
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-25-agent-notifications.md` (`#home-inbox` 1/8). The exit
//! code and what the command prints are the agent's whole interface, so this suite runs the real
//! binary and asserts on exactly those. Unlike `session-hook` it is **not** fail-quiet: an agent
//! that asked to notify must learn when it did not.

use std::sync::{Arc, Mutex};

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use prost::Message as _;
use tddy_rpc::{MultiRpcService, RpcMessage, RpcResult, RpcService, ServiceEntry, Status};
use tddy_service::proto::activity::{
    NotificationLevel, ReportOperatorNotificationRequest, ReportOperatorNotificationResponse,
};
use tddy_service::session_activity::notify_env;

const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000401";
const OS_USER: &str = "ada";
const HOOK_TOKEN: &str = "tok-notify-cli";

/// What the daemon says when it refuses a notification — the reason the agent must be shown.
const DAEMONS_REASON: &str = "title is longer than 200 characters";

// ---------------------------------------------------------------------------
// A daemon's agent-tool socket
// ---------------------------------------------------------------------------

/// Every call the socket answered: the method name and the request body it carried.
type RecordedCalls = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// How the fake daemon answers `ReportOperatorNotification`.
#[derive(Clone, Copy)]
enum Answer {
    Accepts,
    RefusesTheTitle,
}

/// An `activity.ActivityService` that answers `ReportOperatorNotification` as told and keeps every
/// request body it was sent.
struct AnActivityServiceThat {
    answer: Answer,
    bodies: RecordedCalls,
}

#[async_trait::async_trait]
impl RpcService for AnActivityServiceThat {
    async fn handle_rpc(&self, _service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        self.bodies
            .lock()
            .expect("recorded bodies")
            .push((method.to_string(), message.payload.clone()));
        RpcResult::Unary(match self.answer {
            Answer::Accepts => Ok(ReportOperatorNotificationResponse { ok: true }.encode_to_vec()),
            Answer::RefusesTheTitle => Err(Status::invalid_argument(DAEMONS_REASON)),
        })
    }
}

/// A daemon's agent-tool socket serving only `activity.ActivityService` — the channel an embedded
/// daemon's agents reach it by, and the one `TDDY_DAEMON_SOCKET` names. Dropping it stops serving
/// and removes the socket.
struct AnAgentToolSocket {
    path: std::path::PathBuf,
    bodies: RecordedCalls,
    serving: tokio::task::JoinHandle<()>,
}

impl AnAgentToolSocket {
    fn that(answer: Answer, tag: &str) -> Self {
        let bodies: RecordedCalls = Arc::new(Mutex::new(Vec::new()));
        let path = tddy_sandbox::SandboxSpec::short_ipc_socket_path(tag);
        let _ = std::fs::remove_file(&path);
        let listener = tokio::net::UnixListener::bind(&path).expect("bind the agent-tool socket");
        let served = Arc::clone(&bodies);
        let serving = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let bodies = Arc::clone(&served);
                tokio::spawn(async move {
                    let (read_half, write_half) = tokio::io::split(stream);
                    let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                        read_half,
                        write_half,
                        MultiRpcService::new(vec![ServiceEntry {
                            name: tddy_service::session_activity::ACTIVITY_SERVICE,
                            service: Arc::new(AnActivityServiceThat { answer, bodies })
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
            bodies,
            serving,
        }
    }

    fn accepting(tag: &str) -> Self {
        Self::that(Answer::Accepts, tag)
    }

    fn refusing_the_title(tag: &str) -> Self {
        Self::that(Answer::RefusesTheTitle, tag)
    }

    /// The `ReportOperatorNotification` the socket was sent, decoded; `None` when it was sent none.
    fn notification_received(&self) -> Option<ReportOperatorNotificationRequest> {
        self.bodies
            .lock()
            .expect("recorded bodies")
            .iter()
            .find(|(method, _)| method == "ReportOperatorNotification")
            .map(|(_, body)| {
                ReportOperatorNotificationRequest::decode(body.as_slice())
                    .expect("the request body decodes")
            })
    }

    fn methods_called(&self) -> Vec<String> {
        self.bodies
            .lock()
            .expect("recorded bodies")
            .iter()
            .map(|(method, _)| method.clone())
            .collect()
    }
}

impl Drop for AnAgentToolSocket {
    fn drop(&mut self) {
        self.serving.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

// ---------------------------------------------------------------------------
// The agent's process
// ---------------------------------------------------------------------------

/// `tddy-tools` with none of the session environment inherited from whoever runs this suite, so
/// each test sets exactly the environment it is about.
fn tddy_tools_without_a_session_environment() -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET")
        .env_remove(notify_env::SESSION_ID)
        .env_remove(notify_env::OS_USER)
        .env_remove(notify_env::HOOK_TOKEN)
        .env_remove(notify_env::DAEMON_URL)
        .env_remove(notify_env::DAEMON_SOCKET);
    cmd
}

/// `tddy-tools` as a daemon-launched agent on an embedded host runs it: the session's credentials
/// in its environment, and the daemon named by its agent-tool socket.
fn tddy_tools_in_a_session_reaching(socket: &AnAgentToolSocket) -> Command {
    let mut cmd = tddy_tools_without_a_session_environment();
    cmd.env(notify_env::SESSION_ID, SESSION_ID)
        .env(notify_env::OS_USER, OS_USER)
        .env(notify_env::HOOK_TOKEN, HOOK_TOKEN)
        .env(notify_env::DAEMON_SOCKET, &socket.path);
    cmd
}

/// What one run of the command left behind.
struct Ran {
    succeeded: bool,
    stdout: String,
    stderr: String,
}

/// Run `cmd` to completion off the reactor: the socket it talks to is served by this runtime.
async fn run(mut cmd: Command, args: &[&str]) -> Ran {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let output = tokio::task::spawn_blocking(move || {
        cmd.args(&args)
            .timeout(std::time::Duration::from_secs(10))
            .output()
            .expect("the notify subprocess ran")
    })
    .await
    .expect("the notify subprocess ran to completion");
    Ran {
        succeeded: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

impl Ran {
    fn assert_succeeded_saying(&self, line: &str) -> &Self {
        assert!(
            self.succeeded,
            "expected notify to succeed; stderr: {}",
            self.stderr
        );
        assert_eq!(self.stdout.trim_end(), line);
        self
    }

    fn assert_failed_naming(&self, reason: &str) -> &Self {
        assert!(
            !self.succeeded,
            "expected notify to exit non-zero; stdout: {}",
            self.stdout
        );
        assert!(
            self.stderr.contains(reason),
            "expected the failure to name {reason:?}; stderr: {}",
            self.stderr
        );
        self
    }
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn notify_reads_its_credentials_from_the_environment_and_reports_success() {
    // Given a daemon's agent-tool socket that accepts notifications
    let socket = AnAgentToolSocket::accepting("notifyok");

    // When the agent notifies from a session whose environment names it
    let ran = run(
        tddy_tools_in_a_session_reaching(&socket),
        &[
            "notify",
            "Parallel run finished",
            "--body",
            "4 done, **1 blocked**",
            "--level",
            "success",
            "--sound",
        ],
    )
    .await;

    // Then it says the notification was sent
    ran.assert_succeeded_saying("Notification sent");
    // And the daemon was sent the notification under the credentials the environment holds
    assert_eq!(
        socket.notification_received(),
        Some(ReportOperatorNotificationRequest {
            session_id: SESSION_ID.to_string(),
            hook_token: HOOK_TOKEN.to_string(),
            os_user: OS_USER.to_string(),
            title: "Parallel run finished".to_string(),
            body_markdown: "4 done, **1 blocked**".to_string(),
            level: NotificationLevel::Success as i32,
            sound: true,
        }),
        "the socket was asked for {:?}",
        socket.methods_called()
    );
}

#[tokio::test]
async fn notify_exits_non_zero_naming_the_reason_when_the_daemon_refuses() {
    // Given a daemon's agent-tool socket that refuses the notification's title
    let socket = AnAgentToolSocket::refusing_the_title("notifyrefused");

    // When the agent notifies from its session
    let ran = run(
        tddy_tools_in_a_session_reaching(&socket),
        &["notify", "Parallel run finished"],
    )
    .await;

    // Then it exits non-zero, passing on the daemon's reason
    ran.assert_failed_naming(DAEMONS_REASON);
}

#[tokio::test]
async fn notify_exits_non_zero_when_no_session_environment_is_present() {
    // Given a process with no session environment at all — not launched by the daemon

    // When it tries to notify
    let ran = run(
        tddy_tools_without_a_session_environment(),
        &["notify", "Parallel run finished"],
    )
    .await;

    // Then it exits non-zero, naming the variable that would have said which session it is
    ran.assert_failed_naming(notify_env::SESSION_ID);
}

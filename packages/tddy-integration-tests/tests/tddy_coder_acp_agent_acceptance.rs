//! Acceptance: `tddy-coder --acp` exposes the TDD `WorkflowEngine` as a standard ACP agent.
//!
//! An ACP client (`ClientSideConnection`, the exact SDK the coding backends already use) drives the
//! real `tddy-coder` binary in `--acp` mode over stdio. To keep the run deterministic and free of
//! any external agent, the workflow uses the built-in `stub` coding backend and the single-invoke
//! `free-prompting` recipe. The prompt carries the stub's `SKIP_QUESTIONS` catch-word so no
//! clarification is raised, giving one clean prompt → stream → EndTurn round-trip.
//!
//! Contract pinned here (see docs/ft/coder/acp-agent.md):
//!   initialize → advertises `load_session` + a tddy agent identity
//!   session/new → returns a fresh SessionId
//!   session/prompt → streams `session/update` (AgentMessageChunk / ToolCall) and returns EndTurn
//!   session/load → resumes a previously-created session in a fresh process
//!
//! Run: cargo test -p tddy-integration-tests --test tddy_coder_acp_agent_acceptance -- --test-threads=1

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use agent_client_protocol::{self as acp, Agent, Client};
use serial_test::serial;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn tddy_coder_bin() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    // packages/tddy-integration-tests -> workspace root
    let workspace_root = PathBuf::from(manifest_dir).join("../..");
    #[cfg(windows)]
    let bin = workspace_root.join("target/debug/tddy-coder.exe");
    #[cfg(not(windows))]
    let bin = workspace_root.join("target/debug/tddy-coder");
    bin
}

/// Collects the agent's outbound `session/update` notifications and auto-answers any permission
/// request with its first offered option — so a run that happens to elicit still completes.
#[derive(Default)]
struct CollectingClient {
    agent_text: Rc<RefCell<String>>,
    tool_calls: Rc<RefCell<Vec<String>>>,
    permission_requests: Rc<RefCell<u32>>,
}

#[async_trait::async_trait(?Send)]
impl Client for CollectingClient {
    async fn session_notification(&self, notif: acp::SessionNotification) -> acp::Result<()> {
        match notif.update {
            acp::SessionUpdate::AgentMessageChunk(chunk) => {
                if let acp::ContentBlock::Text(t) = chunk.content {
                    self.agent_text.borrow_mut().push_str(&t.text);
                }
            }
            acp::SessionUpdate::ToolCall(tc) => {
                self.tool_calls.borrow_mut().push(tc.title);
            }
            _ => {}
        }
        Ok(())
    }

    async fn request_permission(
        &self,
        req: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        *self.permission_requests.borrow_mut() += 1;
        let option_id = req
            .options
            .first()
            .map(|o| o.option_id.clone())
            .ok_or_else(acp::Error::internal_error)?;
        Ok(acp::RequestPermissionResponse::new(
            acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(option_id)),
        ))
    }
}

fn spawn_acp_agent(cwd: &PathBuf) -> tokio::process::Child {
    let bin = tddy_coder_bin();
    assert!(
        bin.exists(),
        "tddy-coder not built. Run: cargo build -p tddy-coder"
    );
    tokio::process::Command::new(&bin)
        .arg("--acp")
        .arg("--agent")
        .arg("stub")
        .arg("--recipe")
        .arg("free-prompting")
        .arg("--tddy-data-dir")
        .arg(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn tddy-coder --acp")
}

fn temp_cwd(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("tddy-acp-agent-test").join(name);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

const SKIP: &str = "SKIP_QUESTIONS add a health-check endpoint";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn advertises_load_session_and_a_tddy_agent_identity_on_initialize() {
    // Given
    let cwd = temp_cwd("initialize");
    let mut child = spawn_acp_agent(&cwd);
    let outgoing = child.stdin.take().unwrap().compat_write();
    let incoming = child.stdout.take().unwrap().compat();
    let local_set = tokio::task::LocalSet::new();

    // When
    let init: acp::Result<acp::InitializeResponse> = local_set
        .run_until(async move {
            let (conn, handle_io) = acp::ClientSideConnection::new(
                CollectingClient::default(),
                outgoing,
                incoming,
                |f| {
                    tokio::task::spawn_local(f);
                },
            );
            tokio::task::spawn_local(handle_io);
            conn.initialize(
                acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                    .client_info(acp::Implementation::new("test", "0.1.0").title("Test")),
            )
            .await
        })
        .await;
    let _ = child.kill().await;

    // Then
    let resp = init.expect("initialize should succeed");
    assert!(
        resp.agent_capabilities.load_session,
        "workflow agent must advertise load_session capability"
    );
    let name = resp.agent_info.map(|i| i.name).unwrap_or_default();
    assert!(
        name.contains("tddy"),
        "agent identity should name tddy, was {name:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn streams_agent_output_and_returns_end_turn_for_a_completed_prompt() {
    // Given
    let cwd = temp_cwd("prompt");
    let mut child = spawn_acp_agent(&cwd);
    let outgoing = child.stdin.take().unwrap().compat_write();
    let incoming = child.stdout.take().unwrap().compat();
    let client = CollectingClient::default();
    let agent_text = client.agent_text.clone();
    let local_set = tokio::task::LocalSet::new();

    // When — one prompt drives the free-prompting workflow to completion
    let stop: acp::Result<acp::StopReason> = local_set
        .run_until(async move {
            let (conn, handle_io) =
                acp::ClientSideConnection::new(client, outgoing, incoming, |f| {
                    tokio::task::spawn_local(f);
                });
            tokio::task::spawn_local(handle_io);
            conn.initialize(
                acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                    .client_info(acp::Implementation::new("test", "0.1.0").title("Test")),
            )
            .await?;
            let session = conn
                .new_session(acp::NewSessionRequest::new(cwd.clone()))
                .await?;
            // 20s: an integration test that boots the binary and runs a full workflow step through
            // the stub backend; still bounded so a hang fails loudly instead of blocking the suite.
            let resp = tokio::time::timeout(
                Duration::from_secs(20),
                conn.prompt(acp::PromptRequest::new(
                    session.session_id,
                    vec![SKIP.into()],
                )),
            )
            .await
            .expect("prompt timed out")?;
            Ok(resp.stop_reason)
        })
        .await;
    let _ = child.kill().await;

    // Then
    assert_eq!(
        stop.expect("prompt should succeed"),
        acp::StopReason::EndTurn,
        "a completed workflow prompt returns EndTurn"
    );
    assert!(
        !agent_text.borrow().is_empty(),
        "the workflow's agent output must be streamed as AgentMessageChunk notifications"
    );
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn resumes_a_previously_created_session_via_load_session() {
    // Given — a first process creates + runs a session, and we capture its id
    let cwd = temp_cwd("resume");
    let created_id = {
        let mut child = spawn_acp_agent(&cwd);
        let outgoing = child.stdin.take().unwrap().compat_write();
        let incoming = child.stdout.take().unwrap().compat();
        let local_set = tokio::task::LocalSet::new();
        let cwd = cwd.clone();
        let id: acp::Result<acp::SessionId> = local_set
            .run_until(async move {
                let (conn, handle_io) = acp::ClientSideConnection::new(
                    CollectingClient::default(),
                    outgoing,
                    incoming,
                    |f| {
                        tokio::task::spawn_local(f);
                    },
                );
                tokio::task::spawn_local(handle_io);
                conn.initialize(
                    acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                        .client_info(acp::Implementation::new("test", "0.1.0").title("Test")),
                )
                .await?;
                let session = conn
                    .new_session(acp::NewSessionRequest::new(cwd.clone()))
                    .await?;
                tokio::time::timeout(
                    Duration::from_secs(20),
                    conn.prompt(acp::PromptRequest::new(
                        session.session_id.clone(),
                        vec![SKIP.into()],
                    )),
                )
                .await
                .expect("prompt timed out")?;
                Ok(session.session_id)
            })
            .await;
        let _ = child.kill().await;
        id.expect("first session should be created")
    };

    // When — a fresh process loads that session id
    let mut child = spawn_acp_agent(&cwd);
    let outgoing = child.stdin.take().unwrap().compat_write();
    let incoming = child.stdout.take().unwrap().compat();
    let local_set = tokio::task::LocalSet::new();
    let loaded: acp::Result<()> = local_set
        .run_until(async move {
            let (conn, handle_io) = acp::ClientSideConnection::new(
                CollectingClient::default(),
                outgoing,
                incoming,
                |f| {
                    tokio::task::spawn_local(f);
                },
            );
            tokio::task::spawn_local(handle_io);
            conn.initialize(
                acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                    .client_info(acp::Implementation::new("test", "0.1.0").title("Test")),
            )
            .await?;
            conn.load_session(acp::LoadSessionRequest::new(created_id, cwd.clone()))
                .await
                .map(|_| ())
        })
        .await;
    let _ = child.kill().await;

    // Then
    loaded.expect("load_session should resume the previously-created session");
}

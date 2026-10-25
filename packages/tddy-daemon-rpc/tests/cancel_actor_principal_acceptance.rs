//! The daemon derives who cancelled a conversation from the token the cancel arrives over: a token
//! minted for the session's agent carries the agent principal and is recorded as `main-agent`, the
//! operator's own token is recorded under its login, and an agent token no key this daemon trusts
//! signed authenticates nothing. The user's answer route refuses the agent's token: the main agent
//! answers through `subagent_resume`, never as the user. (A user's genuine token rewritten to claim the principal fails its
//! signature: `tddy-github`'s `session_token_v2` tests pin that.)
//!
//! The setup is `cancel_records_refusal_acceptance.rs`'s, over a host that signs real tokens, copied
//! rather than shared so each file reads alone.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use serial_test::serial;
use tddy_coding_subagent::{CodingAgentLaunch, CodingAgentLauncher};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{AcpAgentCommand, McpServerSpec, SessionMetadata};
use tddy_daemon_rpc::test_util::{signing_test_service_with_coding_agent_launcher, TestDaemon};
use tddy_discovery::subagent::SubagentError;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::acp::{acp_agent_message, session_update, AcpAgentMessage};
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest};
use tddy_service::proto::session_agents_svc::{
    AgentConversationChunk, AnswerAgentPermissionRequest, AnswerAgentPermissionResponse,
    AttachSessionAgentRequest, CancelAgentConversationRequest, CancelAgentConversationResponse, OpenAgentConversationRequest, PromptAgentConversationRequest,
    ResumeAgentConversationRequest, SessionAgentService as _,
};
use tddy_service::subagent_conversation_replay::read_conversation_transcript;
use tddy_session_lifecycle::coding_agent_launcher::DaemonCodingAgentLauncher;
use tddy_session_lifecycle::test_util::{SignedTestCredentials, TEST_USER};

/// `tddy-acp-stub` in the target directory cargo builds into.
fn stub_agent_path() -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(|dir| workspace.join(dir))
        .unwrap_or_else(|| workspace.join("target"));
    let stub = target.join("debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// The `tddy-tools` the daemon's launcher resolves a managed agent's MCP server to.
const TDDY_TOOLS_PATH: &str = "/opt/tddy/bin/tddy-tools";

/// Launches every coding agent as `tddy-acp-stub`, scripted by `scenario.json` and recording into
/// `record.jsonl`, both in `dir`.
struct ScenarioLauncher {
    dir: PathBuf,
    daemons_own: DaemonCodingAgentLauncher,
}

impl ScenarioLauncher {
    fn new(dir: &Path) -> Arc<Self> {
        let daemons_own = DaemonCodingAgentLauncher::new(
            Arc::new(|_| Ok(None)),
            Arc::new(|launch| {
                Ok((
                    PathBuf::from(TDDY_TOOLS_PATH),
                    vec![(
                        "TDDY_REMOTE_SESSION_ID".to_string(),
                        launch.session_id.clone(),
                    )],
                ))
            }),
        );
        Arc::new(Self {
            dir: dir.to_path_buf(),
            daemons_own,
        })
    }
}

#[async_trait::async_trait]
impl CodingAgentLauncher for ScenarioLauncher {
    async fn command_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
        Ok(AcpAgentCommand {
            program: stub_agent_path(),
            args: Vec::new(),
            env: vec![
                (
                    "TDDY_ACP_SCENARIO".to_string(),
                    self.dir.join("scenario.json").display().to_string(),
                ),
                (
                    "TDDY_ACP_STUB_RECORD".to_string(),
                    self.dir.join("record.jsonl").display().to_string(),
                ),
            ],
            cwd: Some(launch.worktree.clone()),
        })
    }

    async fn mcp_servers_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<Vec<McpServerSpec>, SubagentError> {
        self.daemons_own.mcp_servers_for(launch).await
    }
}

const SESSION_ID: &str = "1780828020298-principal";

/// A daemon serving one session with a coding agent attached and a conversation open on it, the
/// conversation paused on one permission request.
struct PausedConversation {
    service: TestDaemon,
    credentials: SignedTestCredentials,
    conversation_id: String,
    request_id: String,
    session_dir: PathBuf,
    /// Owns the daemon's home, the session and the worktree; dropped with the session.
    _sessions: tempfile::TempDir,
}

/// A conversation with a coding agent that asks one permission, paused on it.
async fn a_conversation_paused_on_a_permission_request() -> PausedConversation {
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    let agents_dir = sessions.path().join("agents");
    std::fs::create_dir_all(&agents_dir).expect("agents dir");
    std::fs::write(
        agents_dir.join("codex-fast.yaml"),
        "name: codex-fast\nlabel: \"codex-fast\"\nharness: codex-acp\nmodel: stub-model\n",
    )
    .expect("write profile");
    let worktree = sessions.path().join("worktree");
    std::fs::create_dir_all(&worktree).expect("worktree");
    let session_dir = unified_session_dir_path(sessions.path(), SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("session dir");
    tddy_core::write_session_metadata(&session_dir, &a_session_metadata(&worktree))
        .expect("write session metadata");
    let launcher_dir = sessions.path().join("launcher");
    std::fs::create_dir_all(&launcher_dir).expect("launcher dir");
    std::fs::write(
        launcher_dir.join("scenario.json"),
        serde_json::json!({ "responses": [{
            "chunks": ["tests pass"], "tool_calls": [],
            "permission_requests": [{ "title": "Run cargo test" }],
        }] })
        .to_string(),
    )
    .expect("write scenario");
    let (service, credentials) = signing_test_service_with_coding_agent_launcher(
        sessions.path().to_path_buf(),
        ScenarioLauncher::new(&launcher_dir),
    );

    let agent_id = service
        .list_subagents(Request::direct(ListSubagentsRequest {}))
        .await
        .expect("ListSubagents succeeds")
        .into_inner()
        .subagents
        .into_iter()
        .find(|row| row.name == "codex-fast")
        .expect("a codex-fast row")
        .agent_id;
    service
        .session_agents_service()
        .attach_session_agent(Request::direct(AttachSessionAgentRequest {
            session_token: credentials.user_token.clone(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            agent_id: agent_id.clone(),
            placement: 0,
        }))
        .await
        .expect("attach succeeds");
    let conversation_id = service
        .session_agents_service()
        .open_agent_conversation(Request::direct(OpenAgentConversationRequest {
            session_token: credentials.user_token.clone(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            agent_id,
            conversation_id: String::new(),
            system_prompt: String::new(),
        }))
        .await
        .expect("open succeeds")
        .into_inner()
        .conversation_id;

    let mut prompt = service
        .session_agents_service()
        .prompt_agent_conversation(Request::direct(PromptAgentConversationRequest {
            session_token: credentials.user_token.clone(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            conversation_id: conversation_id.clone(),
            prompt: "Run the tests".to_string(),
            max_turns: None,
            yield_conditions_json: String::new(),
        }))
        .await
        .expect("prompting succeeds")
        .into_inner();
    let frames = collect_frames(&mut prompt).await;
    let last = frames.last().expect("a prompt produces a final frame");
    assert_eq!(last.stop_reason, "PermissionRequested");
    let request: serde_json::Value =
        serde_json::from_str(&last.permission_request_json).expect("the request is JSON");
    let request_id = request["requestId"]
        .as_str()
        .expect("the request is named")
        .to_string();

    PausedConversation {
        service,
        credentials,
        conversation_id,
        request_id,
        session_dir,
        _sessions: sessions,
    }
}

impl PausedConversation {
    /// Resume the paused turn, handing `request_id` to the user, as the main agent does.
    fn resume_deferring(
        &self,
    ) -> impl futures_util::Future<Output = Vec<AgentConversationChunk>> + '_ {
        let request = ResumeAgentConversationRequest {
            session_token: self.credentials.user_token.clone(),
            session_id: SESSION_ID.to_string(),
            daemon_instance_id: String::new(),
            conversation_id: self.conversation_id.clone(),
            from_message_id: None,
            correction: None,
            max_turns: None,
            yield_conditions_json: String::new(),
            replacement_json: String::new(),
            permission_answer_json: String::new(),
            defer_permission_to_user: Some(self.request_id.clone()),
        };
        async move {
            let mut stream = self
                .service
                .session_agents_service()
                .resume_agent_conversation(Request::direct(request))
                .await
                .expect("resuming an open conversation succeeds")
                .into_inner();
            collect_frames(&mut stream).await
        }
    }
}

/// Every frame of a turn's stream, up to and including the final one.
async fn collect_frames<S>(stream: &mut S) -> Vec<AgentConversationChunk>
where
    S: futures_util::Stream<Item = Result<AgentConversationChunk, Status>> + Unpin,
{
    let mut frames = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(frame) = stream.next().await {
            let frame = frame.expect("the conversation stream must not error");
            let last = frame.last;
            frames.push(frame);
            if last {
                return;
            }
        }
    })
    .await
    .expect("the turn ends within the deadline");
    frames
}

fn a_session_metadata(worktree: &Path) -> SessionMetadata {
    let mut metadata: SessionMetadata = serde_json::from_value(serde_json::json!({
        "session_id": SESSION_ID,
        "project_id": "project-with-a-coding-agent",
        "created_at": "2026-10-24T10:00:00Z",
        "updated_at": "2026-10-24T10:00:00Z",
        "status": "active",
        "session_type": "claude-cli",
        "sandbox": true,
    }))
    .expect("session metadata");
    metadata.repo_path = Some(worktree.display().to_string());
    metadata
}

impl PausedConversation {
    /// Cancel the conversation over `session_token`.
    async fn cancel_over(
        &self,
        session_token: &str,
    ) -> Result<CancelAgentConversationResponse, Status> {
        self.service
            .session_agents_service()
            .cancel_agent_conversation(Request::direct(CancelAgentConversationRequest {
                session_token: session_token.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: self.conversation_id.clone(),
            }))
            .await
            .map(|response| response.into_inner())
    }

    /// Answer the paused request over `session_token`, choosing `option_id`, as the web does.
    async fn answer_over(
        &self,
        session_token: &str,
        option_id: &str,
    ) -> Result<AnswerAgentPermissionResponse, Status> {
        self.service
            .session_agents_service()
            .answer_agent_permission(Request::direct(AnswerAgentPermissionRequest {
                session_token: session_token.to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: self.conversation_id.clone(),
                request_id: self.request_id.clone(),
                option_id: option_id.to_string(),
                cancelled: false,
            }))
            .await
            .map(|response| response.into_inner())
    }

    /// Defer the paused request to the user and give the resume time to register it.
    async fn defer_to_the_user(&self) {
        let resume = self.resume_deferring();
        tokio::pin!(resume);
        let _ = tokio::time::timeout(Duration::from_millis(300), &mut resume).await;
    }

    /// Who the request's completed record names as having answered it.
    fn answered_by(&self) -> Option<String> {
        let frames: Vec<AcpAgentMessage> =
            read_conversation_transcript(&self.session_dir, &self.conversation_id)
                .expect("the transcript reads")
                .expect("the conversation recorded its turns");
        let tool_call_id = format!("permission:{}", self.request_id);
        frames.iter().rev().find_map(|frame| match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(note)) => {
                match note.update.as_ref().and_then(|u| u.update.as_ref()) {
                    Some(session_update::Update::ToolCall(call))
                        if call.tool_call_id.as_ref().map(|id| id.value.as_str())
                            == Some(tool_call_id.as_str()) =>
                    {
                        let output: serde_json::Value =
                            serde_json::from_str(call.raw_output.as_deref()?).ok()?;
                        output["answeredBy"].as_str().map(str::to_string)
                    }
                    _ => None,
                }
            }
            _ => None,
        })
    }
}

/// An agent token for [`TEST_USER`] signed by a key no daemon in this test has published: what a
/// caller minting its own "agent" token would hold.
fn an_agent_token_from_a_key_nobody_trusts() -> String {
    let home = tempfile::tempdir().expect("a stranger's key home");
    let stranger = tddy_daemon_auth::DaemonSigningKey::load_or_generate(
        &home.path().join(tddy_daemon_auth::SIGNING_KEY_FILE),
    )
    .expect("the stranger generates a key");
    stranger.signer().mint_agent(
        &tddy_github::GitHubUser {
            id: 7,
            login: TEST_USER.to_string(),
            avatar_url: String::new(),
            name: TEST_USER.to_string(),
        },
        Duration::from_secs(600),
    )
}

#[tokio::test]
#[serial]
async fn a_cancel_over_the_agents_token_is_recorded_as_the_main_agents() {
    // Given a deferred request the main agent cancels over the token it was given
    let paused = a_conversation_paused_on_a_permission_request().await;
    paused.defer_to_the_user().await;

    // When it cancels
    paused
        .cancel_over(&paused.credentials.agent_token.clone())
        .await
        .expect("the agent's cancel succeeds");

    // Then the request is recorded as answered by the main agent, not by the login it acts for
    assert_eq!(paused.answered_by().as_deref(), Some("main-agent"));
}

#[tokio::test]
#[serial]
async fn a_cancel_over_the_users_token_is_recorded_under_the_users_login() {
    // Given a deferred request the operator cancels over their own token
    let paused = a_conversation_paused_on_a_permission_request().await;
    paused.defer_to_the_user().await;

    // When they cancel
    paused
        .cancel_over(&paused.credentials.user_token.clone())
        .await
        .expect("the user's cancel succeeds");

    // Then the request is recorded under their login
    assert_eq!(paused.answered_by().as_deref(), Some(TEST_USER));
}

#[tokio::test]
#[serial]
async fn an_agent_token_no_trusted_key_signed_cancels_nothing() {
    // Given a deferred request, and an agent token signed by a key this daemon never learned
    let paused = a_conversation_paused_on_a_permission_request().await;
    paused.defer_to_the_user().await;
    let forged = an_agent_token_from_a_key_nobody_trusts();

    // When a cancel arrives over it
    let refusal = paused
        .cancel_over(&forged)
        .await
        .expect_err("an unverifiable agent token authenticates nothing");

    // Then it is refused, and the conversation is still open for its real owner to cancel
    assert_eq!(refusal.code, Code::Unauthenticated);
    paused
        .cancel_over(&paused.credentials.user_token.clone())
        .await
        .expect("the conversation was left open");
    assert_eq!(paused.answered_by().as_deref(), Some(TEST_USER));
}

#[tokio::test]
#[serial]
async fn the_users_answer_route_refuses_the_agents_token() {
    // Given a request the main agent deferred to the user
    let paused = a_conversation_paused_on_a_permission_request().await;
    paused.defer_to_the_user().await;

    // When an answer arrives on the user's route over the agent's token
    let refusal = paused
        .answer_over(&paused.credentials.agent_token.clone(), "allow-once")
        .await
        .expect_err("the agent does not answer as the user");

    // Then it is refused as not permitted, naming the route the agent answers through, and the
    // request is still the user's to answer
    assert_eq!(refusal.code, Code::PermissionDenied);
    assert!(
        refusal.message.contains("subagent_resume"),
        "{}",
        refusal.message
    );
    let answered = paused
        .answer_over(&paused.credentials.user_token.clone(), "allow-once")
        .await
        .expect("the user still answers the request");
    assert_eq!(answered.answered_by, TEST_USER);
}

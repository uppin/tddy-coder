//! Answering a permission request whose conversation's turn loop runs on another daemon.
//!
//! The web answers the daemon that holds the conversation, and that daemon does not run the loop:
//! the owner does, and the owner holds the parked request and its deferral gate. So the holder
//! carries the answer to the owner through the peers port and returns what the owner recorded,
//! rather than refusing an answer it cannot deliver itself.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tddy_core::SessionAgentRecord;
use tddy_discovery::subagent::SubagentSession;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::session_agents_svc::{
    AgentConversationChunk, AnswerAgentPermissionRequest, AnswerAgentPermissionResponse,
    OpenAgentConversationRequest, PromptAgentConversationRequest, ResumeAgentConversationRequest,
    SessionAgentService,
};
use tddy_session_agents::ports::{
    AdmittedAgent, AgentAdmission, AgentCatalog, AgentConversationPeers, AgentSessions,
    RosterBroadcast, SessionAgentPorts,
};
use tddy_session_agents::session_agent_clone::SessionAgentCloneStore;
use tddy_session_agents::session_agent_roster::SessionAgentRosterStore;
use tddy_session_agents::session_agent_status::SessionAgentActivityStore;
use tddy_session_agents::{OpenAgentConversations, SessionAgentServiceImpl};
use tddy_testing_commons::a_session_metadata;
use tokio::sync::mpsc::UnboundedReceiver;

// ─── the fixed facts these tests are written against ────────────────────────────────────────────

const THIS_DAEMON: &str = "holder-daemon";
const OWNER: &str = "owner-daemon";
const A_SESSION: &str = "01a04d08-2bbf-7850-ae60-0df89791608a";
const AN_AGENT: &str = "FastContext@owner-daemon";
const A_SESSION_TOKEN: &str = "session-token-for-the-holding-daemon";
const A_CONVERSATION: &str = "conv-running-on-the-owner";
const A_REQUEST: &str = "perm-1";

// ─── builders ───────────────────────────────────────────────────────────────────────────────────

/// The roster record for an agent the owning daemon runs, attached to this session.
fn an_agent_the_owner_runs() -> SessionAgentRecord {
    SessionAgentRecord {
        agent_id: AN_AGENT.to_string(),
        name: "FastContext".to_string(),
        daemon_instance_id: OWNER.to_string(),
        label: Some("FastContext (owner)".to_string()),
        model: "fastcontext-tools-32k:latest".to_string(),
        replaces: Vec::new(),
        tools: vec!["Read".to_string(), "Grep".to_string()],
        codebase_session_id: None,
        kind: Default::default(),
        harness: None,
        placement: Default::default(),
    }
}

/// A session directory whose `.session.yaml` holds `agent` as an attached roster entry.
fn a_session_directory_with(agent: SessionAgentRecord) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary session directory");
    let metadata = a_session_metadata()
        .with_session_id(A_SESSION)
        .with_agents(1, [agent])
        .build();
    tddy_core::write_session_metadata(dir.path(), &metadata).expect("write .session.yaml");
    dir
}

/// An open of `AN_AGENT`, which this daemon holds as remote because `OWNER` runs its loop.
fn an_open_of_the_remote_agent() -> Request<OpenAgentConversationRequest> {
    Request::direct(OpenAgentConversationRequest {
        session_token: A_SESSION_TOKEN.to_string(),
        session_id: A_SESSION.to_string(),
        daemon_instance_id: OWNER.to_string(),
        agent_id: AN_AGENT.to_string(),
        conversation_id: A_CONVERSATION.to_string(),
        system_prompt: String::new(),
    })
}

/// The web's answer to the request the owner holds: an option, not a cancel.
fn an_answer() -> Request<AnswerAgentPermissionRequest> {
    Request::direct(AnswerAgentPermissionRequest {
        session_token: A_SESSION_TOKEN.to_string(),
        session_id: A_SESSION.to_string(),
        daemon_instance_id: String::new(),
        conversation_id: A_CONVERSATION.to_string(),
        request_id: A_REQUEST.to_string(),
        option_id: "allow-once".to_string(),
        cancelled: false,
    })
}

// ─── the owner, as the holder reaches it ────────────────────────────────────────────────────────

/// One `AnswerAgentPermission` the holder forwarded, as the owner received it.
#[derive(Debug, Clone)]
struct ForwardedAnswer {
    owner: String,
    session_token: String,
    session_id: String,
    request: AnswerAgentPermissionRequest,
}

/// The owning daemon, as the peers port reaches it. It records each answer forwarded to it and
/// returns the reply it was built with: the answer it recorded, or the refusal its own gate gave.
struct TheOwnerAnswering {
    reply: Result<AnswerAgentPermissionResponse, Status>,
    forwarded: Mutex<Vec<ForwardedAnswer>>,
}

impl TheOwnerAnswering {
    fn recording(answered_by: &str) -> Arc<Self> {
        Arc::new(Self {
            reply: Ok(AnswerAgentPermissionResponse {
                answered_by: answered_by.to_string(),
            }),
            forwarded: Mutex::new(Vec::new()),
        })
    }

    fn refusing(refusal: Status) -> Arc<Self> {
        Arc::new(Self {
            reply: Err(refusal),
            forwarded: Mutex::new(Vec::new()),
        })
    }

    /// The single answer forwarded, or a loud failure when none or several were — either would
    /// make the assertions about the forward meaningless.
    fn the_one_forward(&self) -> ForwardedAnswer {
        let forwarded = self.forwarded.lock().expect("read the forwarded answers");
        assert_eq!(
            forwarded.len(),
            1,
            "expected exactly one forwarded answer, got {forwarded:?}"
        );
        forwarded[0].clone()
    }
}

// ─── the ports the holder's own answer path does not reach ──────────────────────────────────────

/// The turn loops this holder can open. It holds the conversation as remote, so it opens none.
struct NoTurnLoopHere;

#[async_trait]
impl AgentSessions for NoTurnLoopHere {
    fn hosts_a_clone_for(&self, _session_id: &str) -> bool {
        false
    }

    async fn open_owned(
        &self,
        _session_id: &str,
        _agent_id: &str,
        _system_prompt: Option<&str>,
    ) -> Result<Option<Box<dyn SubagentSession>>, Status> {
        Ok(None)
    }

    async fn open_local(
        &self,
        _session_id: &str,
        _conversation_id: &str,
        _session_dir: &Path,
        _record: &SessionAgentRecord,
        _session_token: &str,
        _system_prompt: Option<&str>,
    ) -> Result<Box<dyn SubagentSession>, Status> {
        Err(Status::internal(
            "the agent runs on another daemon, so this holder opens no turn loop for it",
        ))
    }

    fn refuse_unready_clone(
        &self,
        _session_id: &str,
        _record: &SessionAgentRecord,
    ) -> Result<(), Status> {
        Ok(())
    }

    async fn refuse_departed_owner(&self, _daemon_instance_id: &str) -> Result<(), Status> {
        Ok(())
    }

    fn github_user(&self, _session_token: &str) -> Result<String, Status> {
        Ok("the-holding-daemons-user".to_string())
    }

    fn answerer(
        &self,
        _session_token: &str,
    ) -> Result<tddy_discovery::subagent::AnsweredBy, Status> {
        Err(Status::internal(
            "this test cancels no pending request, so it names no answerer",
        ))
    }
}

/// Every port this test does not reach. Each answers by naming itself, so a handler that starts
/// reaching for one fails saying which rather than quietly succeeding.
struct PortsAnAnswerDoesNotUse;

#[async_trait]
impl AgentCatalog for PortsAnAnswerDoesNotUse {
    async fn record_for(&self, agent_id: &str) -> Result<SessionAgentRecord, Status> {
        Err(Status::internal(format!(
            "answering a permission resolves '{agent_id}' from the roster, never from the catalog"
        )))
    }
}

#[async_trait]
impl AgentAdmission for PortsAnAnswerDoesNotUse {
    async fn admit(
        &self,
        _session_id: &str,
        _session_dir: &Path,
        _record: &SessionAgentRecord,
        _session_token: &str,
    ) -> Result<AdmittedAgent, Status> {
        Err(Status::internal(
            "answering a permission admits nothing: the agent is already attached",
        ))
    }

    async fn withdraw(&self, _session_id: &str, _admitted: &AdmittedAgent, _session_token: &str) {}

    async fn tear_down(
        &self,
        _session_id: &str,
        _daemon_instance_id: &str,
        _codebase_session_id: &str,
        _session_token: &str,
    ) -> Result<(), Status> {
        Err(Status::internal(
            "answering a permission tears no checkout down",
        ))
    }
}

#[async_trait]
impl RosterBroadcast for PortsAnAnswerDoesNotUse {
    async fn broadcast(
        &self,
        _session_id: &str,
        _roster: &tddy_service::proto::session_agents_svc::SessionAgentRoster,
    ) {
    }
}

#[async_trait]
impl AgentConversationPeers for TheOwnerAnswering {
    /// The open of the remote conversation reaches the owner once; it succeeds there.
    async fn open(
        &self,
        _request: &OpenAgentConversationRequest,
        _owner: &str,
        _conversation_id: &str,
    ) -> Result<(), Status> {
        Ok(())
    }

    async fn prompt(
        &self,
        _request: &PromptAgentConversationRequest,
        owner: &str,
    ) -> Result<UnboundedReceiver<Result<AgentConversationChunk, Status>>, Status> {
        Err(Status::internal(format!(
            "no prompt is taken in this test, so none is sent to '{owner}'"
        )))
    }

    async fn resume(
        &self,
        _request: &ResumeAgentConversationRequest,
        owner: &str,
    ) -> Result<UnboundedReceiver<Result<AgentConversationChunk, Status>>, Status> {
        Err(Status::internal(format!(
            "no resume is taken in this test, so none is sent to '{owner}'"
        )))
    }

    async fn cancel(
        &self,
        _session_token: &str,
        _session_id: &str,
        owner: &str,
        _conversation_id: &str,
    ) -> Result<(), Status> {
        Err(Status::internal(format!(
            "no cancel is sent in this test, so none reaches '{owner}'"
        )))
    }

    async fn answer_permission(
        &self,
        session_token: &str,
        session_id: &str,
        owner: &str,
        request: &AnswerAgentPermissionRequest,
    ) -> Result<AnswerAgentPermissionResponse, Status> {
        self.forwarded
            .lock()
            .expect("record the forwarded answer")
            .push(ForwardedAnswer {
                owner: owner.to_string(),
                session_token: session_token.to_string(),
                session_id: session_id.to_string(),
                request: request.clone(),
            });
        self.reply.clone()
    }
}

// ─── the service under test ─────────────────────────────────────────────────────────────────────

/// The holding daemon, serving `session_dir` and forwarding to `owner`.
fn a_daemon_holding_the_conversation(
    session_dir: &Path,
    owner: Arc<TheOwnerAnswering>,
) -> SessionAgentServiceImpl {
    let session_dir: PathBuf = session_dir.to_path_buf();
    let clones = Arc::new(SessionAgentCloneStore::new());
    SessionAgentServiceImpl::new(SessionAgentPorts {
        session_dirs: Arc::new(move |_token, _session_id| Ok(session_dir.clone())),
        local_instance_id: THIS_DAEMON.to_string(),
        roster_keepalive: Duration::from_secs(60),
        rosters: Arc::new(SessionAgentRosterStore::new(
            Arc::clone(&clones),
            Arc::new(SessionAgentActivityStore::new()),
        )),
        clones,
        conversations: Arc::new(OpenAgentConversations::new()),
        conversation_frames: Arc::new(
            tddy_daemon_kernel::conversation_frames::ConversationFrames::default(),
        ),
        admission: Arc::new(PortsAnAnswerDoesNotUse),
        catalog: Arc::new(PortsAnAnswerDoesNotUse),
        broadcast: Arc::new(PortsAnAnswerDoesNotUse),
        sessions: Arc::new(NoTurnLoopHere),
        peers: owner,
    })
}

/// A holding daemon with the remote conversation already open on it.
async fn a_holder_with_an_open_remote_conversation(
    owner: Arc<TheOwnerAnswering>,
) -> (tempfile::TempDir, SessionAgentServiceImpl) {
    let session = a_session_directory_with(an_agent_the_owner_runs());
    let daemon = a_daemon_holding_the_conversation(session.path(), owner);
    daemon
        .open_agent_conversation(an_open_of_the_remote_agent())
        .await
        .expect("the remote conversation should have opened");
    (session, daemon)
}

// ─── tests ──────────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_remote_answer_is_forwarded_to_the_owner_and_returns_who_the_owner_recorded() {
    // Given a conversation whose loop runs on the owning daemon, held here as remote
    let owner = TheOwnerAnswering::recording("the-owners-user");
    let (_session, daemon) = a_holder_with_an_open_remote_conversation(Arc::clone(&owner)).await;

    // When the web answers the request the owner holds
    let answered = daemon
        .answer_agent_permission(an_answer())
        .await
        .expect("the owner's answer should be returned");

    // Then the answer reached the owner, under the caller's session and the conversation's request
    let forwarded = owner.the_one_forward();
    assert_eq!(forwarded.owner, OWNER);
    assert_eq!(forwarded.session_token, A_SESSION_TOKEN);
    assert_eq!(forwarded.session_id, A_SESSION);
    assert_eq!(forwarded.request.conversation_id, A_CONVERSATION);
    assert_eq!(forwarded.request.request_id, A_REQUEST);
    assert_eq!(forwarded.request.option_id, "allow-once");
    assert!(!forwarded.request.cancelled);

    // And the answer reports the owner's answered_by, not one this holder recorded itself
    assert_eq!(answered.into_inner().answered_by, "the-owners-user");
}

#[tokio::test]
async fn a_refusal_from_the_owner_is_returned_as_it_is() {
    // Given an owner that refuses the answer, as its own deferral gate does for a request it kept
    let refusal = Status::failed_precondition(format!(
        "permission request '{A_REQUEST}' of conversation '{A_CONVERSATION}' was not handed to the \
         user, so the user may not answer it"
    ));
    let owner = TheOwnerAnswering::refusing(refusal.clone());
    let (_session, daemon) = a_holder_with_an_open_remote_conversation(Arc::clone(&owner)).await;

    // When the web answers that request through this holder
    let returned = daemon
        .answer_agent_permission(an_answer())
        .await
        .expect_err("the owner's refusal should be returned");

    // Then the code and the message are the owner's, unchanged
    assert_eq!(returned.code, Code::FailedPrecondition);
    assert_eq!(returned.message, refusal.message);
    assert_eq!(owner.the_one_forward().owner, OWNER);
}

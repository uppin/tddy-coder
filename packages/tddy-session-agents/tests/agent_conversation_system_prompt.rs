//! Opening a conversation under a system prompt the *caller* chose, not the def's author.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md. A def's `system_prompt` is authored once,
//! for every use of that agent; the agent delegating a specific piece of work knows things the
//! def's author could not. `OpenAgentConversation` carries that override, and these pin what the
//! daemon does with it: hand it to whichever turn loop it builds, and hand nothing on when the
//! caller sent nothing — a dropped override would report a conversation as running under a prompt
//! it is not.
//!
//! Both branches of the handler are covered, because both build a turn loop: the agent this host
//! *resolves* for a session it facilitates (`open_local`), and the agent this host *owns*, read
//! out of the clone it holds for a peer's session (`open_owned`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tddy_core::SessionAgentRecord;
use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{PromptOutcome, SubagentError, SubagentSession, TurnRequest};
use tddy_rpc::{Request, Status};
use tddy_service::proto::session_agents_svc::{
    AgentConversationChunk, OpenAgentConversationRequest, PromptAgentConversationRequest,
    ResumeAgentConversationRequest, SessionAgentService,
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

const THIS_DAEMON: &str = "udoo";
const A_SESSION: &str = "01a04d08-2bbf-7850-ae60-0df89791608a";
const AN_AGENT: &str = "FastContext@udoo";
const A_SESSION_TOKEN: &str = "session-token-for-the-facilitating-daemon";
const AN_OVERRIDE: &str = "Answer only with file paths, one per line.";

// ─── builders ───────────────────────────────────────────────────────────────────────────────────

/// The roster record for an agent this daemon resolves and runs itself.
fn an_agent_this_daemon_runs() -> SessionAgentRecord {
    SessionAgentRecord {
        agent_id: AN_AGENT.to_string(),
        name: "FastContext".to_string(),
        daemon_instance_id: THIS_DAEMON.to_string(),
        label: Some("FastContext (local)".to_string()),
        model: "fastcontext-tools-32k:latest".to_string(),
        replaces: Vec::new(),
        tools: vec!["Read".to_string(), "Grep".to_string()],
        codebase_session_id: None,
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

/// An open of `AN_AGENT`, carrying whatever system prompt the caller chose — `""` for none.
fn an_open_carrying(system_prompt: &str) -> Request<OpenAgentConversationRequest> {
    Request::direct(OpenAgentConversationRequest {
        session_token: A_SESSION_TOKEN.to_string(),
        session_id: A_SESSION.to_string(),
        daemon_instance_id: THIS_DAEMON.to_string(),
        agent_id: AN_AGENT.to_string(),
        conversation_id: "conv-1".to_string(),
        system_prompt: system_prompt.to_string(),
    })
}

// ─── the turn loops this host can open ──────────────────────────────────────────────────────────

/// A turn loop that has been opened and asked nothing. Never prompted by these tests: what is
/// under test is what the handler hands to the *opening*, not what a turn does with it.
struct AnOpenedTurnLoop;

#[async_trait]
impl SubagentSession for AnOpenedTurnLoop {
    async fn take_turn(&mut self, _request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        Err(SubagentError::from(
            "these tests open conversations and never take a turn on one".to_string(),
        ))
    }

    async fn prompt(&mut self, _text: &str) -> Result<PromptOutcome, SubagentError> {
        Err(SubagentError::from(
            "these tests open conversations and never prompt one".to_string(),
        ))
    }

    fn model(&self) -> &str {
        "fastcontext-tools-32k:latest"
    }

    fn cumulative_usage(&self) -> TokenUsage {
        TokenUsage::default()
    }

    fn context_tokens(&self) -> u64 {
        0
    }

    fn tail(&self, _max_messages: usize) -> Vec<String> {
        Vec::new()
    }
}

/// The host's turn loops, recording the system prompt each open was given.
///
/// `holds_a_clone` is what decides which branch of the handler runs: a host holding a clone for
/// the session owns the agent and opens the loop against that checkout, and one that does not
/// resolves the agent out of the session's own roster.
struct TheTurnLoopsThisHostOpened {
    holds_a_clone: bool,
    opened_locally_with: Mutex<Vec<Option<String>>>,
    opened_as_owner_with: Mutex<Vec<Option<String>>>,
}

impl TheTurnLoopsThisHostOpened {
    fn facilitating_the_session() -> Arc<Self> {
        Arc::new(Self {
            holds_a_clone: false,
            opened_locally_with: Mutex::new(Vec::new()),
            opened_as_owner_with: Mutex::new(Vec::new()),
        })
    }

    fn holding_a_clone_for_the_session() -> Arc<Self> {
        Arc::new(Self {
            holds_a_clone: true,
            opened_locally_with: Mutex::new(Vec::new()),
            opened_as_owner_with: Mutex::new(Vec::new()),
        })
    }

    /// The system prompt the single local open was given, or a loud failure when the handler
    /// opened no loop or more than one — either would make the assertion meaningless.
    fn the_one_local_open(&self) -> Option<String> {
        the_one(&self.opened_locally_with, "open_local")
    }

    fn the_one_owned_open(&self) -> Option<String> {
        the_one(&self.opened_as_owner_with, "open_owned")
    }
}

fn the_one(recorded: &Mutex<Vec<Option<String>>>, port: &str) -> Option<String> {
    let opens = recorded.lock().expect("read the recorded opens");
    assert_eq!(
        opens.len(),
        1,
        "expected exactly one {port} call, got {opens:?}"
    );
    opens[0].clone()
}

#[async_trait]
impl AgentSessions for TheTurnLoopsThisHostOpened {
    fn hosts_a_clone_for(&self, _session_id: &str) -> bool {
        self.holds_a_clone
    }

    async fn open_owned(
        &self,
        _session_id: &str,
        _agent_id: &str,
        system_prompt: Option<&str>,
    ) -> Result<Option<Box<dyn SubagentSession>>, Status> {
        if !self.holds_a_clone {
            return Ok(None);
        }
        self.opened_as_owner_with
            .lock()
            .expect("record the owned open")
            .push(system_prompt.map(str::to_string));
        Ok(Some(Box::new(AnOpenedTurnLoop)))
    }

    async fn open_local(
        &self,
        _session_id: &str,
        _session_dir: &Path,
        _record: &SessionAgentRecord,
        _session_token: &str,
        system_prompt: Option<&str>,
    ) -> Result<Box<dyn SubagentSession>, Status> {
        self.opened_locally_with
            .lock()
            .expect("record the local open")
            .push(system_prompt.map(str::to_string));
        Ok(Box::new(AnOpenedTurnLoop))
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
}

// ─── the ports an open never reaches ────────────────────────────────────────────────────────────

/// Every port `OpenAgentConversation` must not consult. Each answers by naming itself, so a
/// handler that starts reaching for one fails saying which rather than quietly succeeding.
struct PortsAnOpenDoesNotUse;

#[async_trait]
impl AgentCatalog for PortsAnOpenDoesNotUse {
    async fn record_for(&self, agent_id: &str) -> Result<SessionAgentRecord, Status> {
        Err(Status::internal(format!(
            "opening a conversation resolves '{agent_id}' from the roster, never from the catalog"
        )))
    }
}

#[async_trait]
impl AgentAdmission for PortsAnOpenDoesNotUse {
    async fn admit(
        &self,
        _session_id: &str,
        _session_dir: &Path,
        _record: &SessionAgentRecord,
        _session_token: &str,
    ) -> Result<AdmittedAgent, Status> {
        Err(Status::internal(
            "opening a conversation admits nothing: the agent is already attached",
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
            "opening a conversation tears no checkout down",
        ))
    }
}

#[async_trait]
impl RosterBroadcast for PortsAnOpenDoesNotUse {
    async fn broadcast(
        &self,
        _session_id: &str,
        _roster: &tddy_service::proto::session_agents_svc::SessionAgentRoster,
    ) {
    }
}

#[async_trait]
impl AgentConversationPeers for PortsAnOpenDoesNotUse {
    async fn open(
        &self,
        _request: &OpenAgentConversationRequest,
        owner: &str,
        _conversation_id: &str,
    ) -> Result<(), Status> {
        Err(Status::internal(format!(
            "this host runs the agent itself; nothing should be forwarded to '{owner}'"
        )))
    }

    async fn prompt(
        &self,
        _request: &PromptAgentConversationRequest,
        owner: &str,
    ) -> Result<UnboundedReceiver<Result<AgentConversationChunk, Status>>, Status> {
        Err(Status::internal(format!(
            "no turn is taken here, so nothing is prompted on '{owner}'"
        )))
    }

    async fn resume(
        &self,
        _request: &ResumeAgentConversationRequest,
        owner: &str,
    ) -> Result<UnboundedReceiver<Result<AgentConversationChunk, Status>>, Status> {
        Err(Status::internal(format!(
            "no turn is taken here, so nothing is resumed on '{owner}'"
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
            "these tests cancel nothing on '{owner}'"
        )))
    }
}

// ─── the service under test ─────────────────────────────────────────────────────────────────────

/// The service, serving `session_dir` and opening its turn loops through `sessions`.
fn a_daemon_serving(
    session_dir: &Path,
    sessions: Arc<TheTurnLoopsThisHostOpened>,
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
        admission: Arc::new(PortsAnOpenDoesNotUse),
        catalog: Arc::new(PortsAnOpenDoesNotUse),
        broadcast: Arc::new(PortsAnOpenDoesNotUse),
        sessions,
        peers: Arc::new(PortsAnOpenDoesNotUse),
    })
}

// ─── tests ──────────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_agent_this_daemon_runs_is_opened_under_the_system_prompt_the_caller_sent() {
    // Given a session whose agent this daemon resolves and runs itself
    let session = a_session_directory_with(an_agent_this_daemon_runs());
    let sessions = TheTurnLoopsThisHostOpened::facilitating_the_session();
    let daemon = a_daemon_serving(session.path(), Arc::clone(&sessions));

    // When a conversation is opened carrying an override
    daemon
        .open_agent_conversation(an_open_carrying(AN_OVERRIDE))
        .await
        .expect("the conversation should have opened");

    // Then the turn loop was built with it
    assert_eq!(sessions.the_one_local_open(), Some(AN_OVERRIDE.to_string()));
}

#[tokio::test]
async fn an_open_that_sends_no_system_prompt_leaves_the_defs_own_prompt_in_place() {
    // Given a session whose agent this daemon resolves and runs itself
    let session = a_session_directory_with(an_agent_this_daemon_runs());
    let sessions = TheTurnLoopsThisHostOpened::facilitating_the_session();
    let daemon = a_daemon_serving(session.path(), Arc::clone(&sessions));

    // When a conversation is opened with no override
    daemon
        .open_agent_conversation(an_open_carrying(""))
        .await
        .expect("the conversation should have opened");

    // Then the turn loop was built with none, so the def's own prompt is what it runs under
    assert_eq!(sessions.the_one_local_open(), None);
}

#[tokio::test]
async fn an_agent_this_daemon_owns_is_opened_under_the_system_prompt_the_caller_sent() {
    // Given a host holding the clone for a peer's session, which is where its own agent reads
    let session = a_session_directory_with(an_agent_this_daemon_runs());
    let sessions = TheTurnLoopsThisHostOpened::holding_a_clone_for_the_session();
    let daemon = a_daemon_serving(session.path(), Arc::clone(&sessions));

    // When the facilitating daemon forwards an open carrying an override
    daemon
        .open_agent_conversation(an_open_carrying(AN_OVERRIDE))
        .await
        .expect("the conversation should have opened");

    // Then the turn loop against the clone was built with it too
    assert_eq!(sessions.the_one_owned_open(), Some(AN_OVERRIDE.to_string()));
}

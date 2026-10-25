//! Acceptance: an agent a session already holds keeps the placement it was attached with.
//!
//! A conversation's worktree is cut from the placement the roster holds, so attaching the same agent
//! under the other placement cannot be honoured. It is refused before anything is admitted, and the
//! same placement stays the idempotent attach it always was.
//!
//! Feature doc: docs/ft/coder/managed-codebase-subagents.md § A daemon-run conversation of a `clone`-placed agent

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_core::{AgentKindRecord, SessionAgentRecord};
use tddy_discovery::subagent::SubagentSession;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::session_agents_svc::AgentPlacement;
use tddy_service::proto::session_agents_svc::{
    AttachSessionAgentRequest, SessionAgentRoster, SessionAgentService,
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

const SESSION: &str = "1780828020298-reattach";
const SESSION_TOKEN: &str = "a-session-token";
const THIS_DAEMON: &str = "daemon-a";
const AN_AGENT: &str = "explorer@daemon-a";

/// Admits every agent without claiming a checkout, and remembers which agents it was asked about.
#[derive(Default)]
struct RecordingAdmission {
    asked: Mutex<Vec<String>>,
}

impl RecordingAdmission {
    fn asked_about(&self) -> Vec<String> {
        self.asked.lock().expect("admissions").clone()
    }
}

#[async_trait]
impl AgentAdmission for RecordingAdmission {
    async fn admit(
        &self,
        _session_id: &str,
        _session_dir: &Path,
        record: &SessionAgentRecord,
        _session_token: &str,
    ) -> Result<AdmittedAgent, Status> {
        self.asked
            .lock()
            .expect("admissions")
            .push(record.agent_id.clone());
        Ok(AdmittedAgent {
            daemon_instance_id: record.daemon_instance_id.clone(),
            codebase_session_id: None,
            commissioned: false,
        })
    }

    async fn withdraw(&self, _session_id: &str, _admitted: &AdmittedAgent, _session_token: &str) {}

    async fn tear_down(
        &self,
        _session_id: &str,
        _daemon_instance_id: &str,
        _codebase_session_id: &str,
        _session_token: &str,
    ) -> Result<(), Status> {
        Ok(())
    }
}

/// Resolves the one agent this test attaches.
struct OneAgentCatalog {
    record: SessionAgentRecord,
}

#[async_trait]
impl AgentCatalog for OneAgentCatalog {
    async fn record_for(&self, agent_id: &str) -> Result<SessionAgentRecord, Status> {
        match agent_id == self.record.agent_id {
            true => Ok(self.record.clone()),
            false => Err(Status::not_found(format!("no agent '{agent_id}'"))),
        }
    }
}

/// Every port an attach does not reach. An attach that reached one would name it, not succeed.
struct NotReachedByAttach;

#[async_trait]
impl AgentSessions for NotReachedByAttach {
    fn hosts_a_clone_for(&self, _session_id: &str) -> bool {
        false
    }

    async fn open_owned(
        &self,
        _session_id: &str,
        _agent_id: &str,
        _system_prompt: Option<&str>,
    ) -> Result<Option<Box<dyn SubagentSession>>, Status> {
        Err(Status::internal("an attach opens no conversation"))
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
        Err(Status::internal("an attach opens no conversation"))
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

#[async_trait]
impl AgentConversationPeers for NotReachedByAttach {
    async fn open(
        &self,
        _request: &tddy_service::proto::session_agents_svc::OpenAgentConversationRequest,
        owner: &str,
        _conversation_id: &str,
    ) -> Result<(), Status> {
        Err(Status::internal(format!(
            "an attach forwards nothing to '{owner}'"
        )))
    }

    async fn prompt(
        &self,
        _request: &tddy_service::proto::session_agents_svc::PromptAgentConversationRequest,
        owner: &str,
    ) -> Result<
        tokio::sync::mpsc::UnboundedReceiver<
            Result<tddy_service::proto::session_agents_svc::AgentConversationChunk, Status>,
        >,
        Status,
    > {
        Err(Status::internal(format!(
            "an attach forwards nothing to '{owner}'"
        )))
    }

    async fn resume(
        &self,
        _request: &tddy_service::proto::session_agents_svc::ResumeAgentConversationRequest,
        owner: &str,
    ) -> Result<
        tokio::sync::mpsc::UnboundedReceiver<
            Result<tddy_service::proto::session_agents_svc::AgentConversationChunk, Status>,
        >,
        Status,
    > {
        Err(Status::internal(format!(
            "an attach forwards nothing to '{owner}'"
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
            "an attach forwards nothing to '{owner}'"
        )))
    }
}

struct NoRoomBroadcast;

#[async_trait]
impl RosterBroadcast for NoRoomBroadcast {
    async fn broadcast(&self, _session_id: &str, _roster: &SessionAgentRoster) {}
}

/// A daemon serving one session, offering one agent, and recording the admissions its attaches ask
/// for.
struct ADaemonWithASession {
    _sessions: tempfile::TempDir,
    service: SessionAgentServiceImpl,
    rosters: Arc<SessionAgentRosterStore>,
    session_dir: std::path::PathBuf,
    admission: Arc<RecordingAdmission>,
}

fn an_agent_this_daemon_runs() -> SessionAgentRecord {
    SessionAgentRecord {
        agent_id: AN_AGENT.to_string(),
        name: "explorer".to_string(),
        daemon_instance_id: THIS_DAEMON.to_string(),
        label: None,
        model: "stub-model".to_string(),
        replaces: Vec::new(),
        tools: Vec::new(),
        codebase_session_id: None,
        kind: AgentKindRecord::Model,
        harness: None,
        placement: Default::default(),
    }
}

fn a_daemon_with_a_session() -> ADaemonWithASession {
    let sessions = tempfile::tempdir().expect("sessions dir");
    let session_dir = sessions.path().join(SESSION);
    std::fs::create_dir_all(&session_dir).expect("session dir");
    let metadata = a_session_metadata().with_session_id(SESSION).build();
    tddy_core::write_session_metadata(&session_dir, &metadata).expect("session metadata");

    let clones = Arc::new(SessionAgentCloneStore::new());
    let rosters = Arc::new(SessionAgentRosterStore::new(
        Arc::clone(&clones),
        Arc::new(SessionAgentActivityStore::new()),
    ));
    let admission = Arc::new(RecordingAdmission::default());
    let service = SessionAgentServiceImpl::new(SessionAgentPorts {
        session_dirs: Arc::new({
            let dir = session_dir.clone();
            move |_token, _session_id| Ok(dir.clone())
        }),
        local_instance_id: THIS_DAEMON.to_string(),
        roster_keepalive: std::time::Duration::from_secs(60),
        rosters: Arc::clone(&rosters),
        clones,
        conversations: Arc::new(OpenAgentConversations::new()),
        admission: admission.clone(),
        catalog: Arc::new(OneAgentCatalog {
            record: an_agent_this_daemon_runs(),
        }),
        broadcast: Arc::new(NoRoomBroadcast),
        sessions: Arc::new(NotReachedByAttach),
        peers: Arc::new(NotReachedByAttach),
        conversation_frames: Arc::new(
            tddy_daemon_kernel::conversation_frames::ConversationFrames::default(),
        ),
    });
    ADaemonWithASession {
        _sessions: sessions,
        service,
        rosters,
        session_dir,
        admission,
    }
}

impl ADaemonWithASession {
    async fn attach(&self, placement: AgentPlacement) -> Result<SessionAgentRoster, Status> {
        self.service
            .attach_session_agent(Request::direct(AttachSessionAgentRequest {
                session_token: SESSION_TOKEN.to_string(),
                session_id: SESSION.to_string(),
                daemon_instance_id: String::new(),
                agent_id: AN_AGENT.to_string(),
                placement: placement as i32,
            }))
            .await
            .map(|response| response.into_inner())
    }

    /// The roster as the session holds it now, read back from the store.
    fn roster_now(&self) -> SessionAgentRoster {
        self.rosters
            .snapshot(SESSION, &self.session_dir)
            .expect("the session's roster")
    }
}

/// What a roster holds of the one agent: its placement and the revision it is at.
fn placement_and_rev(roster: &SessionAgentRoster) -> (i32, u64) {
    let entry = roster
        .agents
        .iter()
        .find(|entry| entry.agent_id == AN_AGENT)
        .expect("the agent is attached");
    (entry.placement, roster.rev)
}

#[tokio::test]
async fn attaching_an_agent_shared_then_clone_is_refused_naming_its_shared_placement() {
    // Given
    let daemon = a_daemon_with_a_session();
    daemon
        .attach(AgentPlacement::Shared)
        .await
        .expect("the first attach");

    // When
    let refusal = daemon
        .attach(AgentPlacement::Clone)
        .await
        .expect_err("a second placement is refused");

    // Then
    assert_eq!(
        (
            refusal.code(),
            refusal.message().contains("shared placement"),
        ),
        (Code::FailedPrecondition, true),
        "{}",
        refusal.message()
    );
}

#[tokio::test]
async fn attaching_an_agent_clone_then_shared_is_refused_naming_its_clone_placement() {
    // Given
    let daemon = a_daemon_with_a_session();
    daemon
        .attach(AgentPlacement::Clone)
        .await
        .expect("the first attach");

    // When
    let refusal = daemon
        .attach(AgentPlacement::Shared)
        .await
        .expect_err("a second placement is refused");

    // Then
    assert_eq!(
        (
            refusal.code(),
            refusal.message().contains("clone placement"),
        ),
        (Code::FailedPrecondition, true),
        "{}",
        refusal.message()
    );
}

#[tokio::test]
async fn a_refused_placement_change_leaves_the_roster_unchanged_and_admits_nothing() {
    // Given
    let daemon = a_daemon_with_a_session();
    daemon
        .attach(AgentPlacement::Shared)
        .await
        .expect("the first attach");
    let admissions_before = daemon.admission.asked_about().len();

    // When
    let _ = daemon.attach(AgentPlacement::Clone).await;

    // Then
    assert_eq!(
        (
            placement_and_rev(&daemon.roster_now()),
            daemon.admission.asked_about().len() - admissions_before,
        ),
        ((AgentPlacement::Shared as i32, 1), 0)
    );
}

#[tokio::test]
async fn attaching_the_same_placement_again_returns_the_roster_at_the_same_revision() {
    // Given
    let daemon = a_daemon_with_a_session();
    daemon
        .attach(AgentPlacement::Clone)
        .await
        .expect("the first attach");

    // When
    let roster = daemon
        .attach(AgentPlacement::Clone)
        .await
        .expect("the same placement is accepted");

    // Then
    assert_eq!(
        (placement_and_rev(&roster), roster.agents.len()),
        ((AgentPlacement::Clone as i32, 1), 1)
    );
}

#[test]
fn the_roster_store_refuses_a_second_placement_for_an_attached_agent() {
    // Given
    let daemon = a_daemon_with_a_session();
    let attached = an_agent_this_daemon_runs();
    daemon
        .rosters
        .attach(SESSION, &daemon.session_dir, attached.clone())
        .expect("the first attach");
    let mut clone = attached;
    clone.placement = tddy_core::AgentPlacementRecord::Clone;

    // When
    let refusal = daemon
        .rosters
        .attach(SESSION, &daemon.session_dir, clone)
        .expect_err("a second placement is refused");

    // Then
    assert_eq!(
        (refusal.code(), placement_and_rev(&daemon.roster_now()),),
        (Code::FailedPrecondition, (AgentPlacement::Shared as i32, 1))
    );
}

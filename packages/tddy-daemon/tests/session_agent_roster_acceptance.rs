//! Acceptance tests: attaching and detaching agents on a live session
//! (`AttachSessionAgent`, `DetachSessionAgent`, `ListSessionAgents`, `StreamSessionAgents`).
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (AC1-AC12)
//!
//! One daemon, real YAML def sources, real session directories, no LiveKit and no peer. That is
//! deliberate: everything about *arity, identity, revisioning and persistence* is decidable on one
//! host, and pushing it into the two-daemon production suite would make the cheap half of this
//! feature cost a room to test. What genuinely needs a second daemon lives in
//! `session_agent_remote_acceptance.rs`.
//!
//! The qualified id a test attaches is never spelled by hand — it is read from `ListSubagents`,
//! the same way the web reads it. A hardcoded `"explorer@some-host"` would pass while the daemon
//! stamped something else entirely.

use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use pretty_assertions::assert_eq;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    AttachSessionAgentRequest, ConnectionService as ConnectionServiceTrait,
    DetachSessionAgentRequest, ListSessionAgentsRequest, ListSubagentsRequest,
    ReportAgentCloneStateRequest, SessionAgentRoster, StreamSessionAgentsRequest,
};

/// `AgentCloneState::READY`, as the proto numbers it.
const CLONE_STATE_READY: i32 = 3;

/// The keepalive cadence the keepalive tests run the service at.
///
/// Short enough to observe inside the integration budget, long enough that a scheduling hiccup on a
/// loaded runner cannot land a keepalive between an attach and the frame it publishes.
const A_BRISK_KEEPALIVE: Duration = Duration::from_millis(50);

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A daemon serving one session, with an `<tddyhome>/agents/` directory the test seeds.
struct RosteredSession {
    service: ConnectionServiceImpl,
    session_id: String,
    _sessions: tempfile::TempDir,
}

impl RosteredSession {
    async fn attach(&self, agent_id: &str) -> Result<SessionAgentRoster, tddy_rpc::Status> {
        self.service
            .attach_session_agent(Request::new(AttachSessionAgentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    async fn detach(&self, agent_id: &str) -> Result<SessionAgentRoster, tddy_rpc::Status> {
        self.service
            .detach_session_agent(Request::new(DetachSessionAgentRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
                agent_id: agent_id.to_string(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    async fn list(&self) -> SessionAgentRoster {
        self.service
            .list_session_agents(Request::new(ListSessionAgentsRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.session_id.clone(),
                daemon_instance_id: String::new(),
            }))
            .await
            .expect("listing a session's agents must succeed")
            .into_inner()
    }

    /// The qualified id this daemon advertises for a def named `name`, read the way a client reads
    /// it rather than spelled by hand.
    async fn agent_id_for(&self, name: &str) -> String {
        let subagents = self
            .service
            .list_subagents(Request::new(ListSubagentsRequest {}))
            .await
            .expect("listing subagents must succeed")
            .into_inner()
            .subagents;
        subagents
            .into_iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("this daemon must advertise a def named '{name}'"))
            .agent_id
    }

    /// The local daemon's own instance id, taken from a stamped `ListSubagents` row.
    async fn local_daemon_instance_id(&self) -> String {
        let subagents = self
            .service
            .list_subagents(Request::new(ListSubagentsRequest {}))
            .await
            .expect("listing subagents must succeed")
            .into_inner()
            .subagents;
        subagents
            .first()
            .expect("the fixture must define at least one agent to read the daemon id from")
            .daemon_instance_id
            .clone()
    }

    /// Re-read the roster from disk through a *fresh* service over the same sessions base — what a
    /// daemon restart sees.
    fn after_restart(&self) -> ConnectionServiceImpl {
        test_service(self._sessions.path().to_path_buf())
    }
}

/// A managed claude-cli session on a daemon whose agents directory defines `agents`.
fn a_session_with_agents_available(agents: &[(&str, &str)]) -> RosteredSession {
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    write_agent_defs(sessions.path(), agents);

    let session_id = "1780828020298-roster".to_string();
    let session_dir = unified_session_dir_path(sessions.path(), &session_id);
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    tddy_core::write_session_metadata(&session_dir, &a_managed_session(&session_id))
        .expect("write session metadata");

    RosteredSession {
        service: test_service(sessions.path().to_path_buf()),
        session_id,
        _sessions: sessions,
    }
}

/// The same session, on a daemon whose roster subscriptions re-send at `interval` instead of the
/// production cadence — so a keepalive can be observed without waiting for it.
fn a_session_whose_roster_keepalive_ticks_every(interval: Duration) -> RosteredSession {
    let RosteredSession {
        service,
        session_id,
        _sessions,
    } = a_session_with_agents_available(&[("explorer", "Grep"), ("linter", "")]);
    RosteredSession {
        service: service.with_roster_keepalive_interval(interval),
        session_id,
        _sessions,
    }
}

/// `<tddy-data-dir>/agents/<name>.yaml` for each named def. `test_service` uses the sessions base
/// as the data dir, so this is the directory the daemon resolves against.
fn write_agent_defs(tddy_data_dir: &Path, agents: &[(&str, &str)]) {
    let agents_dir = tddy_data_dir.join("agents");
    std::fs::create_dir_all(&agents_dir).expect("create agents dir");
    for (name, replaces) in agents {
        std::fs::write(
            agents_dir.join(format!("{name}.yaml")),
            format!(
                "name: {name}\nlabel: \"{name} (local)\"\nmodel: qwen2.5-coder:7b\n\
                 base_url: http://localhost:11434\ntools: [READ, GLOB, GREP]\n\
                 replaces: [{replaces}]\n"
            ),
        )
        .expect("write agent def");
    }
}

/// A managed-codebase claude-cli session — the type a `replaces`-carrying agent may attach to.
fn a_managed_session(session_id: &str) -> SessionMetadata {
    SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "project-under-roster".to_string(),
        created_at: "2026-08-16T10:00:00Z".to_string(),
        updated_at: "2026-08-16T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktrees/roster".to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: None,
        cursor_chat_id: None,
        activity_status: None,
        hook_token: None,
        sandbox: Some(true),
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
    }
}

/// Ten defs named `agent-00` … `agent-09`, none of them replacing anything.
fn ten_agent_defs() -> Vec<(String, String)> {
    (0..10)
        .map(|i| (format!("agent-{i:02}"), String::new()))
        .collect()
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

trait RosterAssertions {
    fn assert_agent_ids(&self, expected: &[&str]) -> &Self;
    fn assert_rev(&self, expected: u64) -> &Self;
    fn assert_replaces(&self, agent_id: &str, expected: &[&str]) -> &Self;
}

impl RosterAssertions for SessionAgentRoster {
    fn assert_agent_ids(&self, expected: &[&str]) -> &Self {
        let actual: Vec<&str> = self.agents.iter().map(|a| a.agent_id.as_str()).collect();
        assert_eq!(actual, expected, "roster membership and order mismatch");
        self
    }

    fn assert_rev(&self, expected: u64) -> &Self {
        assert_eq!(self.rev, expected, "roster revision mismatch");
        self
    }

    fn assert_replaces(&self, agent_id: &str, expected: &[&str]) -> &Self {
        let entry = self
            .agents
            .iter()
            .find(|a| a.agent_id == agent_id)
            .unwrap_or_else(|| panic!("roster has no entry for '{agent_id}'"));
        assert_eq!(
            entry.replaces, expected,
            "replaced set mismatch for {agent_id}"
        );
        self
    }
}

/// Assert a call was refused with `code`, and that the message names `fragment` — a refusal that
/// does not say which id it refused sends the operator back to the daemon log.
fn assert_refused(
    result: Result<SessionAgentRoster, tddy_rpc::Status>,
    code: Code,
    fragment: &str,
) {
    let status = result.expect_err("the call must be refused");
    assert_eq!(status.code(), code, "refusal code mismatch: {status:?}");
    assert!(
        status.message().contains(fragment),
        "refusal must name '{fragment}', was: {}",
        status.message()
    );
}

// ---------------------------------------------------------------------------
// AC1-AC5 — attaching
// ---------------------------------------------------------------------------

/// The base case: one attach, one entry, one revision. `rev` is what every consumer uses to tell a
/// stale registry from a current one, so it advancing by exactly one is part of the contract.
#[tokio::test]
async fn attaching_an_agent_adds_it_to_the_roster_and_advances_the_revision() {
    // Given
    let session = a_session_with_agents_available(&[("explorer", "Grep, Glob")]);
    let explorer = session.agent_id_for("explorer").await;

    // When
    let roster = session
        .attach(&explorer)
        .await
        .expect("attach must succeed");

    // Then
    roster
        .assert_agent_ids(&[&explorer])
        .assert_rev(1)
        .assert_replaces(&explorer, &["Grep", "Glob"]);
}

/// Attaching what is already attached is a no-op, not a duplicate and not an error. A revision
/// bump here would push a new roster to every subscriber for a change that did not happen — and an
/// operator double-clicking Add is the ordinary way to reach this.
#[tokio::test]
async fn attaching_the_same_agent_twice_leaves_the_revision_where_it_was() {
    // Given
    let session = a_session_with_agents_available(&[("explorer", "Grep")]);
    let explorer = session.agent_id_for("explorer").await;
    session.attach(&explorer).await.expect("first attach");

    // When
    let roster = session
        .attach(&explorer)
        .await
        .expect("re-attaching must be accepted, not refused");

    // Then
    roster.assert_agent_ids(&[&explorer]).assert_rev(1);
}

/// An id no def source resolves is a request error naming the id. Silently dropping it would start
/// the main agent short of the tool it was told to delegate.
#[tokio::test]
async fn refuses_an_agent_no_daemon_can_resolve_and_leaves_the_roster_alone() {
    // Given
    let session = a_session_with_agents_available(&[("explorer", "Grep")]);
    let local = session.local_daemon_instance_id().await;
    let unknown = format!("no-such-agent@{local}");

    // When
    let result = session.attach(&unknown).await;

    // Then
    assert_refused(result, Code::InvalidArgument, &unknown);
    session.list().await.assert_agent_ids(&[]).assert_rev(0);
}

/// A bare name names no daemon. There is deliberately no "assume the local one" reading — that is
/// the reading that quietly picks the wrong host the moment two daemons offer the same name.
#[tokio::test]
async fn refuses_an_agent_id_that_does_not_name_its_daemon() {
    // Given
    let session = a_session_with_agents_available(&[("explorer", "Grep")]);

    // When
    let result = session.attach("explorer").await;

    // Then
    assert_refused(result, Code::InvalidArgument, "explorer");
    session.list().await.assert_agent_ids(&[]);
}

/// "Unlimited" is the headline requirement, so it is asserted rather than assumed: ten agents
/// attach, all ten are in the roster, and the revision counted every one of them.
#[tokio::test]
async fn attaches_ten_agents_and_addresses_every_one_of_them() {
    // Given
    let defs = ten_agent_defs();
    let def_refs: Vec<(&str, &str)> = defs
        .iter()
        .map(|(name, replaces)| (name.as_str(), replaces.as_str()))
        .collect();
    let session = a_session_with_agents_available(&def_refs);

    let mut attached = Vec::new();
    for (name, _) in &defs {
        attached.push(session.agent_id_for(name).await);
    }

    // When
    for agent_id in &attached {
        session.attach(agent_id).await.expect("attach must succeed");
    }

    // Then
    let expected: Vec<&str> = attached.iter().map(String::as_str).collect();
    session
        .list()
        .await
        .assert_agent_ids(&expected)
        .assert_rev(10);
}

// ---------------------------------------------------------------------------
// AC6-AC7 — detaching
// ---------------------------------------------------------------------------

/// Detach removes exactly one entry and disturbs nothing else — including the order the others
/// were attached in, which is the order the main agent sees them listed.
#[tokio::test]
async fn detaching_one_agent_leaves_the_others_in_the_order_they_were_attached() {
    // Given
    let session = a_session_with_agents_available(&[
        ("explorer", "Grep"),
        ("linter", "ReadLints"),
        ("reviewer", ""),
    ]);
    let explorer = session.agent_id_for("explorer").await;
    let linter = session.agent_id_for("linter").await;
    let reviewer = session.agent_id_for("reviewer").await;
    session.attach(&explorer).await.expect("attach explorer");
    session.attach(&linter).await.expect("attach linter");
    session.attach(&reviewer).await.expect("attach reviewer");

    // When
    let roster = session.detach(&linter).await.expect("detach must succeed");

    // Then
    roster
        .assert_agent_ids(&[&explorer, &reviewer])
        .assert_rev(4);
}

/// Detaching something that was never attached is `NOT_FOUND`, not a silent success. A silent
/// success tells an operator a tool was restored to the main agent when it never was.
#[tokio::test]
async fn refuses_to_detach_an_agent_that_was_never_attached() {
    // Given
    let session = a_session_with_agents_available(&[("explorer", "Grep"), ("linter", "")]);
    let explorer = session.agent_id_for("explorer").await;
    let linter = session.agent_id_for("linter").await;
    session.attach(&explorer).await.expect("attach explorer");

    // When
    let result = session.detach(&linter).await;

    // Then
    assert_refused(result, Code::NotFound, &linter);
    session
        .list()
        .await
        .assert_agent_ids(&[&explorer])
        .assert_rev(1);
}

// ---------------------------------------------------------------------------
// AC8-AC9 — reading the roster
// ---------------------------------------------------------------------------

/// One roster, two ways of asking. A `List` that can disagree with the stream is a registry that
/// disagrees with the daemon, which is the failure this whole design is arranged against.
#[tokio::test]
async fn reports_the_same_roster_whether_it_is_listed_or_streamed() {
    // Given
    let session = a_session_with_agents_available(&[("explorer", "Grep")]);
    let explorer = session.agent_id_for("explorer").await;
    session.attach(&explorer).await.expect("attach explorer");

    // When
    let listed = session.list().await;
    let streamed = first_roster_frame(&session).await;

    // Then
    assert_eq!(
        listed.rev, streamed.rev,
        "list and stream disagree on revision"
    );
    assert_eq!(
        listed.agents.len(),
        streamed.agents.len(),
        "list and stream disagree on membership"
    );
    streamed.assert_agent_ids(&[&explorer]).assert_rev(1);
}

/// A late subscriber — the in-jail `tddy-tools` reconnecting, a browser tab opening — is handed the
/// current roster immediately. Without it, a reconnecting registry would serve an empty roster
/// until the next attach, which may never come.
#[tokio::test]
async fn hands_a_new_subscriber_the_current_roster_before_anything_changes() {
    // Given
    let session = a_session_with_agents_available(&[("explorer", "Grep"), ("linter", "")]);
    let explorer = session.agent_id_for("explorer").await;
    let linter = session.agent_id_for("linter").await;
    session.attach(&explorer).await.expect("attach explorer");
    session.attach(&linter).await.expect("attach linter");

    // When — subscribing after both attaches already happened
    let first = first_roster_frame(&session).await;

    // Then
    first.assert_agent_ids(&[&explorer, &linter]).assert_rev(2);
}

/// A subscriber sees the snapshot, then one frame per real change — and none for the no-op
/// re-attach in between, which is what keeps `rev` a usable staleness signal.
#[tokio::test]
async fn publishes_no_frame_for_an_attach_that_changed_nothing() {
    // Given
    let session = a_session_with_agents_available(&[("explorer", "Grep"), ("linter", "")]);
    let explorer = session.agent_id_for("explorer").await;
    let linter = session.agent_id_for("linter").await;
    session.attach(&explorer).await.expect("attach explorer");

    let mut stream = roster_stream(&session).await;
    let snapshot = next_frame(&mut stream).await;
    snapshot.assert_agent_ids(&[&explorer]).assert_rev(1);

    // When — a no-op re-attach, then a real one
    session.attach(&explorer).await.expect("re-attach explorer");
    session.attach(&linter).await.expect("attach linter");

    // Then — the next frame is the *linter* attach, so the no-op published nothing
    let next = next_frame(&mut stream).await;
    next.assert_agent_ids(&[&explorer, &linter]).assert_rev(2);
}

// ---------------------------------------------------------------------------
// Keepalive — a subscription nobody changes still produces frames
// ---------------------------------------------------------------------------

/// A roster nobody is changing must still produce frames, because a silent stream and a dead one
/// look identical to anything between the publisher and the subscriber. A split session's
/// subscription is forwarded across a relay that terminates a stream which stops producing, and
/// re-sending the roster already applied is the one thing that costs a subscriber nothing: applying
/// the revision it holds is a defined no-op that announces no change.
#[tokio::test]
async fn re_sends_the_roster_it_last_sent_when_nothing_changes() {
    // Given
    let session = a_session_whose_roster_keepalive_ticks_every(A_BRISK_KEEPALIVE);
    let explorer = session.agent_id_for("explorer").await;
    session.attach(&explorer).await.expect("attach explorer");
    let mut stream = roster_stream(&session).await;
    next_frame(&mut stream).await.assert_rev(1);

    // When — the next frame arrives with nothing attached or detached in between
    let after_the_cadence = next_frame(&mut stream).await;

    // Then
    after_the_cadence
        .assert_agent_ids(&[&explorer])
        .assert_rev(1);
}

/// One frame would only move the deadline once. The subscription lives as long as the session, so
/// the cadence has to as well.
#[tokio::test]
async fn keeps_re_sending_the_roster_for_as_long_as_the_subscription_lives() {
    // Given
    let session = a_session_whose_roster_keepalive_ticks_every(A_BRISK_KEEPALIVE);
    let explorer = session.agent_id_for("explorer").await;
    session.attach(&explorer).await.expect("attach explorer");
    let mut stream = roster_stream(&session).await;
    next_frame(&mut stream).await.assert_rev(1);

    // When — three more frames arrive with nothing attached or detached in between
    let revs = vec![
        next_frame(&mut stream).await.rev,
        next_frame(&mut stream).await.rev,
        next_frame(&mut stream).await.rev,
    ];

    // Then
    assert_eq!(
        revs,
        vec![1, 1, 1],
        "every keepalive must carry the revision the subscriber already holds"
    );
}

/// The keepalive carries the roster the subscription last *sent*, not the one it opened with. A
/// reconnecting subscriber that missed the change and then reads a keepalive would otherwise be told
/// the roster is the one from before it — a stale answer with a current-looking revision.
#[tokio::test]
async fn re_sends_the_change_it_last_published_rather_than_the_snapshot_it_opened_with() {
    // Given
    let session = a_session_whose_roster_keepalive_ticks_every(A_BRISK_KEEPALIVE);
    let explorer = session.agent_id_for("explorer").await;
    let linter = session.agent_id_for("linter").await;
    session.attach(&explorer).await.expect("attach explorer");
    let mut stream = roster_stream(&session).await;
    next_frame(&mut stream).await.assert_rev(1);

    // When
    session.attach(&linter).await.expect("attach linter");
    next_frame(&mut stream).await.assert_rev(2);

    // Then
    next_frame(&mut stream)
        .await
        .assert_agent_ids(&[&explorer, &linter])
        .assert_rev(2);
}

/// The relay deadline and the subscriber's service threshold are one design decision held in two
/// crates. A subscriber classifies a roster pass by how long it lasted: a pass the relay's idle
/// deadline killed lasted at least that deadline (its connect time is on top), so while the
/// deadline is the longer of the two, a keepalive path that goes quiet costs one prompt reconnect
/// per deadline — service. Make the deadline the shorter one and every such teardown reads as
/// churn instead, throttling the roster to its reconnect ceiling and, from there, refusing every
/// subagent call for a session whose stream is in fact being served. Pinned by a test rather than a
/// `const` assert because `tddy-tools` is only a dev-dependency of this crate, so the lib cannot
/// name the threshold.
#[test]
fn tears_a_forwarded_stream_down_no_faster_than_a_pass_needs_to_last_to_count_as_service() {
    // Given
    let relay_gives_up_after =
        tddy_daemon::livekit_peer_discovery::PEER_FORWARD_STREAM_IDLE_TIMEOUT;

    // When
    let a_pass_counts_as_service_after = tddy_tools::session_agents::PASS_LONG_ENOUGH_TO_BE_SERVICE;

    // Then
    assert!(
        relay_gives_up_after >= a_pass_counts_as_service_after,
        "a relay that gives up after {relay_gives_up_after:?} tears down every forwarded roster \
         stream before the {a_pass_counts_as_service_after:?} that makes a pass count as service, \
         so a cross-host roster would throttle to its ceiling and refuse calls it can serve"
    );
}

// ---------------------------------------------------------------------------
// AC10-AC11 — persistence
// ---------------------------------------------------------------------------

/// A roster is operator intent, so it survives the process. Restarting must also continue the
/// revision rather than restarting it at zero — a subscriber holding `rev: 3` would otherwise read
/// a fresh `rev: 1` as stale and never refresh.
#[tokio::test]
async fn restores_the_roster_and_its_revision_after_the_daemon_restarts() {
    // Given
    let session = a_session_with_agents_available(&[("explorer", "Grep"), ("linter", "")]);
    let explorer = session.agent_id_for("explorer").await;
    let linter = session.agent_id_for("linter").await;
    session.attach(&explorer).await.expect("attach explorer");
    session.attach(&linter).await.expect("attach linter");

    // When
    let restarted = session.after_restart();
    let roster = restarted
        .list_session_agents(Request::new(ListSessionAgentsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session.session_id.clone(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("listing after a restart must succeed")
        .into_inner();

    // Then
    roster
        .assert_agent_ids(&[&explorer, &linter])
        .assert_rev(2)
        .assert_replaces(&explorer, &["Grep"]);
}

/// A session written before rosters existed reads as having none. Its bare `specialized_agents`
/// names cannot be promoted, because nothing records which daemon each belonged to — and guessing
/// "the local one" is how a resume silently runs a different agent.
#[tokio::test]
async fn reads_a_session_written_before_rosters_existed_as_having_no_agents() {
    // Given
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    write_agent_defs(sessions.path(), &[("explorer", "Grep")]);
    let session_id = "1780828020298-legacy".to_string();
    let session_dir = unified_session_dir_path(sessions.path(), &session_id);
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    std::fs::write(
        session_dir.join(".session.yaml"),
        "session_id: 1780828020298-legacy\nproject_id: proj-legacy\n\
         created_at: \"2026-01-01T00:00:00Z\"\nupdated_at: \"2026-01-01T00:00:00Z\"\n\
         status: active\nsession_type: claude-cli\n\
         specialized_agents:\n  - fastcontext\n",
    )
    .expect("write legacy session metadata");
    let service = test_service(sessions.path().to_path_buf());

    // When
    let roster = service
        .list_session_agents(Request::new(ListSessionAgentsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.clone(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("a pre-roster session must still be readable")
        .into_inner();

    // Then
    roster.assert_agent_ids(&[]).assert_rev(0);
}

// ---------------------------------------------------------------------------
// AC12 — authorization
// ---------------------------------------------------------------------------

/// An unauthenticated attach is refused before anything happens. "Before" is the load-bearing
/// word: attaching a remote agent contacts a peer and provisions a checkout on it, so an auth
/// check that ran afterwards would let an unauthenticated caller build a clone on another host.
#[tokio::test]
async fn refuses_an_unauthenticated_roster_call_before_contacting_any_peer() {
    // Given
    let session = a_session_with_agents_available(&[("explorer", "Grep")]);
    let explorer = session.agent_id_for("explorer").await;

    // When
    let result = session
        .service
        .attach_session_agent(Request::new(AttachSessionAgentRequest {
            session_token: "not-a-valid-token".to_string(),
            session_id: session.session_id.clone(),
            daemon_instance_id: String::new(),
            agent_id: explorer.clone(),
        }))
        .await
        .map(|r| r.into_inner());

    // Then
    let status = result.expect_err("an unauthenticated attach must be refused");
    assert_eq!(status.code(), Code::Unauthenticated);
    session.list().await.assert_agent_ids(&[]).assert_rev(0);
}

/// The same gate on the read path — a roster names the hosts a session reaches, so it is not
/// public information.
#[tokio::test]
async fn refuses_an_unauthenticated_read_of_the_roster() {
    // Given
    let session = a_session_with_agents_available(&[("explorer", "Grep")]);

    // When
    let result = session
        .service
        .list_session_agents(Request::new(ListSessionAgentsRequest {
            session_token: "not-a-valid-token".to_string(),
            session_id: session.session_id.clone(),
            daemon_instance_id: String::new(),
        }))
        .await
        .map(|r| r.into_inner());

    // Then
    let status = result.expect_err("an unauthenticated read must be refused");
    assert_eq!(status.code(), Code::Unauthenticated);
}

/// A session id is a directory name under the sessions base, and an attach read-modify-writes the
/// `.session.yaml` it finds there. An id that climbs out of that directory would have one session's
/// attach rewrite another session's file — including one belonging to another user.
#[tokio::test]
async fn refuses_a_session_id_that_climbs_out_of_the_sessions_directory() {
    // Given — a session whose file sits outside the daemon's `sessions/` directory
    let session = a_session_with_agents_available(&[("explorer", "Grep")]);
    let explorer = session.agent_id_for("explorer").await;
    let victim_dir = session._sessions.path().join("victim");
    std::fs::create_dir_all(&victim_dir).expect("create the victim's session dir");
    tddy_core::write_session_metadata(&victim_dir, &a_managed_session("victim"))
        .expect("write the victim's session metadata");

    // When
    let result = session
        .service
        .attach_session_agent(Request::new(AttachSessionAgentRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: "../victim".to_string(),
            daemon_instance_id: String::new(),
            agent_id: explorer.clone(),
        }))
        .await
        .map(|r| r.into_inner());

    // Then
    assert_refused(result, Code::InvalidArgument, "session_id");
    let victim = tddy_core::read_session_metadata(&victim_dir)
        .expect("the victim's session file must still be readable");
    assert_eq!(
        victim.agents.len(),
        0,
        "an attach must not be able to write an agent into another session's file"
    );
}

/// A clone's readiness is what authorizes an entry to start serving prompts, and the triple the
/// report is matched on — session, daemon, checkout — is published in the roster every subscriber
/// receives. Without a credential, anyone who saw one frame could report a still-provisioning clone
/// READY and have the next prompt served from an empty checkout.
#[tokio::test]
async fn refuses_an_unauthenticated_clone_state_report() {
    // Given
    let session = a_session_with_agents_available(&[("explorer", "Grep")]);

    // When
    let result = session
        .service
        .report_agent_clone_state(Request::new(ReportAgentCloneStateRequest {
            session_token: "not-a-valid-token".to_string(),
            session_id: session.session_id.clone(),
            daemon_instance_id: "some-peer".to_string(),
            codebase_session_id: "a-checkout-this-daemon-never-asked-for".to_string(),
            clone_state: CLONE_STATE_READY,
            clone_error: String::new(),
            worktree_path: String::new(),
            divergences: Vec::new(),
        }))
        .await
        .map(|r| r.into_inner());

    // Then — refused for the credential, not for the triple: the triple is public
    let status = result.expect_err("an unauthenticated clone report must be refused");
    assert_eq!(status.code(), Code::Unauthenticated);
}

// ---------------------------------------------------------------------------
// Stream helpers
// ---------------------------------------------------------------------------

async fn roster_stream(
    session: &RosteredSession,
) -> impl futures_util::Stream<Item = Result<SessionAgentRoster, tddy_rpc::Status>> + Unpin {
    session
        .service
        .stream_session_agents(Request::new(StreamSessionAgentsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session.session_id.clone(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("subscribing to the roster must succeed")
        .into_inner()
}

async fn first_roster_frame(session: &RosteredSession) -> SessionAgentRoster {
    let mut stream = roster_stream(session).await;
    next_frame(&mut stream).await
}

/// The next frame, or a failure naming what never arrived. Bounded rather than awaited forever:
/// the whole point of the snapshot-first rule is that a frame is already there.
async fn next_frame<S>(stream: &mut S) -> SessionAgentRoster
where
    S: futures_util::Stream<Item = Result<SessionAgentRoster, tddy_rpc::Status>> + Unpin,
{
    // 2s: a local in-process broadcast, well inside the integration budget. Long enough that a
    // loaded CI runner does not turn a scheduling delay into a failure.
    tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
        .await
        .expect("the roster stream produced no frame")
        .expect("the roster stream ended instead of producing a frame")
        .expect("the roster stream produced an error frame")
}

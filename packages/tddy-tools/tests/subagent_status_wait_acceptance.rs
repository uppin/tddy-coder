//! Acceptance tests: `subagent_status { waitFor: "ready" }` — the bounded wait that lets the main
//! agent stop guessing when a freshly attached agent can be prompted.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (AC68-AC72)
//! Changeset: docs/dev/1-WIP/2026-08-29-subagent-status-wait.md
//!
//! `subagent_status` already reports what every agent is doing. What it could not do is *tell the
//! main agent when that changes*: an attach returns before the agent is usable, a prompt sent while
//! its checkout is still provisioning is refused naming the clone state (AC33), and polling a tool
//! in a turn loop costs a turn per poll. The wait closes that gap, and the two properties worth
//! pinning are that it wakes on the frame that matters and that it can never park unboundedly.
//!
//! Frames are pushed straight into the process-wide roster, as the sibling roster acceptance files
//! do: the transport is covered where it is implemented, and a real stream would only make these
//! non-deterministic. Time is virtual (`start_paused`), so a deadline and the frame that beats it
//! are ordered by the runtime rather than by a sleep whose length a test would have to guess.
//! Every test is `#[serial]` because the registry is process-wide.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use serial_test::serial;
use tddy_service::proto::connection::{
    AgentCloneState, SessionAgentEntry, SessionAgentRoster, SessionAgentStatus,
};
use tddy_tools::server::PermissionServer;
use tddy_tools::session_agents::session_agent_roster;

/// Long enough that reaching it means the wait never woke. Virtual, so it costs nothing either way.
const A_DEADLINE_NO_HEALTHY_WAIT_REACHES: u64 = 5_000;

/// A deadline the agent under test will not become ready inside.
const A_DEADLINE_THAT_PASSES: u64 = 200;

/// Longer than the cap, so a wait honouring it verbatim would hold the main agent for ten minutes.
const A_DEADLINE_LONGER_THAN_THE_CAP: u64 = 600_000;

/// The cap every wait is held to, however large `timeoutMs` is.
const THE_LONGEST_ANY_WAIT_MAY_PARK: std::time::Duration =
    std::time::Duration::from_millis(120_000);

/// How long a frame that is on its way takes to arrive — ordering it after the wait has parked.
const WHILE_THE_CALL_IS_PARKED: std::time::Duration = std::time::Duration::from_millis(50);

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// One roster agent. A bare `an_agent()` is attached, local and idle — the state every wait is
/// waiting *for*, so a test only ever spells out how it differs from promptable.
struct AgentBuilder(SessionAgentEntry);

fn an_agent(agent_id: &str) -> AgentBuilder {
    let (name, daemon) = agent_id
        .split_once('@')
        .expect("builder was given a qualified agent id");
    AgentBuilder(SessionAgentEntry {
        agent_id: agent_id.to_string(),
        name: name.to_string(),
        daemon_instance_id: daemon.to_string(),
        label: format!("{name} (local)"),
        model: "qwen2.5-coder:7b".to_string(),
        replaces: Vec::new(),
        tools: vec!["Read".to_string(), "Glob".to_string(), "Grep".to_string()],
        codebase_session_id: String::new(),
        clone_state: AgentCloneState::Local as i32,
        clone_error: String::new(),
        status: SessionAgentStatus::Idle as i32,
        last_activity: None,
    })
}

impl AgentBuilder {
    /// Attached, but its checkout is still being built — so a prompt would be refused.
    fn still_provisioning_its_checkout(mut self) -> Self {
        self.0.clone_state = AgentCloneState::Provisioning as i32;
        self.0.status = SessionAgentStatus::Connecting as i32;
        self
    }

    /// Its checkout failed: listed, and unable to serve a prompt at all.
    fn with_a_failed_checkout(mut self, reason: &str) -> Self {
        self.0.clone_state = AgentCloneState::Error as i32;
        self.0.clone_error = reason.to_string();
        self.0.status = SessionAgentStatus::Error as i32;
        self
    }

    /// A turn is in flight.
    fn mid_turn(mut self) -> Self {
        self.0.status = SessionAgentStatus::Running as i32;
        self
    }

    /// A roster restored from `.session.yaml` has nothing to say about its agents.
    fn restored_from_disk(mut self) -> Self {
        self.0.status = SessionAgentStatus::Unspecified as i32;
        self
    }

    fn build(self) -> SessionAgentEntry {
        self.0
    }
}

// ---------------------------------------------------------------------------
// Publishing
// ---------------------------------------------------------------------------

/// The revision most recently published to this process's roster.
///
/// The registry is process-wide and only moves forward, so a test publishing a literal revision
/// would be ignored whenever an earlier one had already published higher — which would make these
/// tests depend on the order they run in. Each takes the next, and the republish helper reuses the
/// one in force.
static LAST_PUBLISHED_REV: AtomicU64 = AtomicU64::new(0);

/// Serializes the read-modify-write of `LAST_PUBLISHED_REV` against the spawned publisher tasks.
static PUBLISHING: Mutex<()> = Mutex::new(());

/// Publish a roster that **changed** — an attach or a detach — at the next revision.
fn publish(agents: Vec<SessionAgentEntry>) {
    let _guard = PUBLISHING.lock().expect("publishing");
    let rev = LAST_PUBLISHED_REV.fetch_add(1, Ordering::SeqCst) + 1;
    apply(rev, agents);
}

/// Publish a roster whose **membership is unchanged** at the revision already in force — what the
/// daemon does when nothing but a status or a clone state moved.
fn republish_at_the_revision_already_applied(agents: Vec<SessionAgentEntry>) {
    let _guard = PUBLISHING.lock().expect("publishing");
    let rev = LAST_PUBLISHED_REV.load(Ordering::SeqCst);
    apply(rev, agents);
}

fn apply(rev: u64, agents: Vec<SessionAgentEntry>) {
    session_agent_roster().apply_snapshot(SessionAgentRoster {
        session_id: "1780828020298-roster".to_string(),
        rev,
        agents,
    });
}

// ---------------------------------------------------------------------------
// Calling the tool
// ---------------------------------------------------------------------------

/// Invoke `subagent_status` exactly as the main agent does, and parse its result.
///
/// A refusal is a **successful** call carrying `{error, is_error}` — the envelope every subagent
/// tool uses — so this never has to branch on how the call went.
async fn subagent_status(args: Value) -> Value {
    let result = PermissionServer::new()
        .call_tool_by_name("subagent_status", args)
        .await
        .expect("subagent_status must be dispatchable by name");
    serde_json::from_str(&result)
        .unwrap_or_else(|e| panic!("subagent_status returned invalid JSON {result:?}: {e}"))
}

/// Ask to be told when `agent` can be prompted, giving up after `timeout_ms`.
async fn waiting_for_readiness_of(agent: &str, timeout_ms: u64) -> Value {
    subagent_status(json!({
        "agent": agent,
        "waitFor": "ready",
        "timeoutMs": timeout_ms,
    }))
    .await
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

trait WaitResultAssertions {
    fn assert_timed_out(&self, expected: bool) -> &Self;
    fn assert_reports(&self, agent_id: &str, status: &str) -> &Self;
    fn assert_lists_no_agent(&self, agent_id: &str) -> &Self;
    fn assert_refused_naming(&self, fragment: &str) -> &Self;
}

impl WaitResultAssertions for Value {
    fn assert_timed_out(&self, expected: bool) -> &Self {
        assert_eq!(self["timedOut"], json!(expected), "timedOut mismatch");
        self
    }

    fn assert_reports(&self, agent_id: &str, status: &str) -> &Self {
        let row = self["agents"]
            .as_array()
            .expect("a status report always carries an agents array")
            .iter()
            .find(|row| row["agentId"] == json!(agent_id))
            .unwrap_or_else(|| panic!("no row for '{agent_id}' in {self}"));
        assert_eq!(row["status"], json!(status), "status of '{agent_id}'");
        self
    }

    fn assert_lists_no_agent(&self, agent_id: &str) -> &Self {
        let listed = self["agents"]
            .as_array()
            .expect("a status report always carries an agents array")
            .iter()
            .any(|row| row["agentId"] == json!(agent_id));
        assert!(!listed, "'{agent_id}' must not be listed, was: {self}");
        self
    }

    fn assert_refused_naming(&self, fragment: &str) -> &Self {
        assert_eq!(
            self["is_error"],
            json!(true),
            "the call must be refused, was: {self}"
        );
        let message = self["error"].as_str().unwrap_or_default();
        assert!(
            message.contains(fragment),
            "refusal must name '{fragment}', was: {message}"
        );
        self
    }
}

// ---------------------------------------------------------------------------
// AC68 — the wait wakes on the frame that matters
// ---------------------------------------------------------------------------

/// The headline: the main agent attaches, asks to be told when the agent can be prompted, and is
/// told as soon as the frame saying so arrives — rather than sitting out its deadline, or finding
/// out by having a prompt refused.
#[tokio::test(start_paused = true)]
#[serial]
async fn returns_as_soon_as_a_later_frame_reports_the_waited_on_agent_ready() {
    // Given
    publish(vec![an_agent("explorer@ws-01")
        .still_provisioning_its_checkout()
        .build()]);
    tokio::spawn(async {
        tokio::time::sleep(WHILE_THE_CALL_IS_PARKED).await;
        publish(vec![an_agent("explorer@ws-01").build()]);
    });

    // When
    let report =
        waiting_for_readiness_of("explorer@ws-01", A_DEADLINE_NO_HEALTHY_WAIT_REACHES).await;

    // Then
    report
        .assert_timed_out(false)
        .assert_reports("explorer@ws-01", "idle");
}

/// The property the whole wait rests on. A status change republishes at the revision **already
/// applied** — `rev` moves on attach and detach, not on a badge — so a wait watching revisions
/// alone would park until its deadline on precisely the transition it exists to catch.
#[tokio::test(start_paused = true)]
#[serial]
async fn returns_when_the_readiness_arrives_at_the_revision_already_applied() {
    // Given
    publish(vec![an_agent("explorer@ws-01")
        .still_provisioning_its_checkout()
        .build()]);
    tokio::spawn(async {
        tokio::time::sleep(WHILE_THE_CALL_IS_PARKED).await;
        republish_at_the_revision_already_applied(vec![an_agent("explorer@ws-01").build()]);
    });

    // When
    let report =
        waiting_for_readiness_of("explorer@ws-01", A_DEADLINE_NO_HEALTHY_WAIT_REACHES).await;

    // Then
    report
        .assert_timed_out(false)
        .assert_reports("explorer@ws-01", "idle");
}

// ---------------------------------------------------------------------------
// AC69 — what already counts as settled
// ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
#[serial]
async fn returns_immediately_for_an_agent_that_is_already_promptable() {
    // Given
    publish(vec![an_agent("librarian@ws-02").mid_turn().build()]);

    // When
    let report =
        waiting_for_readiness_of("librarian@ws-02", A_DEADLINE_NO_HEALTHY_WAIT_REACHES).await;

    // Then
    report
        .assert_timed_out(false)
        .assert_reports("librarian@ws-02", "running");
}

/// An agent nothing has been observed of is reported `unknown`, and `unknown` settles the wait. It
/// is what a restarted daemon says about an agent whose checkout is on disk and perfectly
/// promptable, so treating it as not-ready would park every wait on such a session until its
/// deadline.
#[tokio::test(start_paused = true)]
#[serial]
async fn treats_an_agent_the_daemon_has_nothing_to_say_about_as_settled() {
    // Given
    publish(vec![an_agent("explorer@ws-01")
        .restored_from_disk()
        .build()]);

    // When
    let report =
        waiting_for_readiness_of("explorer@ws-01", A_DEADLINE_NO_HEALTHY_WAIT_REACHES).await;

    // Then
    report
        .assert_timed_out(false)
        .assert_reports("explorer@ws-01", "unknown");
}

/// A failed checkout is not something waiting fixes, so the wait ends on it and reports the state.
/// Parking to the deadline would report the same failure later, and call it a timeout.
#[tokio::test(start_paused = true)]
#[serial]
async fn stops_waiting_on_an_agent_whose_checkout_failed() {
    // Given
    publish(vec![an_agent("explorer@ws-01")
        .with_a_failed_checkout("clone failed: authentication required")
        .build()]);

    // When
    let report =
        waiting_for_readiness_of("explorer@ws-01", A_DEADLINE_NO_HEALTHY_WAIT_REACHES).await;

    // Then
    report
        .assert_timed_out(false)
        .assert_reports("explorer@ws-01", "error");
}

/// A detach under a parked wait settles it: what it is waiting for cannot happen once the roster
/// has stopped carrying the row. The report says so by not listing the agent, which is a better
/// answer than an error — every other row survives it.
#[tokio::test(start_paused = true)]
#[serial]
async fn stops_waiting_on_an_agent_detached_underneath_it() {
    // Given
    publish(vec![
        an_agent("explorer@ws-01")
            .still_provisioning_its_checkout()
            .build(),
        an_agent("librarian@ws-02").build(),
    ]);
    tokio::spawn(async {
        tokio::time::sleep(WHILE_THE_CALL_IS_PARKED).await;
        publish(vec![an_agent("librarian@ws-02").build()]);
    });

    // When
    let report =
        waiting_for_readiness_of("explorer@ws-01", A_DEADLINE_NO_HEALTHY_WAIT_REACHES).await;

    // Then
    report
        .assert_timed_out(false)
        .assert_lists_no_agent("explorer@ws-01")
        .assert_reports("librarian@ws-02", "idle");
}

// ---------------------------------------------------------------------------
// AC70 — the deadline, and its ceiling
// ---------------------------------------------------------------------------

/// Expiry is not an error: the last known status is the actionable half of the answer, and an error
/// throws it away. "Still connecting" is something the main agent can act on.
#[tokio::test(start_paused = true)]
#[serial]
async fn reports_the_current_status_with_timed_out_when_the_deadline_passes() {
    // Given
    publish(vec![an_agent("explorer@ws-01")
        .still_provisioning_its_checkout()
        .build()]);

    // When
    let report = waiting_for_readiness_of("explorer@ws-01", A_DEADLINE_THAT_PASSES).await;

    // Then
    report
        .assert_timed_out(true)
        .assert_reports("explorer@ws-01", "connecting");
}

/// A `timeoutMs` the caller picked out of the air must not park the main agent for as long as it
/// says. Measured on the runtime's virtual clock, so this asserts the deadline actually honoured
/// rather than wall-clock patience.
#[tokio::test(start_paused = true)]
#[serial]
async fn parks_no_longer_than_the_cap_however_large_the_requested_timeout_is() {
    // Given
    publish(vec![an_agent("explorer@ws-01")
        .still_provisioning_its_checkout()
        .build()]);
    let started = tokio::time::Instant::now();

    // When
    let report = waiting_for_readiness_of("explorer@ws-01", A_DEADLINE_LONGER_THAN_THE_CAP).await;

    // Then
    assert_eq!(started.elapsed(), THE_LONGEST_ANY_WAIT_MAY_PARK);
    report.assert_timed_out(true);
}

// ---------------------------------------------------------------------------
// AC71 — what the wait refuses to guess
// ---------------------------------------------------------------------------

/// "Ready" across an unbounded roster is neither "all of them" nor "any of them" in a way the caller
/// could have meant, and guessing either would make the answer depend on attach order — which the
/// main agent cannot see.
#[tokio::test(start_paused = true)]
#[serial]
async fn refuses_to_wait_when_no_agent_is_named() {
    // Given
    publish(vec![an_agent("explorer@ws-01")
        .still_provisioning_its_checkout()
        .build()]);

    // When
    let report = subagent_status(json!({"waitFor": "ready"})).await;

    // Then
    report.assert_refused_naming("agent");
}

/// A condition the tool cannot detect is refused naming what it takes, rather than read as "ready":
/// a caller that asked to wait for a turn to end and was told the agent is promptable has been
/// answered a question it did not ask.
#[tokio::test(start_paused = true)]
#[serial]
async fn refuses_a_wait_for_condition_it_does_not_recognise() {
    // Given
    publish(vec![an_agent("explorer@ws-01").build()]);

    // When
    let report = subagent_status(json!({"agent": "explorer@ws-01", "waitFor": "idle"})).await;

    // Then
    report.assert_refused_naming("ready");
}

/// A `timeoutMs` that is not a whole number of milliseconds is refused rather than quietly replaced
/// by the default: a caller that silently got a different deadline reads the eventual timeout as
/// the one it set.
#[tokio::test(start_paused = true)]
#[serial]
async fn refuses_a_timeout_that_is_not_a_whole_number_of_milliseconds() {
    // Given
    publish(vec![an_agent("explorer@ws-01").build()]);

    // When
    let report = subagent_status(json!({
        "agent": "explorer@ws-01",
        "waitFor": "ready",
        "timeoutMs": -1,
    }))
    .await;

    // Then
    report.assert_refused_naming("timeoutMs");
}

/// An id no row has ever borne is a caller error, not a roster state, and it is refused the way
/// `subagent_new_session` refuses one. Settling it silently would answer a misspelled agent with
/// `timedOut: false` — indistinguishable from "it is ready", which is the one reading that would
/// send the main agent on to prompt an agent that does not exist.
#[tokio::test(start_paused = true)]
#[serial]
async fn refuses_to_wait_on_an_agent_that_was_never_attached() {
    // Given
    publish(vec![an_agent("librarian@ws-02").build()]);

    // When
    let report =
        waiting_for_readiness_of("explorer@ws-01", A_DEADLINE_NO_HEALTHY_WAIT_REACHES).await;

    // Then
    report
        .assert_refused_naming("explorer@ws-01")
        .assert_refused_naming("librarian@ws-02");
}

// ---------------------------------------------------------------------------
// The plain read is untouched by all of this
// ---------------------------------------------------------------------------

/// A read that asked for no wait carries no `timedOut`. A field that is always `false` on a call
/// that could not time out reads as a guarantee about a wait that never happened.
#[tokio::test]
#[serial]
async fn carries_no_timed_out_when_no_wait_was_asked_for() {
    // Given
    publish(vec![an_agent("explorer@ws-01")
        .still_provisioning_its_checkout()
        .build()]);

    // When
    let report = subagent_status(json!({})).await;

    // Then
    assert_eq!(report["timedOut"], Value::Null);
    report.assert_reports("explorer@ws-01", "connecting");
}

/// And naming an agent without `waitFor` still reports the whole roster: `agent` is the wait's
/// target, not a filter, so a plain read cannot be narrowed by it accidentally.
#[tokio::test]
#[serial]
async fn reports_every_agent_when_one_is_named_without_a_wait() {
    // Given
    publish(vec![
        an_agent("explorer@ws-01").build(),
        an_agent("librarian@ws-02").mid_turn().build(),
    ]);

    // When
    let report = subagent_status(json!({"agent": "explorer@ws-01"})).await;

    // Then
    report
        .assert_reports("explorer@ws-01", "idle")
        .assert_reports("librarian@ws-02", "running");
}

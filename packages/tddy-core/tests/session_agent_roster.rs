//! Unit tests: a session's agent roster as persisted in `.session.yaml`
//! (`tddy_core::session_agent`, `tddy_core::session_metadata`).
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (AC4, AC10, AC11)
//!
//! Two things are pinned here and nowhere else. First, that an agent's qualified id
//! (`name@daemon_instance_id`) is a *round trip* — every consumer routes off the daemon part, so
//! an id that formats one way and parses another sends a prompt to the wrong host. Second, that a
//! roster survives the session file, because a roster is operator intent and a resume that
//! silently drops it starts the session as something else.

use tddy_core::session_agent::{AgentId, SessionAgentRecord};
use tddy_core::session_metadata::{
    read_session_metadata, write_session_metadata, SessionMetadata, SESSION_METADATA_FILENAME,
};

// ---------------------------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------------------------

/// A roster entry with every field populated and nothing surprising in it. Tests override only
/// what the scenario is about.
fn an_agent_record(agent_id: &str) -> SessionAgentRecord {
    let parsed = AgentId::parse(agent_id).expect("builder was given a well-formed agent id");
    SessionAgentRecord {
        agent_id: agent_id.to_string(),
        name: parsed.name,
        daemon_instance_id: parsed.daemon_instance_id,
        label: Some("Repo explorer".to_string()),
        model: "qwen2.5-coder:7b".to_string(),
        replaces: vec!["Grep".to_string(), "Glob".to_string()],
        tools: vec!["Read".to_string(), "Glob".to_string(), "Grep".to_string()],
        codebase_session_id: None,
    }
}

/// A session's metadata with an empty roster. Named fields only — every test that cares about a
/// value sets it, and a reader never has to guess which of twenty fields the scenario turns on.
fn a_session_metadata(session_id: &str) -> SessionMetadata {
    SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "proj-roster".to_string(),
        created_at: "2026-08-16T10:00:00Z".to_string(),
        updated_at: "2026-08-16T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktrees/roster".to_string()),
        pid: Some(4321),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some("claude-opus-5".to_string()),
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
    }
}

/// A session directory under a per-test temp root, kept alive by the returned guard.
fn a_session_dir() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let session_dir = tmp.path().join("sessions").join("sess-roster");
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    (tmp, session_dir)
}

// ---------------------------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------------------------

trait AgentIdAssertions {
    fn assert_names(&self, name: &str, daemon_instance_id: &str) -> &Self;
}

impl AgentIdAssertions for AgentId {
    fn assert_names(&self, name: &str, daemon_instance_id: &str) -> &Self {
        assert_eq!(self.name, name, "agent id parsed the wrong name");
        assert_eq!(
            self.daemon_instance_id, daemon_instance_id,
            "agent id parsed the wrong daemon — a prompt would be routed to the wrong host"
        );
        self
    }
}

trait RosterAssertions {
    fn assert_agent_ids(&self, expected: &[&str]) -> &Self;
}

impl RosterAssertions for Vec<SessionAgentRecord> {
    fn assert_agent_ids(&self, expected: &[&str]) -> &Self {
        let actual: Vec<&str> = self.iter().map(|a| a.agent_id.as_str()).collect();
        assert_eq!(actual, expected, "roster agent ids mismatch");
        self
    }
}

// ---------------------------------------------------------------------------------------------
// AC4 — a qualified id is a round trip
// ---------------------------------------------------------------------------------------------

/// The id the operator picks, the id stored in the roster, and the id the main agent types are the
/// same string, and it always parses back to the pair it was built from.
#[test]
fn formats_an_agent_id_that_parses_back_to_the_same_pair() {
    // Given
    let id = AgentId {
        name: "explorer".to_string(),
        daemon_instance_id: "ws-01".to_string(),
    };

    // When
    let qualified = id.qualified();
    let reparsed = AgentId::parse(&qualified).expect("a formatted id must parse");

    // Then
    assert_eq!(qualified, "explorer@ws-01");
    reparsed.assert_names("explorer", "ws-01");
}

/// A def whose own name contains `@` would produce an id that parses back as a different pair, so
/// it is refused where the id is built rather than accepted and mis-routed later.
#[test]
fn refuses_a_name_that_would_make_its_own_id_ambiguous() {
    // Given
    let id = AgentId {
        name: "explorer@rogue".to_string(),
        daemon_instance_id: "ws-01".to_string(),
    };

    // When
    let result = id.try_qualified();

    // Then
    let error = result.expect_err("a name containing '@' must not produce an id");
    assert!(
        error.to_string().contains('@'),
        "the error must say what about the name is unusable, was: {error}"
    );
}

/// A daemon id carrying its own `@` would mint `explorer@host@1`, which parses as no single pair at
/// all — so it is refused where it is minted rather than stored as an id nothing can read back.
#[test]
fn refuses_a_daemon_id_that_would_make_its_own_id_ambiguous() {
    // Given
    let id = AgentId {
        name: "explorer".to_string(),
        daemon_instance_id: "host@1".to_string(),
    };

    // When
    let result = id.try_qualified();

    // Then
    let error = result.expect_err("a daemon id containing '@' must not produce an id");
    assert!(
        error.to_string().contains("explorer@host@1"),
        "the error must name the id it refused, was: {error}"
    );
}

/// A pair with no name mints `@ws-01`, which `parse` refuses — so minting refuses it too, rather
/// than handing back a string the next reader rejects.
#[test]
fn refuses_a_pair_with_no_agent_name() {
    // Given
    let id = AgentId {
        name: String::new(),
        daemon_instance_id: "ws-01".to_string(),
    };

    // When
    let result = id.try_qualified();

    // Then
    let error = result.expect_err("a pair with no name must not produce an id");
    assert!(
        error.to_string().contains("@ws-01"),
        "the error must name the id it refused, was: {error}"
    );
}

/// A pair with no daemon mints `explorer@`, which points at nowhere — refused at the mint for the
/// same reason `parse` refuses it.
#[test]
fn refuses_a_pair_with_no_daemon() {
    // Given
    let id = AgentId {
        name: "explorer".to_string(),
        daemon_instance_id: String::new(),
    };

    // When
    let result = id.try_qualified();

    // Then
    let error = result.expect_err("a pair with no daemon must not produce an id");
    assert!(
        error.to_string().contains("explorer@"),
        "the error must name the id it refused, was: {error}"
    );
}

/// Every id minting succeeds on parses straight back to the pair it was minted from — the property
/// the mint exists to guarantee, checked on the shape that actually round-trips.
#[test]
fn mints_an_id_that_parses_back_to_the_pair_it_was_built_from() {
    // Given
    let id = AgentId {
        name: "explorer".to_string(),
        daemon_instance_id: "ws-01".to_string(),
    };

    // When
    let qualified = id
        .try_qualified()
        .expect("a well-formed pair must mint an id");

    // Then
    AgentId::parse(&qualified)
        .expect("a minted id must parse")
        .assert_names("explorer", "ws-01");
}

/// A bare name names no daemon, so there is nothing to route it to. There is deliberately no
/// "assume the local daemon" reading — that is the reading that silently picks the wrong host once
/// two daemons offer the same name.
#[test]
fn refuses_an_id_with_no_daemon_part() {
    // Given
    let unqualified = "explorer";

    // When
    let result = AgentId::parse(unqualified);

    // Then
    let error = result.expect_err("an unqualified id must be refused");
    assert!(
        error.to_string().contains("explorer"),
        "the error must name the id it refused, was: {error}"
    );
}

/// An id whose daemon part is empty is as unroutable as one with no `@` at all, and is refused for
/// the same reason rather than producing an entry pointing at nowhere.
#[test]
fn refuses_an_id_whose_daemon_part_is_empty() {
    // Given
    let missing_daemon = "explorer@";

    // When
    let result = AgentId::parse(missing_daemon);

    // Then
    result.expect_err("an id with an empty daemon part must be refused");
}

// ---------------------------------------------------------------------------------------------
// AC10 — the roster survives the session file
// ---------------------------------------------------------------------------------------------

/// A resume rebuilds the session from `.session.yaml`, so every field the roster routes and
/// enforces on has to come back — the qualified id, the owning daemon, the replaced set, and the
/// clone the entry is served from.
#[test]
fn round_trips_a_roster_through_the_session_file() {
    // Given
    let (_tmp, session_dir) = a_session_dir();
    let mut remote = an_agent_record("linter@ws-02");
    remote.codebase_session_id = Some("1780828020298-clone".to_string());
    let metadata = SessionMetadata {
        agents: vec![an_agent_record("explorer@ws-01"), remote],
        agents_rev: 7,
        ..a_session_metadata("sess-roster")
    };
    write_session_metadata(&session_dir, &metadata).expect("write metadata");

    // When
    let read = read_session_metadata(&session_dir).expect("read metadata");

    // Then
    read.agents
        .assert_agent_ids(&["explorer@ws-01", "linter@ws-02"]);
    assert_eq!(read.agents_rev, 7, "the roster revision must be restored");
    assert_eq!(read.agents[0].daemon_instance_id, "ws-01");
    assert_eq!(read.agents[0].replaces, vec!["Grep", "Glob"]);
    assert_eq!(
        read.agents[1].codebase_session_id.as_deref(),
        Some("1780828020298-clone"),
        "the clone serving a remote agent must be restored, or its teardown cannot find it"
    );
}

/// A session with no agents writes no roster key at all, so an ordinary session's file looks
/// exactly as it did before rosters existed.
#[test]
fn omits_the_roster_from_the_session_file_when_it_is_empty() {
    // Given
    let (_tmp, session_dir) = a_session_dir();
    let metadata = a_session_metadata("sess-roster");

    // When
    write_session_metadata(&session_dir, &metadata).expect("write metadata");

    // Then
    let yaml = std::fs::read_to_string(session_dir.join(SESSION_METADATA_FILENAME))
        .expect("read metadata file");
    assert!(
        !yaml.contains("agents:"),
        "an empty roster must not appear in the session file; got:\n{yaml}"
    );
}

// ---------------------------------------------------------------------------------------------
// AC11 — a pre-roster session file still loads
// ---------------------------------------------------------------------------------------------

/// `SessionMetadata` is `deny_unknown_fields`, so a file carrying the superseded
/// `specialized_agents` key would otherwise fail to parse outright — which reads to the daemon and
/// the web as "not a session" and drops a session whose agent is still running.
#[test]
fn reads_a_session_file_that_predates_the_roster() {
    // Given
    let yaml = r#"session_id: sess-legacy
project_id: proj-legacy
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
status: active
specialized_agents:
  - fastcontext
  - my-linter
"#;

    // When
    let metadata: SessionMetadata =
        serde_yaml::from_str(yaml).expect("a pre-roster session file must still parse");

    // Then
    assert!(
        metadata.agents.is_empty(),
        "a pre-roster session must load with no agents rather than guessing which daemon \
         its bare names belonged to"
    );
    assert_eq!(metadata.agents_rev, 0);
}

/// The superseded key is read and discarded, never written back — so the first rewrite of a
/// pre-roster session file leaves no trace of a field nothing consults.
#[test]
fn never_writes_the_superseded_agent_names_back_to_the_session_file() {
    // Given
    let (_tmp, session_dir) = a_session_dir();
    let yaml = r#"session_id: sess-legacy
project_id: proj-legacy
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
status: active
specialized_agents:
  - fastcontext
"#;
    let metadata: SessionMetadata = serde_yaml::from_str(yaml).expect("legacy YAML must parse");

    // When
    write_session_metadata(&session_dir, &metadata).expect("rewrite metadata");

    // Then
    let rewritten = std::fs::read_to_string(session_dir.join(SESSION_METADATA_FILENAME))
        .expect("read metadata file");
    assert!(
        !rewritten.contains("specialized_agents"),
        "the superseded key must not be written back; got:\n{rewritten}"
    );
}

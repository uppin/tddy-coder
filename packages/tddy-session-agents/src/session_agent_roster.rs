//! The session agent roster's store: which agents a live session has, at which revision, and how
//! both survive a restart.
//!
//! Product contract: docs/ft/daemon/session-agent-roster.md § The roster, § Attaching and
//! detaching. Module docs: packages/tddy-daemon/docs/session-agent-roster.md.
//!
//! Three properties this type exists to hold, each of them a bug somewhere else if it slips:
//!
//! - **`rev` is the staleness signal, so it only moves on a real change.** A no-op re-attach that
//!   bumped it would push a fresh snapshot to every subscriber for a change that did not happen,
//!   and an operator double-clicking *Add* is the ordinary way to reach that.
//! - **A subscriber's first frame is the current snapshot.** The consumer that matters is the
//!   in-jail `tddy-tools` registry, and a reconnecting registry that had to wait for the next
//!   attach would answer `subagent_new_session` from an empty roster until one came — which may be
//!   never.
//! - **Disk is the source of truth across a restart, `rev` included.** A fresh process that
//!   restarted the count at zero would hand a subscriber holding `rev: 3` a `rev: 1` it reads as
//!   stale and never refreshes from.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tddy_core::SessionAgentRecord;
use tddy_rpc::Status;
use tddy_service::proto::connection::{AgentCloneState, SessionAgentEntry, SessionAgentRoster};
use tokio::sync::broadcast;

use crate::session_agent_clone::SessionAgentCloneStore;
use crate::session_agent_status::{agent_status, AgentActivity, SessionAgentActivityStore};

/// How many published snapshots a subscriber may fall behind before the oldest are dropped.
///
/// Dropping is safe here in a way it would not be for a delta stream: every frame is a whole
/// roster, so a subscriber that missed the middle of a burst is brought fully current by the frame
/// it does receive. The capacity only bounds how much a stalled subscriber costs in memory.
const ROSTER_BROADCAST_CAPACITY: usize = 32;

/// Every session's roster this daemon serves, rebuilt from `.session.yaml` on first use.
///
/// Keyed by session id alone: an id names one session on one daemon, so two callers reaching the
/// same roster through different sessions bases would be the same session.
pub struct SessionAgentRosterStore {
    sessions: Mutex<HashMap<String, SessionRoster>>,
    /// The clones serving this daemon's remote agents.
    ///
    /// A roster entry's `clone_state` is *not* persisted with the entry: a checkout's readiness is
    /// a fact about a running peer, and a `rev` restored from disk claiming READY would have a
    /// prompt served from a checkout nothing has measured since the restart. So the record on disk
    /// names the clone, and its liveness is read from here at every snapshot.
    clones: Arc<SessionAgentCloneStore>,
    /// What each of those agents was last observed doing.
    ///
    /// Read at every snapshot for the same reason `clones` is, and *not* persisted for a stronger
    /// one: a status restored from disk would claim a turn is in flight in a process that never
    /// started one, and the main agent would wait for an answer nothing is producing.
    activity: Arc<SessionAgentActivityStore>,
}

/// The two live stores a snapshot reads an entry's state from, taken together so neither can be
/// consulted without the other — an entry carrying a fresh `clone_state` and a stale `status` is
/// two different accounts of one agent.
#[derive(Clone, Copy)]
pub struct SnapshotSources<'a> {
    pub clones: &'a SessionAgentCloneStore,
    pub activity: &'a SessionAgentActivityStore,
}

/// One session's roster, its revision, and the channel every subscriber reads it from.
struct SessionRoster {
    rev: u64,
    /// In attach order — detaching one leaves the rest where they were, because that order is the
    /// order the main agent is shown its agents in.
    agents: Vec<SessionAgentRecord>,
    publisher: broadcast::Sender<SessionAgentRoster>,
}

impl SessionRoster {
    /// This roster as the snapshot every read returns.
    fn snapshot(&self, session_id: &str, sources: SnapshotSources<'_>) -> SessionAgentRoster {
        SessionAgentRoster {
            session_id: session_id.to_string(),
            rev: self.rev,
            agents: self
                .agents
                .iter()
                .map(|record| roster_entry(record, session_id, sources))
                .collect(),
        }
    }
}

impl SessionAgentRosterStore {
    #[must_use]
    pub fn new(
        clones: Arc<SessionAgentCloneStore>,
        activity: Arc<SessionAgentActivityStore>,
    ) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            clones,
            activity,
        }
    }

    /// The activity store every snapshot reads, for the handlers that write to it.
    ///
    /// Handed out rather than mirrored by the caller so a recorded turn and the snapshot that
    /// reports it are the same map — a second store would publish a roster that disagreed with the
    /// conversation the daemon is actually running.
    #[must_use]
    pub fn activity(&self) -> &Arc<SessionAgentActivityStore> {
        &self.activity
    }

    /// Push the current snapshot to every subscriber without changing it.
    ///
    /// A clone finishing (or failing) moves what a reader sees without moving `rev` — the roster
    /// itself did not change, only the state of a checkout it names. A subscriber that only ever
    /// heard about `rev` changes would show `provisioning` until the next attach, which may never
    /// come (PRD § What attach does, step 2: "the entry is published with `clone_ready: false` and
    /// republished at `clone_ready: true`").
    pub fn republish(&self, session_id: &str, session_dir: &Path) -> Result<(), Status> {
        self.with_session(session_id, session_dir, |state, sources| {
            let _ = state.publisher.send(state.snapshot(session_id, sources));
            Ok(())
        })
    }

    /// The session's roster as it stands, loading it from `.session.yaml` if this process has not
    /// seen the session before.
    pub fn snapshot(
        &self,
        session_id: &str,
        session_dir: &Path,
    ) -> Result<SessionAgentRoster, Status> {
        self.with_session(session_id, session_dir, |state, sources| {
            Ok(state.snapshot(session_id, sources))
        })
    }

    /// Add one agent, or report the roster unchanged when it is already attached.
    ///
    /// Idempotent on `agent_id`: a second attach of the same id returns the current roster, does
    /// not bump `rev`, and publishes nothing.
    pub fn attach(
        &self,
        session_id: &str,
        session_dir: &Path,
        record: SessionAgentRecord,
    ) -> Result<SessionAgentRoster, Status> {
        self.with_session(session_id, session_dir, |state, sources| {
            if state.agents.iter().any(|a| a.agent_id == record.agent_id) {
                return Ok(state.snapshot(session_id, sources));
            }
            let mut next = state.agents.clone();
            next.push(record);
            state.commit(session_id, session_dir, next, sources)
        })
    }

    /// Remove one agent. An `agent_id` the roster does not hold is `NOT_FOUND`, never a silent
    /// success — a silent success tells an operator a tool was restored to the main agent when it
    /// never was.
    pub fn detach(
        &self,
        session_id: &str,
        session_dir: &Path,
        agent_id: &str,
    ) -> Result<SessionAgentRoster, Status> {
        self.with_session(session_id, session_dir, |state, sources| {
            if !state.agents.iter().any(|a| a.agent_id == agent_id) {
                return Err(Status::not_found(format!(
                    "agent '{agent_id}' is not attached to session '{session_id}'"
                )));
            }
            let next: Vec<SessionAgentRecord> = state
                .agents
                .iter()
                .filter(|a| a.agent_id != agent_id)
                .cloned()
                .collect();
            state.commit(session_id, session_dir, next, sources)
        })
    }

    /// The entry `agent_id` names, or `None` when the roster does not hold it.
    ///
    /// Reads the *persisted* record rather than the wire entry, because the callers that need it —
    /// conversation routing and teardown — route on the owning daemon and the clone, both of which
    /// the record carries verbatim.
    pub fn entry(
        &self,
        session_id: &str,
        session_dir: &Path,
        agent_id: &str,
    ) -> Result<Option<SessionAgentRecord>, Status> {
        self.with_session(session_id, session_dir, |state, _| {
            Ok(state
                .agents
                .iter()
                .find(|a| a.agent_id == agent_id)
                .cloned())
        })
    }

    /// Every agent in the roster owned by `daemon_instance_id`.
    ///
    /// What decides whether a detach tears a checkout down: the clone survives while another agent
    /// on that host still reads it.
    pub fn agents_owned_by(
        &self,
        session_id: &str,
        session_dir: &Path,
        daemon_instance_id: &str,
    ) -> Result<Vec<SessionAgentRecord>, Status> {
        self.with_session(session_id, session_dir, |state, _| {
            Ok(state
                .agents
                .iter()
                .filter(|a| a.daemon_instance_id == daemon_instance_id)
                .cloned()
                .collect())
        })
    }

    /// The current snapshot plus a receiver for every snapshot published after it.
    ///
    /// Both are taken under one lock so nothing published between reading the snapshot and
    /// subscribing is lost — which is what lets a caller emit the snapshot as its first frame and
    /// then forward the receiver verbatim.
    pub fn subscribe(
        &self,
        session_id: &str,
        session_dir: &Path,
    ) -> Result<(SessionAgentRoster, broadcast::Receiver<SessionAgentRoster>), Status> {
        self.with_session(session_id, session_dir, |state, sources| {
            Ok((
                state.snapshot(session_id, sources),
                state.publisher.subscribe(),
            ))
        })
    }

    /// Run `f` against the session's roster, loading it from disk on first use.
    fn with_session<T>(
        &self,
        session_id: &str,
        session_dir: &Path,
        f: impl FnOnce(&mut SessionRoster, SnapshotSources<'_>) -> Result<T, Status>,
    ) -> Result<T, Status> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if !sessions.contains_key(session_id) {
            sessions.insert(
                session_id.to_string(),
                load_roster(session_id, session_dir)?,
            );
        }
        let state = sessions
            .get_mut(session_id)
            .expect("the roster was just inserted under this id");
        f(
            state,
            SnapshotSources {
                clones: &self.clones,
                activity: &self.activity,
            },
        )
    }
}

impl SessionRoster {
    /// Persist `next`, then adopt it and publish the snapshot it produced.
    ///
    /// Persist-then-adopt, not the other way around: a write that fails must leave this process
    /// agreeing with the file it will be rebuilt from, or a restart silently undoes an attach the
    /// operator was told had succeeded.
    fn commit(
        &mut self,
        session_id: &str,
        session_dir: &Path,
        next: Vec<SessionAgentRecord>,
        sources: SnapshotSources<'_>,
    ) -> Result<SessionAgentRoster, Status> {
        let next_rev = self.rev + 1;
        persist_roster(session_dir, &next, next_rev)?;
        self.agents = next;
        self.rev = next_rev;
        let snapshot = self.snapshot(session_id, sources);
        // A send with no subscribers is not a failure: a session whose `tddy-tools` has not opened
        // the stream yet is the ordinary state during start.
        let _ = self.publisher.send(snapshot.clone());
        Ok(snapshot)
    }
}

/// Rebuild a session's roster from its `.session.yaml`, continuing `rev` from the persisted value.
///
/// A session whose metadata cannot be read is `NOT_FOUND` rather than an empty roster: an empty
/// roster is a legitimate answer, so returning one here would tell an in-jail `tddy-tools` that
/// every agent it was seeded with had been detached.
fn load_roster(session_id: &str, session_dir: &Path) -> Result<SessionRoster, Status> {
    let meta = tddy_core::read_session_metadata(session_dir).map_err(|e| {
        Status::not_found(format!(
            "session '{session_id}' has no readable metadata at {}: {e}",
            session_dir.display()
        ))
    })?;
    Ok(SessionRoster {
        rev: meta.agents_rev,
        agents: meta.agents,
        publisher: broadcast::channel(ROSTER_BROADCAST_CAPACITY).0,
    })
}

/// Write the roster into the session's `.session.yaml`.
///
/// Through [`tddy_core::write_session_metadata`], so the file is replaced by rename rather than
/// truncated in place: a session whose `.session.yaml` is empty is invisible to the daemon and the
/// web even while its agent process is alive
/// (docs/dev/1-WIP/2026-08-16-atomic-session-file-writes.md).
fn persist_roster(
    session_dir: &Path,
    agents: &[SessionAgentRecord],
    rev: u64,
) -> Result<(), Status> {
    let mut meta = tddy_core::read_session_metadata(session_dir).map_err(|e| {
        Status::internal(format!(
            "could not read {} to record the roster: {e}",
            session_dir.display()
        ))
    })?;
    meta.agents = agents.to_vec();
    meta.agents_rev = rev;
    tddy_core::write_session_metadata(session_dir, &meta).map_err(|e| {
        Status::internal(format!(
            "could not record the roster in {}: {e}",
            session_dir.display()
        ))
    })
}

/// One persisted record as the wire entry a consumer rebuilds its registry from.
///
/// A record naming no clone is a local agent: the owning daemon is the facilitating daemon, it works
/// the real worktree, and there is nothing to wait for. A record that does name one is a remote
/// agent, and its state is read from the clone store rather than from disk — a checkout's readiness
/// is a fact about a running peer, so persisting it would have a restarted daemon claim READY for a
/// mirror nothing has measured since.
///
/// A remote entry the store knows nothing about reports UNSPECIFIED rather than READY: that is the
/// shape of a roster restored from `.session.yaml` after a restart, where the entry survived and the
/// clone did not, and claiming a checkout is ready when nothing measured it is how a prompt gets
/// served from an empty tree.
///
/// `status` and `last_activity` are read from the second live store on the same terms, and for the
/// same reason: what an agent is doing is a fact about a running turn loop, so persisting it would
/// have a restarted daemon claim a turn is in flight in a process that never started one. See
/// [`crate::session_agent_status`] for the mapping.
fn roster_entry(
    record: &SessionAgentRecord,
    session_id: &str,
    sources: SnapshotSources<'_>,
) -> SessionAgentEntry {
    let (clone_state, clone_error) = match record.codebase_session_id {
        None => (AgentCloneState::Local, String::new()),
        Some(_) => match sources.clones.get(session_id, &record.daemon_instance_id) {
            Some(clone) => (clone.state, clone.error),
            None => (AgentCloneState::Unspecified, String::new()),
        },
    };
    // Both live signals, read together: the checkout decides whether the agent can serve a prompt
    // at all, and only once it can does what the conversation is doing mean anything.
    let activity = sources.activity.get(session_id, &record.agent_id);
    SessionAgentEntry {
        agent_id: record.agent_id.clone(),
        name: record.name.clone(),
        daemon_instance_id: record.daemon_instance_id.clone(),
        label: record.label.clone().unwrap_or_default(),
        model: record.model.clone(),
        replaces: record.replaces.clone(),
        tools: record.tools.clone(),
        codebase_session_id: record.codebase_session_id.clone().unwrap_or_default(),
        clone_state: clone_state as i32,
        clone_error,
        status: agent_status(clone_state, activity.as_ref()) as i32,
        // The last activity survives a state change rather than being cleared with it: what an idle
        // agent was last seen doing is the only useful thing to show on its row.
        last_activity: activity.as_ref().and_then(AgentActivity::to_proto),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_agent_status::ManagedAgentState;
    use tddy_service::proto::connection::SessionAgentStatus;

    use tddy_core::session_metadata::{
        write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
    };

    /// A session directory with a readable `.session.yaml` and no roster. The session id is the
    /// directory's basename, exactly as it is on disk.
    fn a_session_dir() -> (tempfile::TempDir, String, std::path::PathBuf) {
        let parent = tempfile::tempdir().expect("session tempdir");
        let session_id = "1780828020298-store".to_string();
        let dir = parent.path().join(&session_id);
        std::fs::create_dir_all(&dir).expect("create session dir");
        write_initial_tool_session_metadata(
            &dir,
            InitialToolSessionMetadataOpts {
                project_id: "project-under-store".to_string(),
                session_type: Some("claude-cli".to_string()),
                ..InitialToolSessionMetadataOpts::default()
            },
        )
        .expect("write session metadata");
        (parent, session_id, dir)
    }

    fn a_record(agent_id: &str) -> SessionAgentRecord {
        SessionAgentRecord {
            agent_id: agent_id.to_string(),
            name: agent_id.split('@').next().unwrap_or(agent_id).to_string(),
            daemon_instance_id: agent_id.split('@').nth(1).unwrap_or_default().to_string(),
            label: None,
            model: "qwen2.5-coder:7b".to_string(),
            replaces: Vec::new(),
            tools: Vec::new(),
            codebase_session_id: None,
        }
    }

    /// The live stores a snapshot reads, with nothing recorded in either — what a daemon that has
    /// just restarted has.
    fn nothing_observed_yet() -> (SessionAgentCloneStore, SessionAgentActivityStore) {
        (
            SessionAgentCloneStore::new(),
            SessionAgentActivityStore::new(),
        )
    }

    fn sources<'a>(
        clones: &'a SessionAgentCloneStore,
        activity: &'a SessionAgentActivityStore,
    ) -> SnapshotSources<'a> {
        SnapshotSources { clones, activity }
    }

    /// A remote agent's record: it names the checkout serving it, which is what makes its
    /// `clone_state` a question about a peer rather than about this file.
    fn a_remote_record(agent_id: &str, codebase_session_id: &str) -> SessionAgentRecord {
        SessionAgentRecord {
            codebase_session_id: Some(codebase_session_id.to_string()),
            ..a_record(agent_id)
        }
    }

    #[test]
    fn a_local_entry_reports_itself_as_needing_no_clone() {
        let (clones, activity) = nothing_observed_yet();

        let entry = roster_entry(
            &a_record("explorer@ws-01"),
            "session-1",
            sources(&clones, &activity),
        );

        assert_eq!(entry.clone_state, AgentCloneState::Local as i32);
        assert_eq!(entry.codebase_session_id, "");
    }

    #[test]
    fn a_remote_entry_whose_clone_this_process_never_built_is_not_reported_ready() {
        // Given a roster restored from `.session.yaml` — the entry survived a restart, the clone
        // did not
        let (clones, activity) = nothing_observed_yet();

        // When
        let entry = roster_entry(
            &a_remote_record("explorer@ws-01", "clone-1"),
            "session-1",
            sources(&clones, &activity),
        );

        // Then — claiming READY for a checkout nothing has measured is how a prompt gets served
        // from an empty tree
        assert_eq!(entry.clone_state, AgentCloneState::Unspecified as i32);
    }

    #[test]
    fn a_remote_entry_reports_the_state_its_owning_daemon_last_gave() {
        // Given
        let (clones, activity) = nothing_observed_yet();
        clones.claim("session-1", "ws-01", || "clone-1".to_string());
        clones
            .record_report(&crate::session_agent_clone::AgentCloneReport {
                session_id: "session-1".to_string(),
                daemon_instance_id: "ws-01".to_string(),
                codebase_session_id: "clone-1".to_string(),
                state: AgentCloneState::Ready,
                error: String::new(),
                worktree_path: None,
                divergences: Vec::new(),
            })
            .expect("the owning daemon reports on the clone this daemon asked for");

        // When
        let entry = roster_entry(
            &a_remote_record("explorer@ws-01", "clone-1"),
            "session-1",
            sources(&clones, &activity),
        );

        // Then
        assert_eq!(entry.clone_state, AgentCloneState::Ready as i32);
    }

    // ─── status and last activity ride the same snapshot ────────────────────────────────────────

    #[test]
    fn an_entry_nothing_has_been_observed_of_carries_no_status_and_no_activity() {
        // Given the shape of a roster restored after a restart
        let (clones, activity) = nothing_observed_yet();

        // When
        let entry = roster_entry(
            &a_record("explorer@ws-01"),
            "session-1",
            sources(&clones, &activity),
        );

        // Then — UNSPECIFIED is "this daemon has nothing to say", not "idle"
        assert_eq!(entry.status, SessionAgentStatus::Unspecified as i32);
        assert_eq!(entry.last_activity, None);
    }

    #[test]
    fn an_entry_carries_the_status_of_the_conversation_running_on_it() {
        // Given a turn in flight with this agent
        let (clones, activity) = nothing_observed_yet();
        activity.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::Prompting,
            "prompted: summarise the diff",
        );

        // When
        let entry = roster_entry(
            &a_record("explorer@ws-01"),
            "session-1",
            sources(&clones, &activity),
        );

        // Then
        assert_eq!(entry.status, SessionAgentStatus::Running as i32);
        let last = entry.last_activity.expect("a turn was observed");
        assert_eq!(last.summary, "prompted: summarise the diff");
        assert!(last.at_unix_ms > 0, "a summary needs a time behind it");
    }

    #[test]
    fn an_idle_entry_still_shows_what_it_was_last_seen_doing() {
        // Given an agent that has answered everything asked
        let (clones, activity) = nothing_observed_yet();
        activity.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::Open,
            "answered: 3 callers in src/",
        );

        // When
        let entry = roster_entry(
            &a_record("explorer@ws-01"),
            "session-1",
            sources(&clones, &activity),
        );

        // Then — the last activity is the only useful thing on an idle row
        assert_eq!(entry.status, SessionAgentStatus::Idle as i32);
        assert_eq!(
            entry.last_activity.map(|a| a.summary),
            Some("answered: 3 callers in src/".to_string())
        );
    }

    #[test]
    fn a_remote_entry_whose_clone_is_still_building_is_connecting_even_with_an_open_conversation() {
        // Given a clone mid-provision and a conversation that looks idle
        let (clones, activity) = nothing_observed_yet();
        clones.claim("session-1", "ws-01", || "clone-1".to_string());
        activity.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::Open,
            "attached",
        );

        // When
        let entry = roster_entry(
            &a_remote_record("explorer@ws-01", "clone-1"),
            "session-1",
            sources(&clones, &activity),
        );

        // Then — an agent whose checkout is not ready refuses prompts, so IDLE would offer an agent
        // that cannot answer
        assert_eq!(entry.status, SessionAgentStatus::Connecting as i32);
    }

    #[test]
    fn one_agents_turn_does_not_show_on_another_agents_row() {
        // Given two agents on one session and a turn in flight with only one of them
        let (clones, activity) = nothing_observed_yet();
        activity.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::Prompting,
            "prompted",
        );

        // When
        let other = roster_entry(
            &a_record("reviewer@ws-01"),
            "session-1",
            sources(&clones, &activity),
        );

        // Then
        assert_eq!(other.status, SessionAgentStatus::Unspecified as i32);
        assert_eq!(other.last_activity, None);
    }

    #[tokio::test]
    async fn a_second_store_over_the_same_directory_continues_the_revision() {
        // Given
        let (_parent, session_id, session_dir) = a_session_dir();
        let store = SessionAgentRosterStore::new(
            Arc::new(SessionAgentCloneStore::new()),
            Arc::new(SessionAgentActivityStore::new()),
        );
        store
            .attach(&session_id, &session_dir, a_record("explorer@ws-01"))
            .expect("attach explorer");

        // When — a fresh store is what a restarted daemon has
        let restarted = SessionAgentRosterStore::new(
            Arc::new(SessionAgentCloneStore::new()),
            Arc::new(SessionAgentActivityStore::new()),
        );
        let roster = restarted
            .snapshot(&session_id, &session_dir)
            .expect("snapshot after restart");

        // Then
        assert_eq!(roster.rev, 1);
        assert_eq!(roster.agents.len(), 1);
    }
}

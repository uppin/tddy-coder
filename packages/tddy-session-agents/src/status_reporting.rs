//! Recording what a roster agent is doing, and pushing the roster that says so.
//!
//! Free functions rather than methods on the service, because there are two callers and they are in
//! two crates: the nine handlers here, and `tddy-daemon`'s local-agent tool dispatch — the only
//! place this host sees an agent's own loop *enter* a tool call, which is the only place
//! EXECUTING_TOOL can be told from RUNNING. Two copies of the rule would let a badge set on one
//! path disagree with a badge set on the other.

use std::path::Path;

use crate::session_agent_roster::SessionAgentRosterStore;
use crate::session_agent_status::ManagedAgentState;

/// Record what a roster agent is doing, and push the roster that says so.
///
/// Recorded and republished together, never separately. A status change does not move `rev` — the
/// roster itself did not change, only what one of its agents is doing — so a subscriber that heard
/// about `rev` changes alone would show the state an agent was in when it was attached until the
/// next attach, which may never come. This is the same reason
/// [`SessionAgentRosterStore::republish`] exists for clone reports.
///
/// `hosts_a_clone_for_this_session` records nothing when true: the session belongs to a *peer*, the
/// roster naming that agent is on the daemon facilitating it, and a status recorded against a
/// session this host only holds a clone for is one nothing will ever read.
pub fn note_agent_activity(
    rosters: &SessionAgentRosterStore,
    hosts_a_clone_for_this_session: bool,
    session_id: &str,
    session_dir: &Path,
    agent_id: &str,
    state: ManagedAgentState,
    summary: impl AsRef<str>,
) {
    if hosts_a_clone_for_this_session {
        return;
    }
    rosters
        .activity()
        .record(session_id, agent_id, state, summary);
    republish_quietly(rosters, session_id, session_dir, agent_id);
}

/// Push the roster after a status change, and never fail the call that caused it.
///
/// Non-fatal on purpose: the status is a display signal, and failing the turn it decorates would
/// trade a stale badge for a broken conversation.
///
/// `republish` alone, deliberately — not the session-room broadcast. Both consumers that act on a
/// status follow `StreamSessionAgents`, which `republish` feeds: the roster pane and the in-jail
/// registry. The room broadcast exists for participants rebuilding a registry from whole snapshots,
/// and a status ticks on **every tool call** — putting a whole roster on the room for each one would
/// spend the room's bandwidth on a badge, and `rev` has not moved, so nothing those participants act
/// on has changed.
pub fn republish_quietly(
    rosters: &SessionAgentRosterStore,
    session_id: &str,
    session_dir: &Path,
    agent_id: &str,
) {
    if let Err(e) = rosters.republish(session_id, session_dir) {
        log::debug!(
            "could not republish the roster of session {session_id} after '{agent_id}' changed \
             status ({})",
            e.message()
        );
    }
}

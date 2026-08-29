//! The spawn seed: what the registry answers from before the first frame arrives.
//!
//! `TDDY_SUBAGENTS_JSON` and the session-tool transport are everything the jail knows at spawn. It
//! is deliberately the *weakest* of the registry's inputs — the first frame replaces all of it —
//! and it is kept apart from [`super::registry`] so that "what the environment claimed" can never
//! be mistaken for "what the roster says".

use std::sync::OnceLock;

use tddy_core::session_agent::AgentId;
use tddy_discovery::agent_def::SpecializedAgentDef;
use tddy_service::proto::connection::{AgentCloneState, SessionAgentEntry, SessionAgentStatus};

use crate::session_tool_client::{detect_session_tool_transport, SessionToolTransport};

use super::registry::LiveAgentRoster;

/// The id a seeded def is addressed by: qualified when the daemon that resolved it is known, bare
/// when it is not (see [`LiveAgentRoster::seeded_from`]). `None` for a name that cannot produce an
/// id parsing back to itself.
pub(super) fn seed_agent_id(name: &str, local_daemon_instance_id: &str) -> Option<String> {
    if local_daemon_instance_id.is_empty() {
        return (!name.is_empty()).then(|| name.to_string());
    }
    AgentId {
        name: name.to_string(),
        daemon_instance_id: local_daemon_instance_id.to_string(),
    }
    .try_qualified()
    .ok()
}

/// A seeded def as the roster entry it stands in for.
///
/// `clone_state` is `LOCAL`: the seed came from the facilitating daemon's own def sources, so there
/// is no clone and nothing to wait for.
pub(super) fn seed_entry(
    agent_id: &str,
    local_daemon_instance_id: &str,
    def: &SpecializedAgentDef,
) -> SessionAgentEntry {
    SessionAgentEntry {
        agent_id: agent_id.to_string(),
        name: def.name.clone(),
        daemon_instance_id: local_daemon_instance_id.to_string(),
        label: def.label.clone().unwrap_or_default(),
        model: def.model.clone(),
        replaces: def.replaces.clone(),
        tools: def
            .tools
            .iter()
            .map(|tool| tool.catalog_name().to_string())
            .collect(),
        codebase_session_id: String::new(),
        clone_state: AgentCloneState::Local as i32,
        clone_error: String::new(),
        // A seed says what the session was *started* with, not what any of it is doing: the status
        // is the facilitating daemon's to fill in from a live conversation, and a seed claiming
        // IDLE would have the registry show a reachable agent before anything had reached it.
        status: SessionAgentStatus::Unspecified as i32,
        last_activity: None,
    }
}

/// The session's roster for this process.
///
/// Process-wide because the registry outlives any one MCP `tools/call`: the stream task writes it
/// and every tool handler reads it, and a per-request registry is exactly the thing that used to
/// re-read a frozen env var on every call.
pub fn session_agent_roster() -> &'static LiveAgentRoster {
    static ROSTER: OnceLock<LiveAgentRoster> = OnceLock::new();
    ROSTER.get_or_init(|| {
        let transport = detect_session_tool_transport();
        LiveAgentRoster::seeded_from(
            &seed_session_id(transport.as_ref()),
            crate::server::seed_subagents_or_report(),
            &seed_daemon_instance_id(transport.as_ref()),
        )
    })
}

/// The session the roster belongs to, as the spawn environment names it.
fn seed_session_id(transport: Option<&SessionToolTransport>) -> String {
    match transport {
        Some(SessionToolTransport::DaemonHttp { session_id, .. })
        | Some(SessionToolTransport::LiveKit { session_id, .. }) => session_id.clone(),
        // The sandbox socket implies the session by the connection itself; the jail is still told
        // which session it is, for everything that has to name one.
        _ => crate::session_tool_client::session_id_from_env(),
    }
}

/// The daemon that resolved the seeded defs, when the jail was told which one that is.
///
/// TODO(session-agent-roster): a sandbox-IPC jail is told nothing about its facilitating daemon's
/// instance id, so its seeded ids stay bare until the first frame replaces them with qualified
/// ones. Exporting the id at spawn is a `tddy-daemon` change and belongs with the roster's
/// daemon-side tranche.
fn seed_daemon_instance_id(transport: Option<&SessionToolTransport>) -> String {
    match transport {
        Some(SessionToolTransport::DaemonHttp {
            daemon_instance_id, ..
        })
        | Some(SessionToolTransport::LiveKit {
            daemon_instance_id, ..
        }) => daemon_instance_id.clone(),
        _ => String::new(),
    }
}

//! The spawn seed: what the registry answers from before the first frame arrives.
//!
//! `TDDY_SUBAGENTS_JSON` and the session-tool transport are everything the jail knows at spawn. It
//! is deliberately the *weakest* of the registry's inputs — the first frame replaces all of it —
//! and it is kept apart from [`super::registry`] so that "what the environment claimed" can never
//! be mistaken for "what the roster says".
//!
//! Reading the variable is here too, and not in the crate that spawns the jail: `#unbundle` node 5
//! moved [`seed_subagents_or_report`] out of `tddy-tools`' `mcp_primitives` — it was the last edge
//! pointing back out of the MCP server into the roster — and it belongs beside the seed it feeds
//! rather than beside the router that happens to be the other caller.

use std::sync::OnceLock;

use tddy_core::spawn_env::env_non_empty;
use tddy_session_tool_client::{detect_session_tool_transport, SessionToolTransport};

use crate::agent_def::SpecializedAgentDef;

use super::registry::LiveAgentRoster;

/// Parse `TDDY_SUBAGENTS_JSON` (a JSON array of [`SpecializedAgentDef`] — see
/// docs/ft/coder/specialized-subagents.md) into the resolved specialized-agent defs for this
/// process. Empty when the env var is unset or blank: with no def there is no agent, since every
/// agent this process can address came from a def source someone wrote.
///
/// A value that is *set* and does not parse is an error, never an empty seed. `SpecializedAgentDef`
/// is `deny_unknown_fields`, so a `tddy-tools` older than the daemon that wrote the value parses
/// exactly this way — and an empty seed means no agent is attached and none of the withdrawn tools
/// are served by anyone, with nothing naming the variable that caused it.
///
/// The message carries serde's position, never the value: a def carries a provider credential.
pub fn subagents_from_env() -> Result<Vec<SpecializedAgentDef>, String> {
    let Some(json) = env_non_empty("TDDY_SUBAGENTS_JSON") else {
        return Ok(Vec::new());
    };
    serde_json::from_str::<Vec<SpecializedAgentDef>>(&json).map_err(|e| {
        format!(
            "TDDY_SUBAGENTS_JSON is set but does not parse as an array of agent defs: {e}. \
             This is what a tddy-tools older than the daemon that spawned it sees, and treating \
             it as 'no agents are attached' would silently un-withdraw every tool the session's \
             agents took over"
        )
    })
}

/// The spawn seed for the two lazy constructions that have no caller to refuse to — the MCP
/// server's router and the process-wide roster.
///
/// `--mcp` already refused to start on an unparseable value (see `tddy-tools`' `run_mcp_server`),
/// so reaching the error arm means a caller that never passed that gate. It is reported at `error`
/// naming the variable rather than passed off as a session nobody attached an agent to.
pub fn seed_subagents_or_report() -> Vec<SpecializedAgentDef> {
    subagents_from_env().unwrap_or_else(|e| {
        log::error!(target: "tddy_discovery::roster", "{e}");
        Vec::new()
    })
}

/// The session's roster for this process.
///
/// Process-wide because the registry outlives any one MCP `tools/call`: the stream task writes it
/// and every tool handler reads it, and a per-request registry is exactly the thing that used to
/// re-read a frozen env var on every call.
///
/// # Precondition: one session per process
///
/// This is a `OnceLock` seeded from the **spawn environment** on first touch, so the whole process
/// gets the roster of whichever session touched it first. That was sound where this code came from
/// — `tddy-tools` is one in-jail process serving exactly one session — but `tddy-discovery` is
/// linked by `tddy-daemon`, which serves **many sessions per process**. A second session calling
/// this in the same process would silently share the first session's roster: its agents, its
/// session id, its tool withdrawals. Any multi-session host needs a `LiveAgentRoster` per session
/// (`LiveAgentRoster::seeded_from` takes everything this function reads from the environment),
/// not this singleton. Nothing in `tddy-discovery` or `tddy-daemon` calls it today.
pub fn session_agent_roster() -> &'static LiveAgentRoster {
    static ROSTER: OnceLock<LiveAgentRoster> = OnceLock::new();
    ROSTER.get_or_init(|| {
        let transport = detect_session_tool_transport();
        LiveAgentRoster::seeded_from(
            &seed_session_id(transport.as_ref()),
            seed_subagents_or_report(),
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
        _ => tddy_session_tool_client::session_id_from_env(),
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

//! A session's agent roster: how one attached agent is addressed, and how it is persisted.
//!
//! See docs/ft/daemon/session-agent-roster.md. Two facts live here because every other package
//! depends on them being the same fact:
//!
//! - an agent is addressed as `name@daemon_instance_id`, never as a bare name, because a bare name
//!   is ambiguous the moment two daemons contribute a def called `explorer` — and the roster routes
//!   a prompt off the daemon part of the id it stored;
//! - a roster entry is persisted in `.session.yaml`, so a resume restores operator intent instead of
//!   re-deriving it from def sources that may have changed underneath the session.

use serde::{Deserialize, Serialize};

/// Separates an agent's name from the daemon that owns it in a qualified id.
const DAEMON_SEPARATOR: char = '@';

/// A roster agent's qualified id, split into the pair it addresses.
///
/// The qualified form is the *only* form: it is what the operator picks, what the roster stores,
/// what the main agent types into `subagent_new_session`, and what an error message names. Keeping
/// one string for all of those is what makes an entry self-routing — the daemon to forward a prompt
/// to is read off the id rather than resolved again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentId {
    /// The def's own name, e.g. `explorer`. Never contains [`DAEMON_SEPARATOR`].
    pub name: String,
    /// The daemon whose def sources resolved the agent, e.g. `ws-01`. The same identity that
    /// appears in `daemon-{instance_id}` participant names.
    pub daemon_instance_id: String,
}

/// Why a string is not a usable agent id.
///
/// Every variant names the offending value: an id that cannot be parsed is reported to an operator
/// who typed it, and "invalid agent id" without the id is unactionable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentIdError {
    /// No `@` at all. There is deliberately no "assume the local daemon" reading — that reading is
    /// what silently picks the wrong host once two daemons offer the same name.
    #[error("agent id '{0}' names no daemon; expected 'name@daemon_instance_id'")]
    Unqualified(String),
    /// `@daemon` — nothing to resolve against the owning daemon's def sources.
    #[error("agent id '{0}' has no agent name before '@'")]
    EmptyName(String),
    /// `name@` — an entry pointing at nowhere, as unroutable as a bare name.
    #[error("agent id '{0}' has no daemon after '@'")]
    EmptyDaemon(String),
    /// More than one `@`, so the split into a pair is a guess. No valid pair formats to it.
    #[error("agent id '{0}' contains more than one '@', so it names no single daemon")]
    Ambiguous(String),
    /// A def whose own name contains `@` would format to an id that parses back as a different
    /// pair, so the id is refused where it is built rather than mis-routed later.
    #[error("agent name '{0}' contains '@', which would make its qualified id parse back as a different agent")]
    NameContainsSeparator(String),
}

impl AgentId {
    /// Splits a qualified id into its pair, refusing every string a valid pair cannot produce.
    pub fn parse(id: &str) -> Result<Self, AgentIdError> {
        let Some((name, daemon_instance_id)) = id.split_once(DAEMON_SEPARATOR) else {
            return Err(AgentIdError::Unqualified(id.to_string()));
        };
        if name.is_empty() {
            return Err(AgentIdError::EmptyName(id.to_string()));
        }
        if daemon_instance_id.is_empty() {
            return Err(AgentIdError::EmptyDaemon(id.to_string()));
        }
        if daemon_instance_id.contains(DAEMON_SEPARATOR) {
            return Err(AgentIdError::Ambiguous(id.to_string()));
        }
        Ok(Self {
            name: name.to_string(),
            daemon_instance_id: daemon_instance_id.to_string(),
        })
    }

    /// The `name@daemon_instance_id` string this pair is addressed by.
    ///
    /// Infallible, so call sites that already hold a parsed id read as one expression. A pair built
    /// by hand from a name that has not been checked goes through [`Self::try_qualified`] instead.
    #[must_use]
    pub fn qualified(&self) -> String {
        format!("{}{DAEMON_SEPARATOR}{}", self.name, self.daemon_instance_id)
    }

    /// The qualified id, or the reason this pair cannot produce one that parses back to it.
    ///
    /// This is the guard at the point an id is *minted* from a def — resolution refuses a def whose
    /// name contains `@` here rather than letting the roster store an id that routes elsewhere.
    ///
    /// The minted string is run back through [`Self::parse`], so the two cannot disagree by
    /// construction: every shape `parse` refuses (an empty name, an empty daemon, a daemon carrying
    /// its own `@`) is refused here with the same error, rather than minted and then rejected by the
    /// next reader. A pair built from an env-derived daemon id is exactly how such a shape arrives.
    pub fn try_qualified(&self) -> Result<String, AgentIdError> {
        if self.name.contains(DAEMON_SEPARATOR) {
            return Err(AgentIdError::NameContainsSeparator(self.name.clone()));
        }
        let qualified = self.qualified();
        Self::parse(&qualified)?;
        Ok(qualified)
    }
}

/// One roster entry as persisted in `.session.yaml`.
///
/// `replaces` and `tools` are snapshotted from the def **at attach**, not re-read on every use:
/// editing a YAML def or a registry assistant afterwards would otherwise silently change what a
/// running session's main agent is allowed to call. Detaching and re-attaching is the explicit way
/// to pick an edit up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAgentRecord {
    /// `name@daemon_instance_id` — the id the main agent addresses and the entry's identity.
    pub agent_id: String,
    /// The def's own name, as its owning daemon knows it.
    pub name: String,
    /// The daemon that resolved the def and runs the agent's turn loop.
    pub daemon_instance_id: String,
    /// Display only, from the def.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Display only, from the def.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// Exec-catalog tools this agent takes over from the main agent. The union across the roster is
    /// what the main agent loses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaces: Vec<String>,
    /// Exec-catalog tool names the agent's own loop may call. `Vec<String>`, not
    /// `Vec<SubagentTool>`: `tddy-discovery` depends on `tddy-core`, so the enum is not nameable
    /// here — and the wire carries strings anyway.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// The `workspace` session holding the owning daemon's clone. `None` when the owning daemon is
    /// the facilitating daemon — a local agent works the real worktree. Persisted because a detach
    /// after a restart has to be able to name the checkout it deletes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codebase_session_id: Option<String>,
}

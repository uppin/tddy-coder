//! `coding_agent.conversation`: how a session's own conversation is identified across its start and
//! its resumes (`#agent-tools 14/19`).
//!
//! Feature doc: `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md` (conversation slot,
//! written at wrap). Four kinds, written as `{ kind: <word> }`:
//!
//! - `daemon_session_id` — the daemon's session id is the conversation's (Claude Code:
//!   `--session-id ID` on a start, `--resume ID` on a resume); bound as the `session_id` template
//!   parameter.
//! - `minted` — a launch adapter mints an id before the first start (Cursor's `agent create-chat`,
//!   adapter `cursor-create-chat`), recorded on the session and bound as `chat_id` on every launch.
//!   The mint is bounded by `mint_deadline_secs` ([`DEFAULT_CONVERSATION_MINT_DEADLINE_SECS`] when
//!   absent): a mint that never returns refuses the start naming the agent and the deadline.
//! - `worktree` — the agent continues whatever its worktree holds (`--continue`: OpenCode, Muse).
//! - `resume_token` — the token `coding_agent.resume.adapter` read off the last run (Warp).
//!
//! An id-bound `daemon_session_id` conversation may declare a **rotation** (`#orchestrators` 6/11):
//! how the agent reports leaving its conversation for a fresh one inside the running session —
//! `/clear` in Claude Code — as two hook reports, each `{ hook: <event>, when: { <field>: <value> },
//! id_field: <field> }`:
//!
//! ```yaml
//! rotation:
//!   ends:   { hook: SessionEnd,   when: { reason: clear }, id_field: session_id }
//!   begins: { hook: SessionStart, when: { source: clear }, id_field: session_id }
//! ```
//!
//! A rotation happens only when an `ends` report names the session's **current** conversation and
//! the next `begins` report follows it: another agent sharing the session's hooks (a claude-cli
//! agent tab) ends a conversation of its own, so its clear rotates nothing (developer decision
//! 2026-11-13). The daemon appends the `begins` id to the session's `agent_conversations`, and a
//! resume binds the newest. Both hooks must be ones the spec's hooks adapter installs, or the spec
//! is refused at load.
//!
//! A spec that declares none is `resume_token` when it declares a `resume` slot and `worktree`
//! otherwise ([`crate::CodingAgentSpec::conversation`]) — what the generic launch did before the
//! slot existed, so the other agents' YAMLs need no line.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How long a `minted` conversation's adapter may take when the spec sets no
/// `mint_deadline_secs`.
pub const DEFAULT_CONVERSATION_MINT_DEADLINE_SECS: u64 = 30;

/// `coding_agent.conversation`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConversationSlot {
    /// The daemon's session id, bound as `session_id` — or, once the agent rotated its
    /// conversation, the newest id the session recorded.
    DaemonSessionId {
        /// How the agent reports a fresh conversation inside the running session; `None` for an
        /// agent that never rotates, or whose rotation nothing follows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rotation: Option<RotationSlot>,
    },
    /// An id the launch adapter `adapter` mints before the first start, bound as `chat_id`.
    Minted {
        /// A `tddy_agent_backend::adapters::LaunchAdapters` conversation-id adapter
        /// (`cursor-create-chat`). A name the build does not hold is refused at load.
        adapter: String,
        /// The mint's deadline in seconds; [`DEFAULT_CONVERSATION_MINT_DEADLINE_SECS`] when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mint_deadline_secs: Option<u64>,
    },
    /// The worktree's own last conversation (`--continue`).
    Worktree,
    /// The token `coding_agent.resume.adapter` read off the previous run.
    ResumeToken,
}

/// `coding_agent.conversation.rotation`: the report that ends the session's current conversation,
/// and the report of the fresh one that follows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationSlot {
    /// The report that the agent left a conversation (`SessionEnd` with `reason: clear`). It arms a
    /// rotation only when its id is the session's current conversation.
    pub ends: RotationHook,
    /// The report of the conversation that replaces it (`SessionStart` with `source: clear`). It
    /// rotates only when an `ends` for the current conversation armed it.
    pub begins: RotationHook,
}

/// One hook report of a [`RotationSlot`]: its event, the field values that make it this report,
/// and where its conversation id is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationHook {
    /// The hook event (`SessionEnd`, `SessionStart`). Refused at load when the spec's hooks adapter
    /// does not install it.
    pub hook: String,
    /// The event fields that must hold these values (`{ reason: clear }`); empty matches every
    /// report of `hook`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: BTreeMap<String, String>,
    /// The event field the conversation's id is in (`session_id`).
    pub id_field: String,
}

impl RotationHook {
    /// The conversation id a report of `hook_event_name` carrying `hook_fields` names, when it is
    /// this hook: the event is [`Self::hook`], every [`Self::when`] field holds its value and
    /// [`Self::id_field`] is a non-blank id. `None` for any other report.
    #[must_use]
    pub fn conversation_of(
        &self,
        hook_event_name: &str,
        hook_fields: &HashMap<String, String>,
    ) -> Option<String> {
        if hook_event_name != self.hook {
            return None;
        }
        let matches = self
            .when
            .iter()
            .all(|(field, value)| hook_fields.get(field) == Some(value));
        if !matches {
            return None;
        }
        hook_fields
            .get(&self.id_field)
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .map(str::to_string)
    }
}

/// Where a session's rotation stands between two hook reports: whether an `ends` for its current
/// conversation has been seen and not yet followed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RotationState {
    /// No rotation is under way.
    #[default]
    Idle,
    /// The current conversation ended; the next `begins` report is the conversation it rotated to.
    Armed,
}

/// What one hook report does to a session's rotation ([`RotationSlot::step`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RotationStep {
    /// The report leaves the rotation where it was: not one of the two hooks, or an `ends` of a
    /// conversation that is not the session's current one (another agent's clear).
    Unchanged,
    /// The session's current conversation ended: the next `begins` rotates.
    Arm,
    /// The session rotated to this conversation; the rotation is idle again.
    RotateTo(String),
}

impl RotationSlot {
    /// What a report of `hook_event_name` carrying `hook_fields` does, for a session whose current
    /// conversation is `current` and whose rotation is in `state`:
    ///
    /// - an [`Self::ends`] report naming `current` → [`RotationStep::Arm`];
    /// - a [`Self::begins`] report while [`RotationState::Armed`] → [`RotationStep::RotateTo`] its id;
    /// - anything else → [`RotationStep::Unchanged`] — an `ends` of another conversation, and a
    ///   `begins` nothing armed, among them.
    #[must_use]
    pub fn step(
        &self,
        state: RotationState,
        current: &str,
        hook_event_name: &str,
        hook_fields: &HashMap<String, String>,
    ) -> RotationStep {
        if let Some(ended) = self.ends.conversation_of(hook_event_name, hook_fields) {
            return if ended == current {
                RotationStep::Arm
            } else {
                RotationStep::Unchanged
            };
        }
        match (
            state,
            self.begins.conversation_of(hook_event_name, hook_fields),
        ) {
            (RotationState::Armed, Some(began)) => RotationStep::RotateTo(began),
            _ => RotationStep::Unchanged,
        }
    }

    /// The two hook events this rotation is reported by, which the spec's hooks adapter must
    /// install.
    #[must_use]
    pub fn hooks(&self) -> [&str; 2] {
        [self.ends.hook.as_str(), self.begins.hook.as_str()]
    }
}

impl ConversationSlot {
    /// The rotation this conversation declares, if any: only a `daemon_session_id` conversation
    /// binds an id a rotation can replace.
    #[must_use]
    pub fn rotation(&self) -> Option<&RotationSlot> {
        match self {
            Self::DaemonSessionId { rotation } => rotation.as_ref(),
            _ => None,
        }
    }

    /// How long a `minted` conversation's mint may take; `None` for every other kind, which mints
    /// nothing.
    pub fn mint_deadline(&self) -> Option<Duration> {
        match self {
            Self::Minted {
                mint_deadline_secs, ..
            } => Some(Duration::from_secs(
                mint_deadline_secs.unwrap_or(DEFAULT_CONVERSATION_MINT_DEADLINE_SECS),
            )),
            _ => None,
        }
    }
}

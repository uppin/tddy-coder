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
//! A spec that declares none is `resume_token` when it declares a `resume` slot and `worktree`
//! otherwise ([`crate::CodingAgentSpec::conversation`]) — what the generic launch did before the
//! slot existed, so the other agents' YAMLs need no line.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How long a `minted` conversation's adapter may take when the spec sets no
/// `mint_deadline_secs`.
pub const DEFAULT_CONVERSATION_MINT_DEADLINE_SECS: u64 = 30;

/// `coding_agent.conversation`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConversationSlot {
    /// The daemon's session id, bound as `session_id`.
    DaemonSessionId,
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

impl ConversationSlot {
    /// How long a `minted` conversation's mint may take; `None` for every other kind, which mints
    /// nothing.
    pub fn mint_deadline(&self) -> Option<Duration> {
        // TODO(cli-spec-features): implement — `mint_deadline_secs`, else
        // DEFAULT_CONVERSATION_MINT_DEADLINE_SECS, for `Minted`; `None` otherwise.
        todo!("cli-spec-features: ConversationSlot::mint_deadline")
    }
}

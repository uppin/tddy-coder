//! Conversation-id adapters: what mints the id a session's conversation is known by before its
//! agent's first start (`#agent-tools 14/19`). Named by a spec's
//! `coding_agent.conversation: { kind: minted, adapter: … }` and held in [`super::LaunchAdapters`].
//!
//! `cursor-create-chat` runs `<binary> create-chat` in the session's worktree: Cursor prints a
//! **bare** chat id on stdout and exits 0. The mint is bounded: it runs at most
//! [`ConversationMint::deadline`] and is then killed, so a CLI that never answers cannot hang a
//! start.

use std::fmt;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;

/// [`CursorCreateChat`]'s name.
pub const CURSOR_CREATE_CHAT: &str = "cursor-create-chat";

/// One mint: the agent's resolved binary, the directory it runs in, the environment set on it on
/// top of the daemon's, and how long it may take.
#[derive(Debug, Clone, Copy)]
pub struct ConversationMint<'a> {
    pub binary: &'a Path,
    pub workdir: &'a Path,
    pub env: &'a [(String, String)],
    pub deadline: Duration,
}

/// Why a mint produced no conversation id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationMintError {
    /// The mint ran past its deadline and was killed.
    TimedOut { after: Duration },
    /// The mint could not be run, exited non-zero, or printed something that is not an id.
    Failed(String),
}

impl fmt::Display for ConversationMintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TimedOut { after } => {
                write!(
                    f,
                    "no conversation id within {after:?}; the mint was killed"
                )
            }
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ConversationMintError {}

/// How a launch mints the id its session's conversation is known by.
#[async_trait]
pub trait ConversationIdAdapter: Send + Sync + fmt::Debug {
    /// The name a spec's `coding_agent.conversation.adapter` names it by.
    fn name(&self) -> &str;

    /// Mint one conversation id, within `mint.deadline`.
    async fn mint(&self, mint: ConversationMint<'_>) -> Result<String, ConversationMintError>;
}

/// `<binary> create-chat`, its stdout a bare chat id (cursor-agent 2026.07.23).
#[derive(Debug, Clone, Copy, Default)]
pub struct CursorCreateChat;

#[async_trait]
impl ConversationIdAdapter for CursorCreateChat {
    fn name(&self) -> &str {
        CURSOR_CREATE_CHAT
    }

    async fn mint(&self, mint: ConversationMint<'_>) -> Result<String, ConversationMintError> {
        // TODO(cli-spec-features): implement — `<binary> create-chat` in `workdir` with `env`,
        // killed at `deadline` (TimedOut); a non-zero exit or a stdout that is not one bare word is
        // Failed (the rule `tddy_agent_launch::chat::parse_created_chat_id` holds today).
        let _ = mint;
        todo!("cli-spec-features: CursorCreateChat::mint")
    }
}

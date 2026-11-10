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

/// `create-chat` prints a **bare** chat id on stdout and exits 0 ("Create a new empty chat and
/// return its ID" — cursor-agent 2026.07.23). Anything else — no output, a banner, a prompt, an
/// error message — is not a chat id, and pinning it to the session would send every later resume
/// into a chat that does not exist.
pub fn parse_created_chat_id(stdout: &str) -> Result<String, String> {
    let chat_id = stdout.trim();
    if chat_id.is_empty() {
        return Err("printed no chat id on stdout".to_string());
    }
    if chat_id.split_whitespace().count() > 1 {
        return Err(format!(
            "printed {chat_id:?} on stdout, which is not a bare chat id"
        ));
    }
    Ok(chat_id.to_string())
}

/// One mint: the agent's resolved binary, the directory it runs in, the whole environment it
/// is started from, and how long it may take.
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
        let command = format!("`{} create-chat`", mint.binary.display());
        let mut child = tokio::process::Command::new(mint.binary);
        child
            .arg("create-chat")
            .current_dir(mint.workdir)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        crate::agent_env::apply_agent_child_env(child.as_std_mut(), mint.env);
        // `output()` owns the child; dropping its future at the deadline kills it (`kill_on_drop`).
        let output = match tokio::time::timeout(mint.deadline, child.output()).await {
            Err(_) => {
                return Err(ConversationMintError::TimedOut {
                    after: mint.deadline,
                })
            }
            Ok(Err(e)) => {
                return Err(ConversationMintError::Failed(format!(
                    "{command} could not be run: {e}"
                )))
            }
            Ok(Ok(output)) => output,
        };
        if !output.status.success() {
            return Err(ConversationMintError::Failed(format!(
                "{command} exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        parse_created_chat_id(&String::from_utf8_lossy(&output.stdout))
            .map_err(|e| ConversationMintError::Failed(format!("{command} {e}")))
    }
}

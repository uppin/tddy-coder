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

/// What the target's own agent printed and how it ended, when [`AgentOnTarget`] ran it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetAgentRun {
    /// The agent's exit status; `None` when it was ended by a signal.
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// A target's own agent, run through the target's shell: in the session's worktree there, under the
/// target's own login environment — its `HOME`, `PATH` and `TMPDIR` — and the agent found by its
/// bare name, never by a path of the daemon's host. What a mint on a target is run through.
#[async_trait]
pub trait AgentOnTarget: Send + Sync {
    /// Run the target's agent with `args`, bounded by `deadline`. `Err` is the reason the target's
    /// shell could not run it (unreachable, no answer in time).
    async fn run(&self, args: &[&str], deadline: Duration) -> Result<TargetAgentRun, String>;
}

/// How a launch mints the id its session's conversation is known by.
#[async_trait]
pub trait ConversationIdAdapter: Send + Sync + fmt::Debug {
    /// The name a spec's `coding_agent.conversation.adapter` names it by.
    fn name(&self) -> &str;

    /// Mint one conversation id, within `mint.deadline`.
    async fn mint(&self, mint: ConversationMint<'_>) -> Result<String, ConversationMintError>;

    /// Mint one conversation id through a target's shell, within `deadline`: the agent runs
    /// where the session's worktree is, never on this host.
    async fn mint_on_target(
        &self,
        agent: &dyn AgentOnTarget,
        deadline: Duration,
    ) -> Result<String, ConversationMintError>;
}

/// `<binary> create-chat`, its stdout a bare chat id (cursor-agent 2026.07.23).
#[derive(Debug, Clone, Copy, Default)]
pub struct CursorCreateChat;

#[async_trait]
impl ConversationIdAdapter for CursorCreateChat {
    fn name(&self) -> &str {
        CURSOR_CREATE_CHAT
    }

    async fn mint_on_target(
        &self,
        _agent: &dyn AgentOnTarget,
        _deadline: Duration,
    ) -> Result<String, ConversationMintError> {
        // TODO(target-agent-start): implement
        unimplemented!("TODO(target-agent-start): mint through the target's shell")
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

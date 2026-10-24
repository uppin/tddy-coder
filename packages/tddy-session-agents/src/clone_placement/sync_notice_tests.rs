//! What a coding agent's turn tells the agent about the caller's files it merged in before the turn.
//!
//! The agent is sent its prompt through a session, so these tests observe the prompt that session
//! received: a recording fake stands in for the agent, and the conversation worktree is a real git
//! checkout of a temporary session worktree.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{
    PromptOutcome, StopReason, SubagentError, SubagentSession, TurnRequest,
};

use super::{open_conversation_worktree, CodingAgentConversationTurns};

const SESSION_ID: &str = "1780828020298-notice";
const CONVERSATION_ID: &str = "conv-notice";

/// A coding agent that records the prompt of every turn it is sent and answers each one at once.
struct RecordingAgent {
    prompts: Arc<Mutex<Vec<Option<String>>>>,
}

#[async_trait]
impl SubagentSession for RecordingAgent {
    async fn take_turn(&mut self, request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        self.prompts
            .lock()
            .expect("prompts")
            .push(request.prompt_text().map(str::to_string));
        Ok(PromptOutcome::new(
            StopReason::EndTurn,
            Vec::new(),
            TokenUsage::default(),
        ))
    }

    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError> {
        self.take_turn(TurnRequest::prompting(text)).await
    }

    fn model(&self) -> &str {
        "recording-model"
    }

    fn cumulative_usage(&self) -> TokenUsage {
        TokenUsage::default()
    }

    fn context_tokens(&self) -> u64 {
        0
    }

    fn tail(&self, _max_messages: usize) -> Vec<String> {
        Vec::new()
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A session worktree with one committed file, and the directory that holds it.
struct ASessionWorktree {
    _root: tempfile::TempDir,
    path: PathBuf,
}

fn a_session_worktree() -> ASessionWorktree {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("worktree");
    std::fs::create_dir_all(&path).expect("worktree dir");
    git(&path, &["init", "-q", "-b", "main"]);
    std::fs::write(path.join("lib.rs"), "fn main() {}\n").expect("seed file");
    git(&path, &["add", "."]);
    git(
        &path,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    );
    ASessionWorktree { _root: root, path }
}

/// The turns of a coding agent running on the conversation worktree of `session`, and the prompts
/// the agent was sent.
struct AConversation {
    turns: CodingAgentConversationTurns,
    prompts: Arc<Mutex<Vec<Option<String>>>>,
}

async fn a_conversation_on(session: &ASessionWorktree) -> AConversation {
    let worktree = open_conversation_worktree(&session.path, SESSION_ID, CONVERSATION_ID)
        .await
        .expect("the conversation's worktree");
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let agent = RecordingAgent {
        prompts: Arc::clone(&prompts),
    };
    AConversation {
        turns: CodingAgentConversationTurns::new(Box::new(agent), worktree),
        prompts,
    }
}

impl AConversation {
    async fn prompt(&mut self, text: &str) {
        self.turns.prompt(text).await.expect("the turn is taken");
    }

    fn prompts_sent(&self) -> Vec<Option<String>> {
        self.prompts.lock().expect("prompts").clone()
    }
}

#[tokio::test]
async fn a_turn_after_a_caller_change_is_sent_a_notice_naming_the_changed_file() {
    // Given
    let session = a_session_worktree();
    let mut conversation = a_conversation_on(&session).await;
    conversation.prompt("first question").await;
    std::fs::write(
        session.path.join("lib.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .expect("the caller edits lib.rs");

    // When
    conversation.prompt("second question").await;

    // Then
    let prompts = conversation.prompts_sent();
    let second = prompts[1].as_deref().expect("the second turn has a prompt");
    let (notice, question) = second
        .rsplit_once("\n\n")
        .expect("the notice is followed by the question");
    assert_eq!(
        (
            question,
            notice.starts_with("The caller changed 1 file since your last turn"),
            notice.contains("lib.rs"),
        ),
        ("second question", true, true)
    );
}

#[tokio::test]
async fn a_turn_with_no_caller_change_is_sent_its_prompt_as_it_was_written() {
    // Given
    let session = a_session_worktree();
    let mut conversation = a_conversation_on(&session).await;
    conversation.prompt("first question").await;

    // When
    conversation.prompt("second question").await;

    // Then
    assert_eq!(
        conversation.prompts_sent(),
        vec![
            Some("first question".to_string()),
            Some("second question".to_string()),
        ]
    );
}

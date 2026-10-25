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
use tddy_subagent_worktree::ConversationWorktree;

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

/// A coding agent that records the prompt of every turn it is sent, as [`RecordingAgent`] does. It
/// can fail the first turn it is sent, and it can write a file of its worktree on every turn, as a
/// coding agent's edit would.
struct ScriptedAgent {
    prompts: Arc<Mutex<Vec<Option<String>>>>,
    fail_first_turn: bool,
    edit: Option<(PathBuf, String)>,
}

impl ScriptedAgent {
    /// An agent that answers every turn and edits nothing.
    fn new() -> Self {
        Self {
            prompts: Arc::new(Mutex::new(Vec::new())),
            fail_first_turn: false,
            edit: None,
        }
    }

    /// The agent fails the first turn it is sent, after recording its prompt.
    fn failing_its_first_turn(mut self) -> Self {
        self.fail_first_turn = true;
        self
    }

    /// The agent writes `contents` to `path` on every turn.
    fn editing(mut self, path: PathBuf, contents: &str) -> Self {
        self.edit = Some((path, contents.to_string()));
        self
    }
}

#[async_trait]
impl SubagentSession for ScriptedAgent {
    async fn take_turn(&mut self, request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        let turn = {
            let mut prompts = self.prompts.lock().expect("prompts");
            prompts.push(request.prompt_text().map(str::to_string));
            prompts.len()
        };
        if let Some((path, contents)) = &self.edit {
            std::fs::write(path, contents).expect("the agent edits its file");
        }
        if self.fail_first_turn && turn == 1 {
            return Err(SubagentError::from(
                "the agent failed its first turn".to_string(),
            ));
        }
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
        "scripted-model"
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

/// The conversation worktree of `session`, for a conversation run by an agent built from its path.
async fn conversation_worktree_of(session: &ASessionWorktree) -> ConversationWorktree {
    open_conversation_worktree(&session.path, SESSION_ID, CONVERSATION_ID)
        .await
        .expect("the conversation's worktree")
}

/// A conversation on `worktree`, run by `agent`.
fn a_conversation_running(worktree: ConversationWorktree, agent: ScriptedAgent) -> AConversation {
    let prompts = Arc::clone(&agent.prompts);
    AConversation {
        turns: CodingAgentConversationTurns::new(Box::new(agent), worktree),
        prompts,
    }
}

/// The caller edits `lib.rs` in its own worktree.
fn the_caller_writes_lib_rs(session: &ASessionWorktree, contents: &str) {
    std::fs::write(session.path.join("lib.rs"), contents).expect("the caller edits lib.rs");
}

impl AConversation {
    /// Take a turn of `request`, answering with what the turn answered.
    async fn take_turn(&mut self, request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        self.turns.take_turn(request).await
    }

    async fn prompt_that_fails(&mut self, text: &str) {
        assert!(
            self.turns.prompt(text).await.is_err(),
            "the agent's first turn fails"
        );
    }

    /// The text of `file` in the conversation worktree.
    fn worktree_file(&self, file: &str) -> String {
        std::fs::read_to_string(self.turns.worktree.root().join(file))
            .expect("the file is in the conversation worktree")
    }
}

#[tokio::test]
async fn a_merge_on_a_turn_with_no_prompt_sends_the_agent_no_prompt() {
    // Given
    let session = a_session_worktree();
    let mut conversation = a_conversation_on(&session).await;
    conversation.prompt("first question").await;
    the_caller_writes_lib_rs(&session, "fn main() { println!(\"hi\"); }\n");

    // When
    conversation
        .take_turn(TurnRequest::resuming())
        .await
        .expect("the turn is taken");

    // Then
    assert_eq!(
        conversation.prompts_sent(),
        vec![Some("first question".to_string()), None]
    );
}

#[tokio::test]
async fn a_notice_held_past_a_turn_with_no_prompt_rides_on_the_next_prompt_once() {
    // Given
    let session = a_session_worktree();
    let mut conversation = a_conversation_on(&session).await;
    conversation.prompt("first question").await;
    the_caller_writes_lib_rs(&session, "fn main() { println!(\"hi\"); }\n");
    conversation
        .take_turn(TurnRequest::resuming())
        .await
        .expect("the turn is taken");

    // When
    conversation.prompt("second question").await;
    conversation.prompt("third question").await;

    // Then
    let prompts = conversation.prompts_sent();
    let second = prompts[2].as_deref().expect("the second turn has a prompt");
    let (notice, question) = second
        .rsplit_once("\n\n")
        .expect("the notice is followed by the question");
    assert_eq!(
        (
            question,
            notice.starts_with("The caller changed 1 file since your last turn"),
            notice.contains("lib.rs"),
            prompts[3].as_deref(),
        ),
        ("second question", true, true, Some("third question"))
    );
}

#[tokio::test]
async fn a_notice_survives_a_prompt_turn_that_fails() {
    // Given
    let session = a_session_worktree();
    let mut conversation = a_conversation_running(
        conversation_worktree_of(&session).await,
        ScriptedAgent::new().failing_its_first_turn(),
    );
    the_caller_writes_lib_rs(&session, "fn main() { println!(\"hi\"); }\n");
    conversation.prompt_that_fails("first question").await;

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
async fn a_turn_that_skips_the_caller_sync_is_sent_no_notice_and_leaves_the_caller_change_out() {
    // Given
    let session = a_session_worktree();
    let mut conversation = a_conversation_on(&session).await;
    conversation.prompt("first question").await;
    the_caller_writes_lib_rs(&session, "fn main() { println!(\"hi\"); }\n");

    // When
    conversation
        .take_turn(TurnRequest::prompting("second question").without_sync())
        .await
        .expect("the turn is taken");

    // Then
    assert_eq!(
        (
            conversation.prompts_sent(),
            conversation.worktree_file("lib.rs"),
        ),
        (
            vec![
                Some("first question".to_string()),
                Some("second question".to_string()),
            ],
            "fn main() {}\n".to_string(),
        )
    );
}

#[tokio::test]
async fn a_turn_that_skips_the_caller_sync_is_not_refused_by_a_conflicting_caller_change() {
    // Given: the conversation's own edit to lib.rs is committed, unpulled, and the caller then
    // edits the same line differently
    let session = a_session_worktree();
    let worktree = conversation_worktree_of(&session).await;
    let agent = ScriptedAgent::new().editing(
        worktree.root().join("lib.rs"),
        "fn main() { /* the conversation */ }\n",
    );
    let mut conversation = a_conversation_running(worktree, agent);
    conversation.prompt("first question").await;
    the_caller_writes_lib_rs(&session, "fn main() { /* the caller */ }\n");

    // When
    conversation
        .take_turn(TurnRequest::prompting("second question").without_sync())
        .await
        .expect("a turn that skips the sync is taken");

    // Then
    assert_eq!(
        conversation.prompts_sent(),
        vec![
            Some("first question".to_string()),
            Some("second question".to_string()),
        ]
    );
}

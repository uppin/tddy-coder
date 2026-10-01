//! A rewind takes the conversation's worktree back with its transcript.
//!
//! The strongest assertion available here is *what reset was asked for, and when*: the port that
//! reaches the worktree is recorded, and the provider is a `wiremock` server whose request count says
//! whether the reset came before the resumed turn's first model call.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § A rewind takes the worktree back

use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, MessageId, MessageRole, PromptOutcome, ResetTarget, SubagentConfig,
    SubagentError, SubagentRegistry, SubagentSession, TurnRequest, WorktreeReset,
    WorktreeResetPort,
};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// What the recording port answers.
#[derive(Clone)]
enum PortAnswer {
    Reset(WorktreeReset),
    NoWorktree,
    Fails,
}

/// Records every reset asked of it, with how many model requests the provider had seen by then.
struct RecordingResets {
    asked: Mutex<Vec<(ResetTarget, usize)>>,
    answer: PortAnswer,
    provider: Arc<MockServer>,
}

#[async_trait]
impl WorktreeResetPort for RecordingResets {
    async fn reset(&self, target: ResetTarget) -> Result<Option<WorktreeReset>, SubagentError> {
        let seen = self
            .provider
            .received_requests()
            .await
            .expect("recording enabled")
            .len();
        self.asked.lock().unwrap().push((target, seen));
        match &self.answer {
            PortAnswer::Reset(reset) => Ok(Some(reset.clone())),
            PortAnswer::NoWorktree => Ok(None),
            PortAnswer::Fails => Err(SubagentError::from("the worktree is locked".to_string())),
        }
    }
}

fn a_def(base_url: &str) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "coder".to_string(),
        label: None,
        model: "fastcontext-tools-32k:latest".to_string(),
        base_url: base_url.to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::Write],
        max_turns: 3,
        replaces: Vec::new(),
        usage_notes: None,
    }
}

/// The daemon's side of two writes: the first committed as `c1`, the second as `c2`.
fn a_codebase_committing_each_write() -> CodebaseAccess {
    let calls = Arc::new(AtomicUsize::new(0));
    CodebaseAccess::managed(
        move |_tool: String,
              _args: serde_json::Value|
              -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
            Box::pin(async move {
                json!({
                    "bytes_written": 2,
                    "worktreeChange": {
                        "commit": format!("c{n}"),
                        "files": { "created": 1, "updated": 0, "removed": 0 },
                        "lines": { "added": 1, "removed": 0 }
                    }
                })
                .to_string()
            })
        },
    )
}

fn a_write_of(file: &str, call_id: &str) -> serde_json::Value {
    json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": format!("Writing {file}."),
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {
                        "name": "WRITE",
                        "arguments": json!({ "path": file, "contents": "x\n" }).to_string()
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    })
}

fn a_final_answer() -> serde_json::Value {
    json!({
        "choices": [{
            "message": { "role": "assistant", "content": "done" },
            "finish_reason": "stop"
        }]
    })
}

/// A conversation whose first turn wrote `a.txt` (commit `c1`) then `b.txt` (commit `c2`), with a
/// port that answers `answer`.
struct AConversationThatCommittedTwice {
    session: Box<dyn SubagentSession>,
    first_turn: PromptOutcome,
    resets: Arc<RecordingResets>,
    provider: Arc<MockServer>,
}

async fn a_conversation_that_committed_twice(
    answer: PortAnswer,
) -> AConversationThatCommittedTwice {
    let provider = Arc::new(MockServer::start().await);
    for response in [a_write_of("a.txt", "call_1"), a_write_of("b.txt", "call_2")] {
        Mock::given(method("POST"))
            .and(path_matcher("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .up_to_n_times(1)
            .mount(&provider)
            .await;
    }
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_final_answer()))
        .mount(&provider)
        .await;
    let resets = Arc::new(RecordingResets {
        asked: Mutex::new(Vec::new()),
        answer,
        provider: Arc::clone(&provider),
    });
    let mut session = SubagentRegistry::from_defs(vec![a_def(&provider.uri())])
        .create(
            "coder",
            SubagentConfig::new(a_codebase_committing_each_write())
                .with_worktree_reset(Arc::clone(&resets) as Arc<dyn WorktreeResetPort>),
        )
        .expect("the def must resolve");
    let first_turn = session
        .prompt("write both files")
        .await
        .expect("the first turn runs");
    AConversationThatCommittedTwice {
        session,
        first_turn,
        resets,
        provider,
    }
}

impl AConversationThatCommittedTwice {
    fn ids_of(&self, role: MessageRole) -> Vec<MessageId> {
        self.first_turn
            .messages
            .iter()
            .filter(|described| described.role == role)
            .map(|described| described.id.clone())
            .collect()
    }

    fn resets_asked(&self) -> Vec<ResetTarget> {
        self.resets
            .asked
            .lock()
            .unwrap()
            .iter()
            .map(|(target, _)| target.clone())
            .collect()
    }

    async fn requests_so_far(&self) -> usize {
        self.provider.received_requests().await.unwrap().len()
    }
}

fn a_reset_to(to: &str, dropped: &[&str]) -> WorktreeReset {
    WorktreeReset {
        to: to.to_string(),
        dropped_commits: dropped.iter().map(|c| c.to_string()).collect(),
    }
}

#[tokio::test]
async fn a_rewind_resets_to_the_commit_of_the_last_kept_tool_result() {
    // Given
    let mut conversation =
        a_conversation_that_committed_twice(PortAnswer::Reset(a_reset_to("c1", &["c2"]))).await;
    let first_result = conversation.ids_of(MessageRole::Tool)[0].clone();

    // When
    conversation
        .session
        .take_turn(TurnRequest::resuming().from_message(first_result))
        .await
        .expect("the resume runs");

    // Then
    assert_eq!(
        conversation.resets_asked(),
        vec![ResetTarget::Commit("c1".to_string())]
    );
}

#[tokio::test]
async fn a_rewind_to_the_call_that_made_a_commit_keeps_that_commit() {
    // Given — the second assistant message is the call that wrote b.txt; the cut extends over its
    // result, so its commit stays
    let mut conversation =
        a_conversation_that_committed_twice(PortAnswer::Reset(a_reset_to("c2", &[]))).await;
    let second_call = conversation.ids_of(MessageRole::Assistant)[1].clone();

    // When
    conversation
        .session
        .take_turn(TurnRequest::resuming().from_message(second_call))
        .await
        .expect("the resume runs");

    // Then
    assert_eq!(
        conversation.resets_asked(),
        vec![ResetTarget::Commit("c2".to_string())]
    );
}

#[tokio::test]
async fn a_rewind_before_any_commit_resets_to_the_base() {
    // Given
    let mut conversation =
        a_conversation_that_committed_twice(PortAnswer::Reset(a_reset_to("b0", &["c1", "c2"])))
            .await;
    let the_prompt = conversation.ids_of(MessageRole::User)[0].clone();

    // When
    conversation
        .session
        .take_turn(TurnRequest::resuming().from_message(the_prompt))
        .await
        .expect("the resume runs");

    // Then
    assert_eq!(conversation.resets_asked(), vec![ResetTarget::Base]);
}

#[tokio::test]
async fn a_rewind_that_keeps_the_worktree_asks_for_no_reset() {
    // Given
    let mut conversation =
        a_conversation_that_committed_twice(PortAnswer::Reset(a_reset_to("c1", &["c2"]))).await;
    let first_result = conversation.ids_of(MessageRole::Tool)[0].clone();

    // When
    let outcome = conversation
        .session
        .take_turn(
            TurnRequest::resuming()
                .from_message(first_result)
                .keeping_worktree(),
        )
        .await
        .expect("the resume runs");

    // Then
    assert_eq!(
        (conversation.resets_asked(), outcome.worktree_reset),
        (vec![], None)
    );
}

#[tokio::test]
async fn the_reset_happens_before_the_resumed_turn_asks_the_model_anything() {
    // Given
    let mut conversation =
        a_conversation_that_committed_twice(PortAnswer::Reset(a_reset_to("c1", &["c2"]))).await;
    let first_result = conversation.ids_of(MessageRole::Tool)[0].clone();
    let before_the_resume = conversation.requests_so_far().await;

    // When
    conversation
        .session
        .take_turn(TurnRequest::resuming().from_message(first_result))
        .await
        .expect("the resume runs");

    // Then
    let seen_at_reset: Vec<usize> = conversation
        .resets
        .asked
        .lock()
        .unwrap()
        .iter()
        .map(|(_, seen)| *seen)
        .collect();
    assert_eq!(seen_at_reset, vec![before_the_resume]);
}

#[tokio::test]
async fn the_outcome_reports_the_reset() {
    // Given
    let mut conversation =
        a_conversation_that_committed_twice(PortAnswer::Reset(a_reset_to("c1", &["c2"]))).await;
    let first_result = conversation.ids_of(MessageRole::Tool)[0].clone();

    // When
    let outcome = conversation
        .session
        .take_turn(TurnRequest::resuming().from_message(first_result))
        .await
        .expect("the resume runs");

    // Then
    assert_eq!(outcome.worktree_reset, Some(a_reset_to("c1", &["c2"])));
}

/// Guards the no-worktree answer: a port saying there is nothing to reset reports no reset.
#[tokio::test]
async fn a_conversation_without_a_worktree_reports_no_reset() {
    // Given
    let mut conversation = a_conversation_that_committed_twice(PortAnswer::NoWorktree).await;
    let first_result = conversation.ids_of(MessageRole::Tool)[0].clone();

    // When
    let outcome = conversation
        .session
        .take_turn(TurnRequest::resuming().from_message(first_result))
        .await
        .expect("the resume runs");

    // Then
    assert_eq!(outcome.worktree_reset, None);
}

#[tokio::test]
async fn a_failed_reset_refuses_the_resume_and_keeps_the_history() {
    // Given
    let mut conversation = a_conversation_that_committed_twice(PortAnswer::Fails).await;
    let first_result = conversation.ids_of(MessageRole::Tool)[0].clone();
    let refused = conversation
        .session
        .take_turn(TurnRequest::resuming().from_message(first_result))
        .await;

    // When — carry on without a rewind, so the model is sent the history as it now stands
    conversation
        .session
        .take_turn(TurnRequest::resuming())
        .await
        .expect("the conversation is still usable");

    // Then
    let requests = conversation.provider.received_requests().await.unwrap();
    let last_history: serde_json::Value =
        serde_json::from_slice(&requests.last().expect("a request").body).unwrap();
    let tool_call_ids: Vec<String> = last_history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|message| message["tool_call_id"].as_str().map(str::to_string))
        .collect();
    assert_eq!(
        (refused.is_err(), tool_call_ids),
        (true, vec!["call_1".to_string(), "call_2".to_string()])
    );
}

/// Guards the plain resume: nothing rewound, nothing reset.
#[tokio::test]
async fn a_resume_without_a_rewind_asks_for_no_reset() {
    // Given
    let mut conversation =
        a_conversation_that_committed_twice(PortAnswer::Reset(a_reset_to("c1", &["c2"]))).await;

    // When
    conversation
        .session
        .take_turn(TurnRequest::resuming())
        .await
        .expect("the resume runs");

    // Then
    assert_eq!(conversation.resets_asked(), Vec::<ResetTarget>::new());
}
